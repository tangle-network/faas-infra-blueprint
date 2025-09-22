use crate::{AccountUsage, ExecutionRecord, InstanceRecord, Result, SnapshotRecord, UsageError};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[async_trait]
pub trait UsageStorage: Send + Sync {
    async fn get_account(&self, account_id: &str) -> Result<AccountUsage>;
    async fn update_account(&self, account: &AccountUsage) -> Result<()>;
    async fn record_execution(&self, record: &ExecutionRecord) -> Result<()>;
    async fn add_instance(&self, account_id: &str, instance: &InstanceRecord) -> Result<()>;
    async fn stop_instance(&self, account_id: &str, instance_id: &str) -> Result<()>;
    async fn add_snapshot(&self, account_id: &str, snapshot: &SnapshotRecord) -> Result<()>;
    async fn delete_snapshot(&self, account_id: &str, snapshot_id: &str) -> Result<()>;
    async fn get_usage_history(
        &self,
        account_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<AccountUsage>>;
}

// In-memory storage implementation for development/testing
pub struct InMemoryStorage {
    accounts: Arc<RwLock<HashMap<String, AccountUsage>>>,
    executions: Arc<RwLock<Vec<ExecutionRecord>>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            accounts: Arc::new(RwLock::new(HashMap::new())),
            executions: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn create_account(&self, account_id: String, tier: crate::Tier) -> Result<()> {
        let mut accounts = self.accounts.write().await;
        let now = Utc::now();
        let limits = tier.limits();

        accounts.insert(
            account_id.clone(),
            AccountUsage {
                account_id,
                tier,
                billing_period_start: now,
                billing_period_end: now + chrono::Duration::days(30),
                mcus_allocated: limits.starting_mcus as f64,
                mcus_consumed: 0.0,
                pay_as_you_go_enabled: false,
                usage: crate::McuUsage::default(),
                active_resources: crate::ActiveResources::default(),
                last_updated: now,
            },
        );
        Ok(())
    }
}

#[async_trait]
impl UsageStorage for InMemoryStorage {
    async fn get_account(&self, account_id: &str) -> Result<AccountUsage> {
        self.accounts
            .read()
            .await
            .get(account_id)
            .cloned()
            .ok_or_else(|| UsageError::AccountNotFound(account_id.to_string()))
    }

    async fn update_account(&self, account: &AccountUsage) -> Result<()> {
        self.accounts
            .write()
            .await
            .insert(account.account_id.clone(), account.clone());
        Ok(())
    }

    async fn record_execution(&self, record: &ExecutionRecord) -> Result<()> {
        // Update account usage
        let mut accounts = self.accounts.write().await;
        if let Some(account) = accounts.get_mut(&record.account_id) {
            account.usage.vcpu_hours += record.vcpu_seconds / 3600.0;
            account.usage.ram_gb_hours += record.ram_gb_seconds / 3600.0;
            account.mcus_consumed = account.usage.calculate_mcus();
            account.last_updated = Utc::now();
        }

        // Store execution record
        self.executions.write().await.push(record.clone());
        Ok(())
    }

    async fn add_instance(&self, account_id: &str, instance: &InstanceRecord) -> Result<()> {
        let mut accounts = self.accounts.write().await;
        if let Some(account) = accounts.get_mut(account_id) {
            account.active_resources.instances.push(instance.clone());
            account.last_updated = Utc::now();
        }
        Ok(())
    }

    async fn stop_instance(&self, account_id: &str, instance_id: &str) -> Result<()> {
        let mut accounts = self.accounts.write().await;
        if let Some(account) = accounts.get_mut(account_id) {
            if let Some(instance) = account
                .active_resources
                .instances
                .iter_mut()
                .find(|i| i.instance_id == instance_id)
            {
                instance.stopped_at = Some(Utc::now());

                // Calculate usage for the instance
                if let Some(stopped) = instance.stopped_at {
                    let duration = stopped - instance.started_at;
                    let hours = duration.num_seconds() as f64 / 3600.0;
                    account.usage.vcpu_hours += instance.vcpus as f64 * hours;
                    account.usage.ram_gb_hours += instance.ram_gb as f64 * hours;
                    account.usage.disk_gb_hours += instance.disk_gb as f64 * hours;
                    account.mcus_consumed = account.usage.calculate_mcus();
                }
            }
            account.last_updated = Utc::now();
        }
        Ok(())
    }

    async fn add_snapshot(&self, account_id: &str, snapshot: &SnapshotRecord) -> Result<()> {
        let mut accounts = self.accounts.write().await;
        if let Some(account) = accounts.get_mut(account_id) {
            account.active_resources.snapshots.push(snapshot.clone());
            account.last_updated = Utc::now();
        }
        Ok(())
    }

    async fn delete_snapshot(&self, account_id: &str, snapshot_id: &str) -> Result<()> {
        let mut accounts = self.accounts.write().await;
        if let Some(account) = accounts.get_mut(account_id) {
            if let Some(snapshot) = account
                .active_resources
                .snapshots
                .iter_mut()
                .find(|s| s.snapshot_id == snapshot_id)
            {
                snapshot.deleted_at = Some(Utc::now());

                // Calculate snapshot storage usage
                if let Some(deleted) = snapshot.deleted_at {
                    let duration = deleted - snapshot.created_at;
                    let hours = duration.num_seconds() as f64 / 3600.0;
                    let tb_hours = (snapshot.size_gb as f64 / 1024.0) * hours;
                    account.usage.snapshot_tb_hours += tb_hours;
                    account.mcus_consumed = account.usage.calculate_mcus();
                }
            }
            account.last_updated = Utc::now();
        }
        Ok(())
    }

    async fn get_usage_history(
        &self,
        account_id: &str,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Result<Vec<AccountUsage>> {
        // For in-memory, just return current snapshot
        if let Ok(account) = self.get_account(account_id).await {
            Ok(vec![account])
        } else {
            Ok(vec![])
        }
    }
}
