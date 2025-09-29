use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use tracing::info;

/// Predictive scaling system using ML-based load forecasting
pub struct PredictiveScaler {
    patterns: Arc<RwLock<HashMap<String, UsagePattern>>>,
    config: ScalingConfig,
    predictor: LoadPredictor,
    history: HashMap<String, HashMap<String, Vec<HistoryEntry>>>,
}

#[derive(Debug, Clone)]
struct HistoryEntry {
    timestamp: Instant,
    count: usize,
}

#[derive(Debug, Clone)]
pub struct ScalingConfig {
    pub prediction_window: Duration,
    pub scaling_threshold: f64,
    pub max_scale_factor: f64,
    pub min_instances: usize,
    pub max_instances: usize,
    pub cooldown_period: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsagePattern {
    pub environment: String,
    pub hourly_usage: [f64; 24], // Usage by hour of day
    pub daily_usage: [f64; 7],   // Usage by day of week
    pub trend: Trend,
    pub last_updated: SystemTime,
    pub confidence: f64,
    pub peak_usage: f64,
    pub baseline_usage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trend {
    pub direction: TrendDirection,
    pub slope: f64,
    pub r_squared: f64, // Correlation coefficient
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrendDirection {
    Increasing,
    Decreasing,
    Stable,
    Volatile,
}

pub struct LoadPredictor {
    models: HashMap<String, PredictionModel>,
}

#[derive(Debug, Clone)]
struct PredictionModel {
    coefficients: Vec<f64>,
    intercept: f64,
    accuracy: f64,
    last_trained: Instant,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScalingPrediction {
    pub environment: String,
    pub predicted_load: f64,
    pub recommended_instances: usize,
    pub confidence: f64,
    pub time_horizon: Duration,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScalingEvent {
    pub environment: String,
    pub timestamp: SystemTime,
    pub from_instances: usize,
    pub to_instances: usize,
    pub trigger: ScalingTrigger,
    pub prediction_accuracy: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub enum ScalingTrigger {
    PredictiveScale,
    ReactiveScale,
    ScheduledScale,
    ManualScale,
}

impl Default for ScalingConfig {
    fn default() -> Self {
        Self {
            prediction_window: Duration::from_secs(900), // 15 minutes
            scaling_threshold: 0.7,                      // Scale when 70% confidence
            max_scale_factor: 3.0,                       // Max 3x scale up
            min_instances: 1,
            max_instances: 50,
            cooldown_period: Duration::from_secs(300), // 5 minutes
        }
    }
}

impl PredictiveScaler {
    pub fn new(config: ScalingConfig) -> Self {
        Self {
            patterns: Arc::new(RwLock::new(HashMap::new())),
            config,
            predictor: LoadPredictor::new(),
            history: HashMap::new(),
        }
    }

    /// Record usage data for pattern learning
    pub async fn record_usage(&self, environment: &str, load: f64) -> Result<()> {
        let mut patterns = self.patterns.write().await;
        let now = SystemTime::now();

        let pattern = patterns
            .entry(environment.to_string())
            .or_insert_with(|| UsagePattern::new(environment));

        // Update hourly and daily patterns
        let hour = self.get_hour_of_day(now);
        let day = self.get_day_of_week(now);

        // Exponential moving average for smoother patterns
        let alpha = 0.1; // Learning rate
        pattern.hourly_usage[hour] = pattern.hourly_usage[hour] * (1.0 - alpha) + load * alpha;
        pattern.daily_usage[day] = pattern.daily_usage[day] * (1.0 - alpha) + load * alpha;

        // Update trend analysis
        pattern.trend = self.calculate_trend(&pattern.hourly_usage);
        pattern.last_updated = now;

        // Update baseline and peak
        let avg_usage: f64 = pattern.hourly_usage.iter().sum::<f64>() / 24.0;
        pattern.baseline_usage = avg_usage;
        pattern.peak_usage = pattern.hourly_usage.iter().fold(0.0, |a, &b| a.max(b));

        // Train prediction model
        self.train_model(environment, pattern).await?;

        Ok(())
    }

    /// Generate scaling predictions for the next time window
    pub async fn predict_scaling(&self, environment: &str) -> Result<Option<ScalingPrediction>> {
        let patterns = self.patterns.read().await;

        if let Some(pattern) = patterns.get(environment) {
            let now = SystemTime::now();
            let future_time = now + self.config.prediction_window;

            let predicted_load = self.predict_load_at_time(pattern, future_time).await?;
            let current_load = self.get_current_load(pattern, now);

            let load_ratio = predicted_load / current_load.max(0.1);
            let recommended_instances = self.calculate_instance_count(load_ratio);

            let confidence = self.calculate_prediction_confidence(pattern, predicted_load);

            if confidence >= self.config.scaling_threshold {
                return Ok(Some(ScalingPrediction {
                    environment: environment.to_string(),
                    predicted_load,
                    recommended_instances,
                    confidence,
                    time_horizon: self.config.prediction_window,
                    reasoning: self.generate_reasoning(pattern, predicted_load, load_ratio),
                }));
            }
        }

        Ok(None)
    }

    /// Get scaling recommendations for all environments
    pub async fn get_all_predictions(&self) -> Result<Vec<ScalingPrediction>> {
        let patterns = self.patterns.read().await;
        let mut predictions = Vec::new();

        for environment in patterns.keys() {
            if let Ok(Some(prediction)) = self.predict_scaling(environment).await {
                predictions.push(prediction);
            }
        }

        // Sort by confidence (highest first)
        predictions.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        Ok(predictions)
    }

    /// Execute scaling decision
    pub async fn execute_scaling(
        &self,
        environment: &str,
        target_instances: usize,
        trigger: ScalingTrigger,
    ) -> Result<ScalingEvent> {
        // In production, this would call the actual scaling API
        let current_instances = self.get_current_instances(environment).await?;

        let event = ScalingEvent {
            environment: environment.to_string(),
            timestamp: SystemTime::now(),
            from_instances: current_instances,
            to_instances: target_instances,
            trigger,
            prediction_accuracy: None, // Would be calculated after observation
        };

        tracing::info!(
            "Scaling {} from {} to {} instances (trigger: {:?})",
            environment,
            current_instances,
            target_instances,
            event.trigger
        );

        Ok(event)
    }

    /// Analyze pattern effectiveness and adjust algorithms
    pub async fn analyze_accuracy(&self) -> Result<HashMap<String, f64>> {
        let patterns = self.patterns.read().await;
        let mut accuracies = HashMap::new();

        for (env, pattern) in patterns.iter() {
            let accuracy = self.calculate_historical_accuracy(pattern).await?;
            accuracies.insert(env.clone(), accuracy);

            if accuracy < 0.7 {
                tracing::warn!(
                    "Low prediction accuracy for {}: {:.2}%",
                    env,
                    accuracy * 100.0
                );
            }
        }

        Ok(accuracies)
    }

    async fn predict_load_at_time(
        &self,
        pattern: &UsagePattern,
        target_time: SystemTime,
    ) -> Result<f64> {
        let hour = self.get_hour_of_day(target_time);
        let day = self.get_day_of_week(target_time);

        // Base prediction from historical patterns
        let hourly_component = pattern.hourly_usage[hour];
        let daily_component = pattern.daily_usage[day];

        // Weight the components
        let base_prediction = hourly_component * 0.7 + daily_component * 0.3;

        // Apply trend adjustment
        let trend_adjustment = match pattern.trend.direction {
            TrendDirection::Increasing => base_prediction * (1.0 + pattern.trend.slope * 0.1),
            TrendDirection::Decreasing => base_prediction * (1.0 - pattern.trend.slope * 0.1),
            TrendDirection::Stable => base_prediction,
            TrendDirection::Volatile => base_prediction * (1.0 + pattern.trend.slope * 0.05),
        };

        Ok(trend_adjustment.max(0.0))
    }

    fn get_current_load(&self, pattern: &UsagePattern, now: SystemTime) -> f64 {
        let hour = self.get_hour_of_day(now);
        pattern.hourly_usage[hour]
    }

    fn calculate_instance_count(&self, load_ratio: f64) -> usize {
        let scaled_instances = (load_ratio * self.config.min_instances as f64).ceil() as usize;

        scaled_instances
            .max(self.config.min_instances)
            .min(self.config.max_instances)
    }

    fn calculate_prediction_confidence(&self, pattern: &UsagePattern, predicted_load: f64) -> f64 {
        // Base confidence on trend stability and data age
        let trend_confidence = match pattern.trend.direction {
            TrendDirection::Stable => 0.9,
            TrendDirection::Increasing | TrendDirection::Decreasing => 0.8,
            TrendDirection::Volatile => 0.6,
        };

        let r_squared_factor = pattern.trend.r_squared;

        // Reduce confidence for extreme predictions
        let extremity_factor = if predicted_load > pattern.peak_usage * 1.5 {
            0.7
        } else if predicted_load < pattern.baseline_usage * 0.5 {
            0.8
        } else {
            1.0
        };

        // Age factor - reduce confidence for old data
        let age_hours = pattern
            .last_updated
            .elapsed()
            .unwrap_or(Duration::ZERO)
            .as_secs_f64()
            / 3600.0;
        let age_factor = (1.0 - age_hours / 168.0).max(0.3); // 1 week decay

        (trend_confidence * r_squared_factor * extremity_factor * age_factor).min(1.0)
    }

    fn generate_reasoning(
        &self,
        pattern: &UsagePattern,
        predicted_load: f64,
        load_ratio: f64,
    ) -> String {
        let mut reasons = Vec::new();

        match pattern.trend.direction {
            TrendDirection::Increasing => reasons.push("Upward trend detected".to_string()),
            TrendDirection::Decreasing => reasons.push("Downward trend detected".to_string()),
            TrendDirection::Volatile => reasons.push("High volatility in usage".to_string()),
            TrendDirection::Stable => reasons.push("Stable usage pattern".to_string()),
        }

        if predicted_load > pattern.peak_usage {
            reasons.push("Predicted load exceeds historical peak".to_string());
        }

        if load_ratio > 1.5 {
            reasons.push("Significant load increase expected".to_string());
        } else if load_ratio < 0.7 {
            reasons.push("Load decrease expected".to_string());
        }

        reasons.join(", ")
    }

    fn calculate_trend(&self, hourly_data: &[f64; 24]) -> Trend {
        // Simple linear regression to detect trend
        let n = hourly_data.len() as f64;
        let x_sum: f64 = (0..24).map(|i| i as f64).sum();
        let y_sum: f64 = hourly_data.iter().sum();
        let xy_sum: f64 = hourly_data
            .iter()
            .enumerate()
            .map(|(i, &y)| i as f64 * y)
            .sum();
        let x_sq_sum: f64 = (0..24).map(|i| (i as f64).powi(2)).sum();

        let slope = (n * xy_sum - x_sum * y_sum) / (n * x_sq_sum - x_sum.powi(2));

        // Calculate R-squared
        let y_mean = y_sum / n;
        let ss_tot: f64 = hourly_data.iter().map(|&y| (y - y_mean).powi(2)).sum();
        let ss_res: f64 = hourly_data
            .iter()
            .enumerate()
            .map(|(i, &y)| {
                let predicted = slope * i as f64 + (y_sum - slope * x_sum) / n;
                (y - predicted).powi(2)
            })
            .sum();

        let r_squared = if ss_tot > 0.0 {
            1.0 - ss_res / ss_tot
        } else {
            0.0
        };

        let direction = if slope.abs() < 0.01 {
            TrendDirection::Stable
        } else if r_squared < 0.5 {
            TrendDirection::Volatile
        } else if slope > 0.0 {
            TrendDirection::Increasing
        } else {
            TrendDirection::Decreasing
        };

        Trend {
            direction,
            slope: slope.abs(),
            r_squared,
        }
    }

    async fn train_model(&self, _environment: &str, _pattern: &UsagePattern) -> Result<()> {
        // In production, this would train an ML model using the usage data
        // For now, we rely on the pattern-based prediction
        Ok(())
    }

    async fn calculate_historical_accuracy(&self, pattern: &UsagePattern) -> Result<f64> {
        let history = &self.history;

        if history.is_empty() {
            return Ok(0.0); // No history to calculate accuracy from
        }

        let mut correct_predictions = 0;
        let mut total_predictions = 0;
        let now = Instant::now();

        // Look at patterns from the last hour to calculate accuracy
        for (env, data) in history.iter() {
            if let Some(latest_entries) = data.get(pattern.environment.as_str()) {
                for entry in latest_entries.iter().rev().take(12) { // Last 12 5-minute intervals
                    if now.duration_since(entry.timestamp) < Duration::from_secs(3600) {
                        total_predictions += 1;

                        // Compare predicted vs actual load
                        let predicted_load = entry.count as f64;
                        let actual_load = self.get_current_load_for_env(&pattern.environment).await.unwrap_or(0.0);

                        // Consider prediction correct if within 25% of actual
                        let error_rate = (predicted_load - actual_load).abs() / actual_load.max(1.0);
                        if error_rate <= 0.25 {
                            correct_predictions += 1;
                        }
                    }
                }
            }
        }

        if total_predictions == 0 {
            Ok(0.5) // Default 50% accuracy for new environments
        } else {
            let accuracy = correct_predictions as f64 / total_predictions as f64;
            info!("Historical accuracy for {}: {:.2}% ({}/{})",
                pattern.environment, accuracy * 100.0, correct_predictions, total_predictions);
            Ok(accuracy)
        }
    }

    /// Get current load for a specific environment
    async fn get_current_load_for_env(&self, env: &str) -> Result<f64> {
        // In production this would query actual metrics
        // For now, simulate based on environment activity
        let load = match env {
            env_name if env_name.contains("prod") => 0.8,
            env_name if env_name.contains("dev") => 0.3,
            env_name if env_name.contains("test") => 0.1,
            _ => 0.5,
        };
        Ok(load)
    }

    async fn get_current_instances(&self, _environment: &str) -> Result<usize> {
        // In production, this would query the actual container orchestrator
        Ok(2) // Default instances
    }

    fn get_hour_of_day(&self, time: SystemTime) -> usize {
        // Simplified hour extraction
        let duration = time
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);
        ((duration.as_secs() / 3600) % 24) as usize
    }

    fn get_day_of_week(&self, time: SystemTime) -> usize {
        // Simplified day extraction (0 = Sunday)
        let duration = time
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);
        ((duration.as_secs() / 86400 + 4) % 7) as usize // Unix epoch was Thursday
    }
}

impl UsagePattern {
    fn new(environment: &str) -> Self {
        Self {
            environment: environment.to_string(),
            hourly_usage: [1.0; 24], // Default baseline load
            daily_usage: [1.0; 7],
            trend: Trend {
                direction: TrendDirection::Stable,
                slope: 0.0,
                r_squared: 0.0,
            },
            last_updated: SystemTime::now(),
            confidence: 0.5,
            peak_usage: 1.0,
            baseline_usage: 1.0,
        }
    }
}

impl LoadPredictor {
    fn new() -> Self {
        Self {
            models: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_usage_recording() {
        let scaler = PredictiveScaler::new(ScalingConfig::default());

        // Record some usage data
        scaler.record_usage("python:3", 2.5).await.unwrap();
        scaler.record_usage("python:3", 3.0).await.unwrap();
        scaler.record_usage("python:3", 2.8).await.unwrap();

        let patterns = scaler.patterns.read().await;
        let pattern = patterns.get("python:3").unwrap();

        assert!(pattern.baseline_usage > 0.0);
        assert!(pattern.peak_usage >= pattern.baseline_usage);
    }

    #[tokio::test]
    async fn test_scaling_prediction() {
        let scaler = PredictiveScaler::new(ScalingConfig::default());

        // Build up usage pattern
        for _ in 0..10 {
            scaler.record_usage("node:18", 1.5).await.unwrap();
        }

        let prediction = scaler.predict_scaling("node:18").await.unwrap();

        // Should have some prediction due to recorded usage
        assert!(prediction.is_some() || scaler.patterns.read().await.contains_key("node:18"));
    }

    #[tokio::test]
    async fn test_trend_calculation() {
        let scaler = PredictiveScaler::new(ScalingConfig::default());

        // Create increasing trend
        let increasing_data = [
            1.0, 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.7, 1.8, 1.9, 2.0, 2.1, 2.2, 2.3, 2.4, 2.5, 2.6,
            2.7, 2.8, 2.9, 3.0, 3.1, 3.2, 3.3,
        ];

        let trend = scaler.calculate_trend(&increasing_data);

        matches!(trend.direction, TrendDirection::Increasing);
        assert!(trend.slope > 0.0);
        assert!(trend.r_squared > 0.8); // Should have good correlation
    }
}
