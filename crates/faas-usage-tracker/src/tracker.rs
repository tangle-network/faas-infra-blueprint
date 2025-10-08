use crate::{
    AccountUsage, BillingEstimate, ExecutionRecord, InstanceRecord, Result, UsageError,
    UsageStorage,
};
use std::sync::Arc;

pub struct UsageTracker {
    storage: Arc<dyn UsageStorage>,
}

impl UsageTracker {
    pub fn new(storage: Arc<dyn UsageStorage>) -> Self {
        Self { storage }
    }

    pub async fn check_limits(
        &self,
        account_id: &str,
        requested_vcpus: u32,
        requested_ram_gb: u32,
    ) -> Result<()> {
        let account = self.storage.get_account(account_id).await?;
        let limits = account.tier.limits();

        // Calculate current usage
        let current_vcpus: u32 = account
            .active_resources
            .instances
            .iter()
            .filter(|i| i.stopped_at.is_none())
            .map(|i| i.vcpus)
            .sum();

        let current_ram: u32 = account
            .active_resources
            .instances
            .iter()
            .filter(|i| i.stopped_at.is_none())
            .map(|i| i.ram_gb)
            .sum();

        // Check limits
        if current_vcpus + requested_vcpus > limits.max_vcpu {
            return Err(UsageError::LimitExceeded {
                message: format!(
                    "vCPU limit exceeded: {} + {} > {}",
                    current_vcpus, requested_vcpus, limits.max_vcpu
                ),
            });
        }

        if current_ram + requested_ram_gb > limits.max_ram_gb {
            return Err(UsageError::LimitExceeded {
                message: format!(
                    "RAM limit exceeded: {} + {} > {}",
                    current_ram, requested_ram_gb, limits.max_ram_gb
                ),
            });
        }

        // Check MCU credits
        let remaining = account.mcus_allocated - account.mcus_consumed;
        if remaining <= 0.0 && !account.pay_as_you_go_enabled {
            return Err(UsageError::LimitExceeded {
                message: format!("Insufficient MCU credits: {remaining:.2} remaining"),
            });
        }

        Ok(())
    }

    pub async fn record_execution(&self, record: ExecutionRecord) -> Result<()> {
        self.storage.record_execution(&record).await
    }

    pub async fn start_instance(&self, account_id: &str, instance: InstanceRecord) -> Result<()> {
        // Check limits first
        self.check_limits(account_id, instance.vcpus, instance.ram_gb)
            .await?;

        // Add instance
        self.storage.add_instance(account_id, &instance).await
    }

    pub async fn stop_instance(&self, account_id: &str, instance_id: &str) -> Result<()> {
        self.storage.stop_instance(account_id, instance_id).await
    }

    pub async fn get_usage(&self, account_id: &str) -> Result<AccountUsage> {
        self.storage.get_account(account_id).await
    }

    pub async fn get_billing_estimate(&self, account_id: &str) -> Result<BillingEstimate> {
        let account = self.storage.get_account(account_id).await?;
        let limits = account.tier.limits();

        let overage_mcus = if account.mcus_consumed > account.mcus_allocated {
            account.mcus_consumed - account.mcus_allocated
        } else {
            0.0
        };

        let overage_cost = overage_mcus * 0.05; // $0.05 per MCU

        Ok(BillingEstimate {
            tier: account.tier,
            base_subscription: limits.monthly_price,
            mcus_included: account.mcus_allocated,
            mcus_used: account.mcus_consumed,
            mcus_overage: overage_mcus,
            overage_cost,
            total_estimate: limits.monthly_price + overage_cost,
            billing_period_end: account.billing_period_end,
        })
    }
}
