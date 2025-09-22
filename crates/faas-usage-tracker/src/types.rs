use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// MCU calculation and usage types
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct McuUsage {
    pub vcpu_hours: f64,
    pub ram_gb_hours: f64,
    pub disk_gb_hours: f64,
    pub snapshot_tb_hours: f64,
}

impl McuUsage {
    pub fn calculate_mcus(&self) -> f64 {
        let cpu_mcus = self.vcpu_hours;
        let ram_mcus = self.ram_gb_hours / 4.0;
        let disk_mcus = self.disk_gb_hours / 16.0;
        let snapshot_mcus = self.snapshot_tb_hours / 5.0;

        // Take the max of compute resources, add snapshot separately
        cpu_mcus.max(ram_mcus).max(disk_mcus) + snapshot_mcus
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountUsage {
    pub account_id: String,
    pub tier: crate::Tier,
    pub billing_period_start: DateTime<Utc>,
    pub billing_period_end: DateTime<Utc>,

    // MCU tracking
    pub mcus_allocated: f64,
    pub mcus_consumed: f64,
    pub pay_as_you_go_enabled: bool,

    // Resource usage
    pub usage: McuUsage,
    pub active_resources: ActiveResources,

    // Metadata
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ActiveResources {
    pub instances: Vec<InstanceRecord>,
    pub snapshots: Vec<SnapshotRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceRecord {
    pub instance_id: String,
    pub vcpus: u32,
    pub ram_gb: u32,
    pub disk_gb: u32,
    pub started_at: DateTime<Utc>,
    pub stopped_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRecord {
    pub snapshot_id: String,
    pub size_gb: u64,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRecord {
    pub execution_id: String,
    pub account_id: String,
    pub vcpu_seconds: f64,
    pub ram_gb_seconds: f64,
    pub mode: String,
    pub timestamp: DateTime<Utc>,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BillingEstimate {
    pub tier: crate::Tier,
    pub base_subscription: f64,
    pub mcus_included: f64,
    pub mcus_used: f64,
    pub mcus_overage: f64,
    pub overage_cost: f64,
    pub total_estimate: f64,
    pub billing_period_end: DateTime<Utc>,
}
