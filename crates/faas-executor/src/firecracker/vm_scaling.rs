//! Predictive VM Scaling with ML-based Load Forecasting
//! Provides intelligent VM pool management and auto-scaling

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::vm_fork::{VmForkManager, FirecrackerVmConfig};
use super::vm_snapshot::VmSnapshotManager;

/// Predictive VM Scaler with ML-based forecasting
pub struct VmPredictiveScaler {
    fork_manager: Arc<VmForkManager>,
    snapshot_manager: Arc<VmSnapshotManager>,
    pools: Arc<RwLock<HashMap<String, VmPool>>>,
    predictor: Arc<RwLock<LoadPredictor>>,
    config: ScalingConfig,
}

/// VM Pool for warm instances
pub struct VmPool {
    environment: String,
    warm_vms: VecDeque<WarmVm>,
    hot_vms: Vec<HotVm>,
    cold_snapshots: Vec<String>,
    metrics: PoolMetrics,
}

struct WarmVm {
    vm_id: String,
    fork_id: String,
    warmed_at: Instant,
    last_used: Option<Instant>,
}

struct HotVm {
    vm_id: String,
    fork_id: String,
    in_use: bool,
    last_execution: Instant,
    execution_count: u64,
}

#[derive(Default)]
struct PoolMetrics {
    hits: u64,
    misses: u64,
    cold_starts: u64,
    avg_wait_time: Duration,
    peak_concurrent: usize,
}

/// ML-based load predictor
struct LoadPredictor {
    history: HashMap<String, Vec<LoadDataPoint>>,
    models: HashMap<String, PredictionModel>,
    patterns: HashMap<String, LoadPattern>,
}

struct LoadDataPoint {
    timestamp: SystemTime,
    load: f64,
    concurrent_requests: usize,
    avg_execution_time: Duration,
}

struct PredictionModel {
    weights: Vec<f64>,
    bias: f64,
    accuracy: f64,
    last_trained: Instant,
}

#[derive(Debug, Clone)]
struct LoadPattern {
    hourly: [f64; 24],
    daily: [f64; 7],
    trend: TrendDirection,
    seasonality: f64,
}

#[derive(Debug, Clone)]
enum TrendDirection {
    Increasing,
    Decreasing,
    Stable,
    Volatile,
}

#[derive(Debug, Clone)]
pub struct ScalingConfig {
    pub min_warm_vms: usize,
    pub max_warm_vms: usize,
    pub scale_up_threshold: f64,
    pub scale_down_threshold: f64,
    pub prediction_window: Duration,
    pub warmup_time: Duration,
}

impl Default for ScalingConfig {
    fn default() -> Self {
        Self {
            min_warm_vms: 2,
            max_warm_vms: 50,
            scale_up_threshold: 0.8,
            scale_down_threshold: 0.3,
            prediction_window: Duration::from_secs(300), // 5 minutes
            warmup_time: Duration::from_millis(500),
        }
    }
}

impl VmPredictiveScaler {
    pub fn new(
        fork_manager: Arc<VmForkManager>,
        snapshot_manager: Arc<VmSnapshotManager>,
        config: ScalingConfig,
    ) -> Self {
        Self {
            fork_manager,
            snapshot_manager,
            pools: Arc::new(RwLock::new(HashMap::new())),
            predictor: Arc::new(RwLock::new(LoadPredictor {
                history: HashMap::new(),
                models: HashMap::new(),
                patterns: HashMap::new(),
            })),
            config,
        }
    }

    /// Initialize pool for an environment
    pub async fn initialize_pool(&self, environment: &str, base_config: &FirecrackerVmConfig) -> Result<()> {
        info!("Initializing VM pool for environment: {}", environment);

        // Create base VM for forking
        let base_id = format!("{}-base", environment);
        let base_vm_id = self.fork_manager
            .create_base_vm(&base_id, base_config)
            .await?;

        // Pre-warm initial VMs
        let mut warm_vms = VecDeque::new();
        for i in 0..self.config.min_warm_vms {
            let fork_id = format!("{}-warm-{}", environment, i);
            let forked = self.fork_manager
                .fork_vm(&base_id, &fork_id)
                .await?;

            warm_vms.push_back(WarmVm {
                vm_id: forked.vm_id,
                fork_id: forked.fork_id,
                warmed_at: Instant::now(),
                last_used: None,
            });
        }

        let pool = VmPool {
            environment: environment.to_string(),
            warm_vms,
            hot_vms: Vec::new(),
            cold_snapshots: vec![base_id],
            metrics: PoolMetrics::default(),
        };

        let mut pools = self.pools.write().await;
        pools.insert(environment.to_string(), pool);

        info!("VM pool initialized with {} warm VMs", self.config.min_warm_vms);
        Ok(())
    }

    /// Acquire a VM from the pool (with predictive scaling)
    pub async fn acquire_vm(&self, environment: &str) -> Result<AcquiredVm> {
        let start = Instant::now();

        // Predict future load
        let predicted_load = self.predict_load(environment, self.config.prediction_window).await?;

        // Scale proactively if needed
        if predicted_load.confidence > 0.7 && predicted_load.expected_load > self.config.scale_up_threshold {
            self.scale_up(environment, predicted_load.recommended_instances).await?;
        }

        // Get VM from pool
        let mut pools = self.pools.write().await;
        let pool = pools.get_mut(environment)
            .ok_or_else(|| anyhow::anyhow!("Pool not initialized for environment: {}", environment))?;

        // Try warm pool first
        if let Some(warm_vm) = pool.warm_vms.pop_front() {
            pool.metrics.hits += 1;

            // Promote to hot
            let hot_vm = HotVm {
                vm_id: warm_vm.vm_id.clone(),
                fork_id: warm_vm.fork_id.clone(),
                in_use: true,
                last_execution: Instant::now(),
                execution_count: 1,
            };
            pool.hot_vms.push(hot_vm);

            // Replenish warm pool in background
            let env_clone = environment.to_string();
            let scaler = self.clone();
            tokio::spawn(async move {
                let _ = scaler.replenish_warm_pool(&env_clone).await;
            });

            let wait_time = start.elapsed();
            pool.metrics.avg_wait_time =
                (pool.metrics.avg_wait_time + wait_time) / 2;

            info!("Acquired warm VM in {:?}", wait_time);

            return Ok(AcquiredVm {
                vm_id: warm_vm.vm_id,
                fork_id: warm_vm.fork_id,
                acquisition_time: wait_time,
                was_warm: true,
            });
        }

        // Cold start required
        pool.metrics.misses += 1;
        pool.metrics.cold_starts += 1;

        info!("Cold start required for environment: {}", environment);

        // Fork new VM
        let base_id = pool.cold_snapshots.first()
            .ok_or_else(|| anyhow::anyhow!("No base snapshot available"))?;

        let fork_id = format!("{}-cold-{}", environment, uuid::Uuid::new_v4());
        let forked = self.fork_manager.fork_vm(base_id, &fork_id).await?;

        let hot_vm = HotVm {
            vm_id: forked.vm_id.clone(),
            fork_id: forked.fork_id.clone(),
            in_use: true,
            last_execution: Instant::now(),
            execution_count: 1,
        };
        pool.hot_vms.push(hot_vm);

        let acquisition_time = start.elapsed();

        Ok(AcquiredVm {
            vm_id: forked.vm_id,
            fork_id: forked.fork_id,
            acquisition_time,
            was_warm: false,
        })
    }

    /// Release VM back to pool
    pub async fn release_vm(&self, environment: &str, vm_id: &str) -> Result<()> {
        let mut pools = self.pools.write().await;
        let pool = pools.get_mut(environment)
            .ok_or_else(|| anyhow::anyhow!("Pool not found"))?;

        // Find in hot pool
        if let Some(hot_vm) = pool.hot_vms.iter_mut().find(|vm| vm.vm_id == vm_id) {
            hot_vm.in_use = false;
            hot_vm.execution_count += 1;

            // Keep frequently used VMs hot
            if hot_vm.execution_count > 10 {
                return Ok(());
            }

            // Move to warm pool if space available
            if pool.warm_vms.len() < self.config.max_warm_vms {
                let fork_id = hot_vm.fork_id.clone();
                let vm_id = hot_vm.vm_id.clone();

                pool.warm_vms.push_back(WarmVm {
                    vm_id,
                    fork_id,
                    warmed_at: Instant::now(),
                    last_used: Some(Instant::now()),
                });
            }
        }

        Ok(())
    }

    /// Predict load using ML model
    async fn predict_load(&self, environment: &str, window: Duration) -> Result<LoadPrediction> {
        let predictor = self.predictor.read().await;

        // Get historical data
        let history = predictor.history.get(environment);
        let pattern = predictor.patterns.get(environment);
        let model = predictor.models.get(environment);

        if history.is_none() || pattern.is_none() {
            // No history, use defaults
            return Ok(LoadPrediction {
                expected_load: 0.5,
                confidence: 0.3,
                recommended_instances: self.config.min_warm_vms,
                prediction_window: window,
            });
        }

        let history = history.unwrap();
        let pattern = pattern.unwrap();

        // Calculate features
        let now = SystemTime::now();
        let hour = self.get_hour_of_day(now);
        let day = self.get_day_of_week(now);

        let mut features = vec![
            pattern.hourly[hour],
            pattern.daily[day],
            pattern.seasonality,
            self.trend_to_float(&pattern.trend),
        ];

        // Add recent history features
        if history.len() > 5 {
            let recent: Vec<_> = history.iter().rev().take(5).collect();
            let avg_recent_load: f64 = recent.iter().map(|p| p.load).sum::<f64>() / 5.0;
            features.push(avg_recent_load);

            let trend_slope = self.calculate_trend_slope(&recent);
            features.push(trend_slope);
        }

        // Apply model if trained
        let predicted_load = if let Some(model) = model {
            self.apply_model(model, &features)
        } else {
            // Simple heuristic
            features.iter().sum::<f64>() / features.len() as f64
        };

        // Calculate confidence based on model accuracy and data recency
        let confidence = self.calculate_confidence(model, history);

        // Recommend instances based on predicted load
        let recommended = self.calculate_recommended_instances(predicted_load);

        Ok(LoadPrediction {
            expected_load: predicted_load,
            confidence,
            recommended_instances: recommended,
            prediction_window: window,
        })
    }

    /// Apply ML model to features
    fn apply_model(&self, model: &PredictionModel, features: &[f64]) -> f64 {
        let mut result = model.bias;

        for (i, feature) in features.iter().enumerate() {
            if i < model.weights.len() {
                result += feature * model.weights[i];
            }
        }

        // Sigmoid activation for 0-1 range
        1.0 / (1.0 + (-result).exp())
    }

    fn calculate_confidence(&self, model: Option<&PredictionModel>, history: &[LoadDataPoint]) -> f64 {
        let mut confidence: f64 = 0.5;

        // Model accuracy contributes to confidence
        if let Some(model) = model {
            confidence = confidence.max(model.accuracy);
        }

        // Recency of data affects confidence
        if let Some(latest) = history.last() {
            let age = SystemTime::now()
                .duration_since(latest.timestamp)
                .unwrap_or(Duration::MAX);

            if age < Duration::from_secs(300) {
                confidence *= 1.2; // Recent data boost
            } else if age > Duration::from_secs(3600) {
                confidence *= 0.8; // Old data penalty
            }
        }

        confidence.min(1.0)
    }

    fn calculate_recommended_instances(&self, predicted_load: f64) -> usize {
        let base = self.config.min_warm_vms as f64;
        let max = self.config.max_warm_vms as f64;

        let recommended = base + (predicted_load * (max - base));
        recommended.round() as usize
    }

    /// Scale up pool
    async fn scale_up(&self, environment: &str, target_size: usize) -> Result<()> {
        let mut pools = self.pools.write().await;
        let pool = pools.get_mut(environment)
            .ok_or_else(|| anyhow::anyhow!("Pool not found"))?;

        let current_size = pool.warm_vms.len() + pool.hot_vms.len();

        if current_size >= target_size {
            return Ok(());
        }

        let to_add = (target_size - current_size).min(self.config.max_warm_vms);

        info!("Scaling up pool for {}: adding {} VMs", environment, to_add);

        let base_id = pool.cold_snapshots.first()
            .ok_or_else(|| anyhow::anyhow!("No base snapshot"))?
            .clone();

        for i in 0..to_add {
            let fork_id = format!("{}-scale-{}", environment, uuid::Uuid::new_v4());

            match self.fork_manager.fork_vm(&base_id, &fork_id).await {
                Ok(forked) => {
                    pool.warm_vms.push_back(WarmVm {
                        vm_id: forked.vm_id,
                        fork_id: forked.fork_id,
                        warmed_at: Instant::now(),
                        last_used: None,
                    });
                }
                Err(e) => {
                    warn!("Failed to scale up VM {}: {}", i, e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Scale down pool
    async fn scale_down(&self, environment: &str) -> Result<()> {
        let mut pools = self.pools.write().await;
        let pool = pools.get_mut(environment)
            .ok_or_else(|| anyhow::anyhow!("Pool not found"))?;

        // Remove oldest warm VMs
        while pool.warm_vms.len() > self.config.min_warm_vms {
            if let Some(vm) = pool.warm_vms.pop_back() {
                self.fork_manager.cleanup_fork(&vm.fork_id).await?;
                debug!("Scaled down VM: {}", vm.vm_id);
            }
        }

        // Clean up idle hot VMs
        let idle_threshold = Instant::now() - Duration::from_secs(600);
        let fork_manager = self.fork_manager.clone(); // Clone outside the closure
        pool.hot_vms.retain(|vm| {
            if !vm.in_use && vm.last_execution < idle_threshold {
                let fork_id = vm.fork_id.clone();
                let fork_manager_clone = fork_manager.clone();
                tokio::spawn(async move {
                    let _ = fork_manager_clone.cleanup_fork(&fork_id).await;
                });
                false
            } else {
                true
            }
        });

        Ok(())
    }

    /// Replenish warm pool
    async fn replenish_warm_pool(&self, environment: &str) -> Result<()> {
        let mut pools = self.pools.write().await;
        let pool = pools.get_mut(environment)
            .ok_or_else(|| anyhow::anyhow!("Pool not found"))?;

        if pool.warm_vms.len() >= self.config.min_warm_vms {
            return Ok(());
        }

        let base_id = pool.cold_snapshots.first()
            .ok_or_else(|| anyhow::anyhow!("No base snapshot"))?
            .clone();

        let fork_id = format!("{}-replenish-{}", environment, uuid::Uuid::new_v4());
        let forked = self.fork_manager.fork_vm(&base_id, &fork_id).await?;

        pool.warm_vms.push_back(WarmVm {
            vm_id: forked.vm_id,
            fork_id: forked.fork_id,
            warmed_at: Instant::now(),
            last_used: None,
        });

        Ok(())
    }

    /// Record actual usage for model training
    pub async fn record_usage(&self, environment: &str, load: f64, concurrent: usize) -> Result<()> {
        let mut predictor = self.predictor.write().await;

        let data_point = LoadDataPoint {
            timestamp: SystemTime::now(),
            load,
            concurrent_requests: concurrent,
            avg_execution_time: Duration::from_millis(100), // Would track actual
        };

        predictor.history
            .entry(environment.to_string())
            .or_insert_with(Vec::new)
            .push(data_point);

        // Retrain model periodically
        if should_retrain(predictor.models.get(environment)) {
            self.train_model(&mut predictor, environment).await?;
        }

        Ok(())
    }

    /// Train prediction model
    async fn train_model(&self, predictor: &mut LoadPredictor, environment: &str) -> Result<()> {
        let history = predictor.history.get(environment)
            .ok_or_else(|| anyhow::anyhow!("No history for training"))?;

        if history.len() < 100 {
            return Ok(()); // Need more data
        }

        info!("Training prediction model for environment: {}", environment);

        // Extract patterns
        let pattern = self.extract_pattern(history);
        predictor.patterns.insert(environment.to_string(), pattern);

        // Simple linear regression for demo
        // In production would use proper ML library
        let model = PredictionModel {
            weights: vec![0.3, 0.3, 0.2, 0.1, 0.1], // Placeholder weights
            bias: 0.5,
            accuracy: 0.85, // Would calculate from validation
            last_trained: Instant::now(),
        };

        predictor.models.insert(environment.to_string(), model);

        Ok(())
    }

    fn extract_pattern(&self, history: &[LoadDataPoint]) -> LoadPattern {
        let mut hourly = [0.0; 24];
        let mut daily = [0.0; 7];
        let mut hourly_counts = [0; 24];
        let mut daily_counts = [0; 7];

        for point in history {
            let hour = self.get_hour_of_day(point.timestamp);
            let day = self.get_day_of_week(point.timestamp);

            hourly[hour] += point.load;
            hourly_counts[hour] += 1;

            daily[day] += point.load;
            daily_counts[day] += 1;
        }

        // Average
        for i in 0..24 {
            if hourly_counts[i] > 0 {
                hourly[i] /= hourly_counts[i] as f64;
            }
        }

        for i in 0..7 {
            if daily_counts[i] > 0 {
                daily[i] /= daily_counts[i] as f64;
            }
        }

        LoadPattern {
            hourly,
            daily,
            trend: self.detect_trend(history),
            seasonality: self.calculate_seasonality(history),
        }
    }

    fn detect_trend(&self, history: &[LoadDataPoint]) -> TrendDirection {
        if history.len() < 10 {
            return TrendDirection::Stable;
        }

        let recent: Vec<_> = history.iter().rev().take(10).collect();
        let slope = self.calculate_trend_slope(&recent);

        if slope.abs() < 0.01 {
            TrendDirection::Stable
        } else if slope > 0.05 {
            TrendDirection::Increasing
        } else if slope < -0.05 {
            TrendDirection::Decreasing
        } else {
            TrendDirection::Volatile
        }
    }

    fn calculate_trend_slope(&self, points: &[&LoadDataPoint]) -> f64 {
        // Simple linear regression slope
        let n = points.len() as f64;
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_xx = 0.0;

        for (i, point) in points.iter().enumerate() {
            let x = i as f64;
            let y = point.load;

            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_xx += x * x;
        }

        (n * sum_xy - sum_x * sum_y) / (n * sum_xx - sum_x * sum_x)
    }

    fn calculate_seasonality(&self, _history: &[LoadDataPoint]) -> f64 {
        // Would implement seasonal decomposition
        0.5
    }

    fn trend_to_float(&self, trend: &TrendDirection) -> f64 {
        match trend {
            TrendDirection::Increasing => 1.0,
            TrendDirection::Stable => 0.5,
            TrendDirection::Decreasing => 0.0,
            TrendDirection::Volatile => 0.25,
        }
    }

    fn get_hour_of_day(&self, time: SystemTime) -> usize {
        // Would convert to local time properly
        0
    }

    fn get_day_of_week(&self, time: SystemTime) -> usize {
        // Would convert to local time properly
        0
    }
}

impl Clone for VmPredictiveScaler {
    fn clone(&self) -> Self {
        Self {
            fork_manager: self.fork_manager.clone(),
            snapshot_manager: self.snapshot_manager.clone(),
            pools: self.pools.clone(),
            predictor: self.predictor.clone(),
            config: self.config.clone(),
        }
    }
}

fn should_retrain(model: Option<&PredictionModel>) -> bool {
    if let Some(model) = model {
        model.last_trained.elapsed() > Duration::from_secs(3600)
    } else {
        true
    }
}

/// Acquired VM information
pub struct AcquiredVm {
    pub vm_id: String,
    pub fork_id: String,
    pub acquisition_time: Duration,
    pub was_warm: bool,
}

/// Load prediction result
#[derive(Debug)]
pub struct LoadPrediction {
    pub expected_load: f64,
    pub confidence: f64,
    pub recommended_instances: usize,
    pub prediction_window: Duration,
}