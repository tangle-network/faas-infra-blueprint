pub mod cache_manager;
pub mod metrics_collector;
pub mod predictive_scaling;
pub mod snapshot_optimizer;

pub use cache_manager::{CacheManager, CacheStrategy};
pub use metrics_collector::{MetricsCollector, PerformanceMetrics};
pub use predictive_scaling::{PredictiveScaler, UsagePattern};
pub use snapshot_optimizer::{OptimizationConfig, SnapshotOptimizer, SnapshotStats};
