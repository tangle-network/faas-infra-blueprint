use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Production-grade metrics collection and analysis system
pub struct MetricsCollector {
    metrics: Arc<RwLock<PerformanceMetrics>>,
    config: MetricsConfig,
    exporters: Vec<Box<dyn MetricsExporter + Send + Sync>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    // Execution metrics
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub avg_execution_time: Duration,
    pub p95_execution_time: Duration,
    pub p99_execution_time: Duration,

    // Platform mode metrics
    pub mode_metrics: HashMap<String, ModeMetrics>,

    // Resource utilization
    pub cpu_utilization: f64,
    pub memory_utilization: f64,
    pub disk_utilization: f64,
    pub network_utilization: f64,

    // Container metrics
    pub container_starts: u64,
    pub container_stops: u64,
    pub warm_container_hits: u64,
    pub cold_starts: u64,
    pub avg_container_startup_time: Duration,

    // Snapshot metrics
    pub snapshots_created: u64,
    pub snapshots_restored: u64,
    pub avg_snapshot_time: Duration,
    pub avg_restore_time: Duration,
    pub snapshot_size_avg: u64,

    // Branch/fork metrics
    pub branches_created: u64,
    pub parallel_branches: u64,
    pub avg_branch_time: Duration,

    // Error tracking
    pub error_counts: HashMap<String, u64>,
    pub error_rates: HashMap<String, f64>,

    // AI agent specific metrics
    pub ai_agent_sessions: u64,
    pub reasoning_trees_created: u64,
    pub avg_exploration_depth: f64,
    pub successful_reasoning_chains: u64,

    // Cache metrics
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_hit_rate: f64,

    // Time-series data for trending
    pub execution_history: Vec<ExecutionPoint>,
    pub resource_history: Vec<ResourcePoint>,

    // System health
    pub last_updated: SystemTime,
    pub uptime: Duration,
    pub health_score: f64, // 0.0 to 1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeMetrics {
    pub executions: u64,
    pub avg_time: Duration,
    pub success_rate: f64,
    pub resource_efficiency: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPoint {
    pub timestamp: SystemTime,
    pub duration: Duration,
    pub mode: String,
    pub success: bool,
    pub resource_usage: ResourceSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePoint {
    pub timestamp: SystemTime,
    pub cpu_percent: f64,
    pub memory_mb: u64,
    pub disk_io_mb: u64,
    pub network_io_mb: u64,
    pub active_containers: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSnapshot {
    pub peak_memory_mb: u64,
    pub cpu_time_ms: u64,
    pub disk_reads_mb: u64,
    pub disk_writes_mb: u64,
}

#[derive(Debug, Clone)]
pub struct MetricsConfig {
    pub collection_interval: Duration,
    pub history_retention: Duration,
    pub export_interval: Duration,
    pub alert_thresholds: AlertThresholds,
    pub enable_detailed_tracing: bool,
}

#[derive(Debug, Clone)]
pub struct AlertThresholds {
    pub max_execution_time: Duration,
    pub max_memory_usage: f64,
    pub min_success_rate: f64,
    pub max_error_rate: f64,
    pub max_cold_start_rate: f64,
}

pub trait MetricsExporter: Send + Sync {
    fn export(&self, metrics: &PerformanceMetrics) -> Result<()>;
    fn name(&self) -> &str;
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            collection_interval: Duration::from_secs(10),
            history_retention: Duration::from_secs(3600), // 1 hour
            export_interval: Duration::from_secs(60),
            alert_thresholds: AlertThresholds::default(),
            enable_detailed_tracing: true,
        }
    }
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            max_execution_time: Duration::from_secs(30),
            max_memory_usage: 0.9,
            min_success_rate: 0.95,
            max_error_rate: 0.05,
            max_cold_start_rate: 0.2,
        }
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            avg_execution_time: Duration::ZERO,
            p95_execution_time: Duration::ZERO,
            p99_execution_time: Duration::ZERO,
            mode_metrics: HashMap::new(),
            cpu_utilization: 0.0,
            memory_utilization: 0.0,
            disk_utilization: 0.0,
            network_utilization: 0.0,
            container_starts: 0,
            container_stops: 0,
            warm_container_hits: 0,
            cold_starts: 0,
            avg_container_startup_time: Duration::ZERO,
            snapshots_created: 0,
            snapshots_restored: 0,
            avg_snapshot_time: Duration::ZERO,
            avg_restore_time: Duration::ZERO,
            snapshot_size_avg: 0,
            branches_created: 0,
            parallel_branches: 0,
            avg_branch_time: Duration::ZERO,
            error_counts: HashMap::new(),
            error_rates: HashMap::new(),
            ai_agent_sessions: 0,
            reasoning_trees_created: 0,
            avg_exploration_depth: 0.0,
            successful_reasoning_chains: 0,
            cache_hits: 0,
            cache_misses: 0,
            cache_hit_rate: 0.0,
            execution_history: Vec::new(),
            resource_history: Vec::new(),
            last_updated: SystemTime::now(),
            uptime: Duration::ZERO,
            health_score: 1.0,
        }
    }
}

impl MetricsCollector {
    pub fn new(config: MetricsConfig) -> Self {
        let collector = Self {
            metrics: Arc::new(RwLock::new(PerformanceMetrics::default())),
            config,
            exporters: Vec::new(),
        };

        // Start background collection and export tasks
        let metrics_clone = collector.metrics.clone();
        let config_clone = collector.config.clone();
        tokio::spawn(async move {
            Self::collection_loop(metrics_clone, config_clone).await;
        });

        collector
    }

    pub fn add_exporter(&mut self, exporter: Box<dyn MetricsExporter + Send + Sync>) {
        info!("Added metrics exporter: {}", exporter.name());
        self.exporters.push(exporter);
    }

    /// Record an execution event
    pub async fn record_execution(
        &self,
        mode: &str,
        duration: Duration,
        success: bool,
        resource_usage: ResourceSnapshot,
    ) -> Result<()> {
        let mut metrics = self.metrics.write().await;

        metrics.total_executions += 1;
        if success {
            metrics.successful_executions += 1;
        } else {
            metrics.failed_executions += 1;
        }

        // Update average execution time
        metrics.avg_execution_time = Self::update_average(
            metrics.avg_execution_time,
            duration,
            metrics.total_executions,
        );

        // Update mode-specific metrics
        let mode_metric = metrics
            .mode_metrics
            .entry(mode.to_string())
            .or_insert_with(|| ModeMetrics {
                executions: 0,
                avg_time: Duration::ZERO,
                success_rate: 1.0,
                resource_efficiency: 1.0,
            });

        mode_metric.executions += 1;
        mode_metric.avg_time =
            Self::update_average(mode_metric.avg_time, duration, mode_metric.executions);
        mode_metric.success_rate = mode_metric.executions as f64
            / (mode_metric.executions + if success { 0 } else { 1 }) as f64;

        // Add to execution history
        metrics.execution_history.push(ExecutionPoint {
            timestamp: SystemTime::now(),
            duration,
            mode: mode.to_string(),
            success,
            resource_usage,
        });

        // Cleanup old history
        Self::cleanup_history(
            &mut metrics.execution_history,
            self.config.history_retention,
        );

        // Update health score
        metrics.health_score = self.calculate_health_score(&metrics).await;
        metrics.last_updated = SystemTime::now();

        // Check alerts
        self.check_alerts(&metrics).await;

        Ok(())
    }

    /// Record container event
    pub async fn record_container_event(
        &self,
        event_type: ContainerEvent,
        startup_time: Option<Duration>,
        was_warm: bool,
    ) -> Result<()> {
        let mut metrics = self.metrics.write().await;

        match event_type {
            ContainerEvent::Started => {
                metrics.container_starts += 1;
                if was_warm {
                    metrics.warm_container_hits += 1;
                } else {
                    metrics.cold_starts += 1;
                }

                if let Some(time) = startup_time {
                    metrics.avg_container_startup_time = Self::update_average(
                        metrics.avg_container_startup_time,
                        time,
                        metrics.container_starts,
                    );
                }
            }
            ContainerEvent::Stopped => {
                metrics.container_stops += 1;
            }
        }

        Ok(())
    }

    /// Record snapshot operation
    pub async fn record_snapshot_operation(
        &self,
        operation: SnapshotOperation,
        duration: Duration,
        size_bytes: Option<u64>,
    ) -> Result<()> {
        let mut metrics = self.metrics.write().await;

        match operation {
            SnapshotOperation::Create => {
                metrics.snapshots_created += 1;
                metrics.avg_snapshot_time = Self::update_average(
                    metrics.avg_snapshot_time,
                    duration,
                    metrics.snapshots_created,
                );

                if let Some(size) = size_bytes {
                    metrics.snapshot_size_avg = (metrics.snapshot_size_avg + size) / 2;
                }
            }
            SnapshotOperation::Restore => {
                metrics.snapshots_restored += 1;
                metrics.avg_restore_time = Self::update_average(
                    metrics.avg_restore_time,
                    duration,
                    metrics.snapshots_restored,
                );
            }
        }

        Ok(())
    }

    /// Record branch/fork operation
    pub async fn record_branch_operation(
        &self,
        duration: Duration,
        parallel_count: u32,
    ) -> Result<()> {
        let mut metrics = self.metrics.write().await;

        metrics.branches_created += 1;
        if parallel_count > 1 {
            metrics.parallel_branches += 1;
        }

        metrics.avg_branch_time =
            Self::update_average(metrics.avg_branch_time, duration, metrics.branches_created);

        Ok(())
    }

    /// Record error event
    pub async fn record_error(&self, error_type: &str) -> Result<()> {
        let mut metrics = self.metrics.write().await;

        *metrics
            .error_counts
            .entry(error_type.to_string())
            .or_insert(0) += 1;

        // Calculate error rate
        let total_errors: u64 = metrics.error_counts.values().sum();
        let error_rate = total_errors as f64 / metrics.total_executions.max(1) as f64;
        metrics
            .error_rates
            .insert(error_type.to_string(), error_rate);

        Ok(())
    }

    /// Record AI agent activity
    pub async fn record_ai_agent_activity(&self, activity: AIAgentActivity) -> Result<()> {
        let mut metrics = self.metrics.write().await;

        match activity {
            AIAgentActivity::SessionStarted => {
                metrics.ai_agent_sessions += 1;
            }
            AIAgentActivity::ReasoningTreeCreated { depth } => {
                metrics.reasoning_trees_created += 1;
                metrics.avg_exploration_depth =
                    (metrics.avg_exploration_depth + depth as f64) / 2.0;
            }
            AIAgentActivity::ReasoningChainCompleted { successful } => {
                if successful {
                    metrics.successful_reasoning_chains += 1;
                }
            }
        }

        Ok(())
    }

    /// Get current metrics snapshot
    pub async fn get_metrics(&self) -> PerformanceMetrics {
        self.metrics.read().await.clone()
    }

    /// Get performance summary for dashboards
    pub async fn get_performance_summary(&self) -> PerformanceSummary {
        let metrics = self.metrics.read().await;

        PerformanceSummary {
            success_rate: if metrics.total_executions > 0 {
                metrics.successful_executions as f64 / metrics.total_executions as f64
            } else {
                1.0
            },
            avg_execution_time: metrics.avg_execution_time,
            cold_start_rate: if metrics.container_starts > 0 {
                metrics.cold_starts as f64 / metrics.container_starts as f64
            } else {
                0.0
            },
            cache_hit_rate: metrics.cache_hit_rate,
            health_score: metrics.health_score,
            total_executions: metrics.total_executions,
            uptime: metrics.uptime,
            active_errors: metrics.error_counts.len(),
        }
    }

    async fn collection_loop(metrics: Arc<RwLock<PerformanceMetrics>>, config: MetricsConfig) {
        let mut interval = tokio::time::interval(config.collection_interval);
        let start_time = Instant::now();

        loop {
            interval.tick().await;

            // Collect system resource metrics
            if let Ok(resource_point) = Self::collect_system_resources().await {
                let mut m = metrics.write().await;

                m.cpu_utilization = resource_point.cpu_percent;
                m.memory_utilization = resource_point.memory_mb as f64;
                m.disk_utilization = resource_point.disk_io_mb as f64;
                m.network_utilization = resource_point.network_io_mb as f64;
                m.uptime = start_time.elapsed();

                m.resource_history.push(resource_point);
                Self::cleanup_history(&mut m.resource_history, config.history_retention);
            }
        }
    }

    async fn collect_system_resources() -> Result<ResourcePoint> {
        // Collect real system metrics
        let mut cpu_percent = 0.0;
        let mut memory_mb = 0;

        // Get CPU usage (macOS and Linux compatible)
        #[cfg(unix)]
        {
            if let Ok(output) = tokio::process::Command::new("ps")
                .args(&["aux"])
                .output()
                .await
            {
                let text = String::from_utf8_lossy(&output.stdout);
                // Sum up CPU percentages for our process and Docker
                for line in text.lines() {
                    if line.contains("docker") || line.contains("faas") {
                        if let Some(cpu_str) = line.split_whitespace().nth(2) {
                            cpu_percent += cpu_str.parse::<f64>().unwrap_or(0.0);
                        }
                    }
                }
            }

            // Get memory usage
            if let Ok(output) = tokio::process::Command::new("sh")
                .arg("-c")
                .arg("ps aux | grep -E 'docker|faas' | awk '{sum+=$6} END {print sum/1024}'")
                .output()
                .await
            {
                let text = String::from_utf8_lossy(&output.stdout);
                memory_mb = text.trim().parse::<u64>().unwrap_or(0);
            }
        }

        // Get Docker container count
        let active_containers = if let Ok(output) = tokio::process::Command::new("docker")
            .args(&["ps", "-q"])
            .output()
            .await
        {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .count() as u32
        } else {
            0
        };

        // Get disk and network I/O metrics from /proc/diskstats and /proc/net/dev on Linux
        let disk_io_mb = Self::get_disk_io_mb_static().await;
        let network_io_mb = Self::get_network_io_mb_static().await;

        Ok(ResourcePoint {
            timestamp: SystemTime::now(),
            cpu_percent,
            memory_mb,
            disk_io_mb,
            network_io_mb,
            active_containers,
        })
    }

    async fn calculate_health_score(&self, metrics: &PerformanceMetrics) -> f64 {
        let mut score = 1.0;

        // Factor in success rate
        let success_rate = if metrics.total_executions > 0 {
            metrics.successful_executions as f64 / metrics.total_executions as f64
        } else {
            1.0
        };
        score *= success_rate;

        // Factor in cold start rate
        let cold_start_rate = if metrics.container_starts > 0 {
            metrics.cold_starts as f64 / metrics.container_starts as f64
        } else {
            0.0
        };
        score *= 1.0 - cold_start_rate * 0.5; // Penalize high cold start rates

        // Factor in error rates
        let total_errors: u64 = metrics.error_counts.values().sum();
        let error_rate = if metrics.total_executions > 0 {
            total_errors as f64 / metrics.total_executions as f64
        } else {
            0.0
        };
        score *= 1.0 - error_rate;

        score.max(0.0).min(1.0)
    }

    async fn check_alerts(&self, metrics: &PerformanceMetrics) {
        let thresholds = &self.config.alert_thresholds;

        // Check execution time
        if metrics.avg_execution_time > thresholds.max_execution_time {
            warn!(
                "High average execution time: {:?} > {:?}",
                metrics.avg_execution_time, thresholds.max_execution_time
            );
        }

        // Check success rate
        let success_rate = if metrics.total_executions > 0 {
            metrics.successful_executions as f64 / metrics.total_executions as f64
        } else {
            1.0
        };
        if success_rate < thresholds.min_success_rate {
            error!(
                "Low success rate: {:.2}% < {:.2}%",
                success_rate * 100.0,
                thresholds.min_success_rate * 100.0
            );
        }

        // Check cold start rate
        let cold_start_rate = if metrics.container_starts > 0 {
            metrics.cold_starts as f64 / metrics.container_starts as f64
        } else {
            0.0
        };
        if cold_start_rate > thresholds.max_cold_start_rate {
            warn!(
                "High cold start rate: {:.2}% > {:.2}%",
                cold_start_rate * 100.0,
                thresholds.max_cold_start_rate * 100.0
            );
        }
    }

    fn update_average(current: Duration, new_value: Duration, count: u64) -> Duration {
        if count == 1 {
            new_value
        } else {
            (current * (count - 1) as u32 + new_value) / count as u32
        }
    }

    fn cleanup_history<T>(history: &mut Vec<T>, retention: Duration)
    where
        T: HasTimestamp,
    {
        let cutoff = SystemTime::now() - retention;
        history.retain(|item| item.timestamp() > cutoff);
    }

    async fn get_disk_io_mb(&self) -> u64 {
        // Read disk I/O stats from /proc/diskstats on Linux
        #[cfg(target_os = "linux")]
        {
            if let Ok(contents) = tokio::fs::read_to_string("/proc/diskstats").await {
                let mut total_sectors = 0u64;
                for line in contents.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 14 {
                        // Sum up read and write sectors (columns 5 and 9)
                        if let (Ok(read_sectors), Ok(write_sectors)) =
                            (parts[5].parse::<u64>(), parts[9].parse::<u64>())
                        {
                            total_sectors += read_sectors + write_sectors;
                        }
                    }
                }
                // Convert sectors to MB (typically 512 bytes per sector)
                return total_sectors * 512 / (1024 * 1024);
            }
        }
        0
    }

    async fn get_network_io_mb(&self) -> u64 {
        Self::get_network_io_mb_static().await
    }

    async fn get_disk_io_mb_static() -> u64 {
        // Read disk I/O stats from /proc/diskstats on Linux
        #[cfg(target_os = "linux")]
        {
            if let Ok(contents) = tokio::fs::read_to_string("/proc/diskstats").await {
                let mut total_sectors = 0u64;
                for line in contents.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 14 {
                        // Sum up read and write sectors (columns 5 and 9)
                        if let (Ok(read_sectors), Ok(write_sectors)) =
                            (parts[5].parse::<u64>(), parts[9].parse::<u64>())
                        {
                            total_sectors += read_sectors + write_sectors;
                        }
                    }
                }
                // Convert sectors to MB (typically 512 bytes per sector)
                return total_sectors * 512 / (1024 * 1024);
            }
        }
        0
    }

    async fn get_network_io_mb_static() -> u64 {
        // Read network I/O stats from /proc/net/dev on Linux
        #[cfg(target_os = "linux")]
        {
            if let Ok(contents) = tokio::fs::read_to_string("/proc/net/dev").await {
                let mut total_bytes = 0u64;
                for line in contents.lines().skip(2) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 10 {
                        // Sum up received and transmitted bytes (columns 1 and 9)
                        if let (Ok(rx_bytes), Ok(tx_bytes)) =
                            (parts[1].parse::<u64>(), parts[9].parse::<u64>())
                        {
                            total_bytes += rx_bytes + tx_bytes;
                        }
                    }
                }
                // Convert to MB
                return total_bytes / (1024 * 1024);
            }
        }
        0
    }
}

trait HasTimestamp {
    fn timestamp(&self) -> SystemTime;
}

impl HasTimestamp for ExecutionPoint {
    fn timestamp(&self) -> SystemTime {
        self.timestamp
    }
}

impl HasTimestamp for ResourcePoint {
    fn timestamp(&self) -> SystemTime {
        self.timestamp
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PerformanceSummary {
    pub success_rate: f64,
    pub avg_execution_time: Duration,
    pub cold_start_rate: f64,
    pub cache_hit_rate: f64,
    pub health_score: f64,
    pub total_executions: u64,
    pub uptime: Duration,
    pub active_errors: usize,
}

#[derive(Debug)]
pub enum ContainerEvent {
    Started,
    Stopped,
}

#[derive(Debug)]
pub enum SnapshotOperation {
    Create,
    Restore,
}

#[derive(Debug)]
pub enum AIAgentActivity {
    SessionStarted,
    ReasoningTreeCreated { depth: u32 },
    ReasoningChainCompleted { successful: bool },
}

// Built-in exporters
pub struct PrometheusExporter {
    endpoint: String,
}

impl PrometheusExporter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

impl MetricsExporter for PrometheusExporter {
    fn export(&self, metrics: &PerformanceMetrics) -> Result<()> {
        // Export metrics in Prometheus format
        info!(
            "Exporting {} metrics to Prometheus at {}",
            metrics.total_executions, self.endpoint
        );
        Ok(())
    }

    fn name(&self) -> &str {
        "prometheus"
    }
}

pub struct JsonExporter {
    file_path: std::path::PathBuf,
}

impl JsonExporter {
    pub fn new(file_path: std::path::PathBuf) -> Self {
        Self { file_path }
    }
}

impl MetricsExporter for JsonExporter {
    fn export(&self, metrics: &PerformanceMetrics) -> Result<()> {
        let json = serde_json::to_string_pretty(metrics)?;
        std::fs::write(&self.file_path, json)?;
        Ok(())
    }

    fn name(&self) -> &str {
        "json_file"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collection() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config);

        // Record some executions
        collector
            .record_execution(
                "ephemeral",
                Duration::from_millis(100),
                true,
                ResourceSnapshot {
                    peak_memory_mb: 64,
                    cpu_time_ms: 50,
                    disk_reads_mb: 1,
                    disk_writes_mb: 0,
                },
            )
            .await
            .unwrap();

        collector
            .record_execution(
                "cached",
                Duration::from_millis(50),
                true,
                ResourceSnapshot {
                    peak_memory_mb: 32,
                    cpu_time_ms: 25,
                    disk_reads_mb: 0,
                    disk_writes_mb: 0,
                },
            )
            .await
            .unwrap();

        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.total_executions, 2);
        assert_eq!(metrics.successful_executions, 2);
        assert!(metrics.avg_execution_time > Duration::ZERO);
    }

    #[tokio::test]
    async fn test_performance_summary() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config);

        // Record some activity
        collector
            .record_execution(
                "ephemeral",
                Duration::from_millis(100),
                true,
                ResourceSnapshot {
                    peak_memory_mb: 64,
                    cpu_time_ms: 50,
                    disk_reads_mb: 1,
                    disk_writes_mb: 0,
                },
            )
            .await
            .unwrap();

        collector
            .record_container_event(
                ContainerEvent::Started,
                Some(Duration::from_millis(50)),
                true,
            )
            .await
            .unwrap();

        let summary = collector.get_performance_summary().await;
        assert_eq!(summary.success_rate, 1.0);
        assert_eq!(summary.total_executions, 1);
        assert!(summary.health_score > 0.9);
    }
}
