use chrono::{Duration, Utc};
use faas_usage_tracker::*;
use std::sync::Arc;

#[tokio::test]
async fn test_account_creation_and_tier_limits() {
    let storage = Arc::new(InMemoryStorage::new());

    // Create accounts with different tiers
    storage.create_account("dev_user".to_string(), Tier::Developer).await.unwrap();
    storage.create_account("team_user".to_string(), Tier::Team).await.unwrap();
    storage.create_account("scale_user".to_string(), Tier::Scale).await.unwrap();

    let tracker = UsageTracker::new(storage.clone());

    // Test Developer tier limits
    let dev_account = tracker.get_usage("dev_user").await.unwrap();
    assert_eq!(dev_account.tier, Tier::Developer);
    assert_eq!(dev_account.mcus_allocated, 300.0);

    // Test Team tier limits
    let team_account = tracker.get_usage("team_user").await.unwrap();
    assert_eq!(team_account.tier, Tier::Team);
    assert_eq!(team_account.mcus_allocated, 1000.0);

    // Test Scale tier limits
    let scale_account = tracker.get_usage("scale_user").await.unwrap();
    assert_eq!(scale_account.tier, Tier::Scale);
    assert_eq!(scale_account.mcus_allocated, 7500.0);
}

#[tokio::test]
async fn test_vcpu_limit_enforcement() {
    let storage = Arc::new(InMemoryStorage::new());
    storage.create_account("test".to_string(), Tier::Developer).await.unwrap();

    let tracker = UsageTracker::new(storage.clone());

    // Developer tier has 64 vCPU limit
    // Should succeed with 32 vCPUs
    assert!(tracker.check_limits("test", 32, 100).await.is_ok());

    // Should fail with 65 vCPUs
    let result = tracker.check_limits("test", 65, 100).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), UsageError::LimitExceeded { .. }));
}

#[tokio::test]
async fn test_ram_limit_enforcement() {
    let storage = Arc::new(InMemoryStorage::new());
    storage.create_account("test".to_string(), Tier::Developer).await.unwrap();

    let tracker = UsageTracker::new(storage.clone());

    // Developer tier has 256 GB RAM limit
    // Should succeed with 200 GB
    assert!(tracker.check_limits("test", 10, 200).await.is_ok());

    // Should fail with 257 GB
    let result = tracker.check_limits("test", 10, 257).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), UsageError::LimitExceeded { .. }));
}

#[tokio::test]
async fn test_instance_lifecycle_tracking() {
    let storage = Arc::new(InMemoryStorage::new());
    storage.create_account("test".to_string(), Tier::Developer).await.unwrap();

    let tracker = UsageTracker::new(storage.clone());

    // Start an instance (with time in the past to ensure some usage)
    let instance = InstanceRecord {
        instance_id: "inst-123".to_string(),
        vcpus: 4,
        ram_gb: 16,
        disk_gb: 100,
        started_at: Utc::now() - Duration::seconds(10), // Started 10 seconds ago
        stopped_at: None,
    };

    tracker.start_instance("test", instance.clone()).await.unwrap();

    // Verify instance is tracked
    let usage = tracker.get_usage("test").await.unwrap();
    assert_eq!(usage.active_resources.instances.len(), 1);
    assert_eq!(usage.active_resources.instances[0].instance_id, "inst-123");
    assert!(usage.active_resources.instances[0].stopped_at.is_none());

    // Stop the instance
    tracker.stop_instance("test", "inst-123").await.unwrap();

    // Verify instance is marked as stopped and usage is calculated
    let usage = tracker.get_usage("test").await.unwrap();
    assert!(usage.active_resources.instances[0].stopped_at.is_some());
    assert!(usage.mcus_consumed > 0.0); // Some MCUs should be consumed
}

#[tokio::test]
async fn test_execution_recording() {
    let storage = Arc::new(InMemoryStorage::new());
    storage.create_account("test".to_string(), Tier::Developer).await.unwrap();

    let tracker = UsageTracker::new(storage.clone());

    // Record a 1-hour execution
    let execution = ExecutionRecord {
        execution_id: "exec-456".to_string(),
        account_id: "test".to_string(),
        vcpu_seconds: 3600.0, // 1 hour
        ram_gb_seconds: 14400.0, // 4 GB for 1 hour = 1 MCU
        mode: "ephemeral".to_string(),
        timestamp: Utc::now(),
        duration_ms: 3600000,
    };

    tracker.record_execution(execution).await.unwrap();

    // Check MCU consumption
    let usage = tracker.get_usage("test").await.unwrap();
    assert_eq!(usage.usage.vcpu_hours, 1.0);
    assert_eq!(usage.usage.ram_gb_hours, 4.0);
    assert_eq!(usage.mcus_consumed, 1.0); // Should be 1 MCU (1 vCPU-hour)
}

#[tokio::test]
async fn test_mcu_calculation_accuracy() {
    let storage = Arc::new(InMemoryStorage::new());
    storage.create_account("test".to_string(), Tier::Team).await.unwrap();

    let tracker = UsageTracker::new(storage.clone());

    // Record various executions to test MCU calculation
    // 10 vCPU-hours = 10 MCUs
    let execution1 = ExecutionRecord {
        execution_id: "exec-1".to_string(),
        account_id: "test".to_string(),
        vcpu_seconds: 36000.0, // 10 hours
        ram_gb_seconds: 0.0,
        mode: "cached".to_string(),
        timestamp: Utc::now(),
        duration_ms: 36000000,
    };

    tracker.record_execution(execution1).await.unwrap();

    let usage = tracker.get_usage("test").await.unwrap();
    assert_eq!(usage.mcus_consumed, 10.0);

    // Add 40 GB-hours of RAM = 10 MCUs (40/4)
    let execution2 = ExecutionRecord {
        execution_id: "exec-2".to_string(),
        account_id: "test".to_string(),
        vcpu_seconds: 0.0,
        ram_gb_seconds: 144000.0, // 40 GB-hours
        mode: "cached".to_string(),
        timestamp: Utc::now(),
        duration_ms: 3600000,
    };

    tracker.record_execution(execution2).await.unwrap();

    let usage = tracker.get_usage("test").await.unwrap();
    // Max of vCPU (10) and RAM (10) = 10 MCUs total
    assert_eq!(usage.usage.vcpu_hours, 10.0);
    assert_eq!(usage.usage.ram_gb_hours, 40.0);
    assert_eq!(usage.mcus_consumed, 10.0); // Max(10 vCPU, 10 RAM) = 10
}

#[tokio::test]
async fn test_billing_estimate() {
    let storage = Arc::new(InMemoryStorage::new());
    storage.create_account("team_user".to_string(), Tier::Team).await.unwrap();

    let tracker = UsageTracker::new(storage.clone());

    // Use up some MCUs
    let execution = ExecutionRecord {
        execution_id: "exec-billing".to_string(),
        account_id: "team_user".to_string(),
        vcpu_seconds: 360000.0, // 100 hours = 100 MCUs
        ram_gb_seconds: 0.0,
        mode: "persistent".to_string(),
        timestamp: Utc::now(),
        duration_ms: 360000000,
    };

    tracker.record_execution(execution).await.unwrap();

    let estimate = tracker.get_billing_estimate("team_user").await.unwrap();

    assert_eq!(estimate.tier, Tier::Team);
    assert_eq!(estimate.base_subscription, 40.0); // Team tier is $40/month
    assert_eq!(estimate.mcus_included, 1000.0);
    assert_eq!(estimate.mcus_used, 100.0);
    assert_eq!(estimate.mcus_overage, 0.0); // Still within included MCUs
    assert_eq!(estimate.total_estimate, 40.0); // No overage charges
}

#[tokio::test]
async fn test_overage_billing() {
    let storage = Arc::new(InMemoryStorage::new());
    storage.create_account("dev_user".to_string(), Tier::Developer).await.unwrap();

    let tracker = UsageTracker::new(storage.clone());

    // Use more than 300 MCUs (Developer tier limit)
    let execution = ExecutionRecord {
        execution_id: "exec-overage".to_string(),
        account_id: "dev_user".to_string(),
        vcpu_seconds: 1440000.0, // 400 hours = 400 MCUs
        ram_gb_seconds: 0.0,
        mode: "persistent".to_string(),
        timestamp: Utc::now(),
        duration_ms: 1440000000,
    };

    tracker.record_execution(execution).await.unwrap();

    let estimate = tracker.get_billing_estimate("dev_user").await.unwrap();

    assert_eq!(estimate.mcus_included, 300.0);
    assert_eq!(estimate.mcus_used, 400.0);
    assert_eq!(estimate.mcus_overage, 100.0);
    assert_eq!(estimate.overage_cost, 5.0); // 100 MCUs * $0.05
    assert_eq!(estimate.total_estimate, 5.0); // Developer tier is free + overage
}

#[tokio::test]
async fn test_insufficient_credits_without_pay_as_you_go() {
    let storage = Arc::new(InMemoryStorage::new());
    storage.create_account("limited_user".to_string(), Tier::Developer).await.unwrap();

    let tracker = UsageTracker::new(storage.clone());

    // Use up all MCUs
    let execution = ExecutionRecord {
        execution_id: "exec-exhaust".to_string(),
        account_id: "limited_user".to_string(),
        vcpu_seconds: 1080000.0, // 300 hours = 300 MCUs
        ram_gb_seconds: 0.0,
        mode: "persistent".to_string(),
        timestamp: Utc::now(),
        duration_ms: 1080000000,
    };

    tracker.record_execution(execution).await.unwrap();

    // Try to start a new instance - should fail
    let instance = InstanceRecord {
        instance_id: "inst-fail".to_string(),
        vcpus: 1,
        ram_gb: 1,
        disk_gb: 1,
        started_at: Utc::now(),
        stopped_at: None,
    };

    let result = tracker.start_instance("limited_user", instance).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), UsageError::LimitExceeded { .. }));
}

#[tokio::test]
async fn test_snapshot_tracking() {
    let storage = Arc::new(InMemoryStorage::new());
    storage.create_account("test".to_string(), Tier::Developer).await.unwrap();

    let snapshot = SnapshotRecord {
        snapshot_id: "snap-789".to_string(),
        size_gb: 100,
        created_at: Utc::now() - Duration::hours(2),
        deleted_at: None,
    };

    storage.add_snapshot("test", &snapshot).await.unwrap();

    let usage = storage.get_account("test").await.unwrap();
    assert_eq!(usage.active_resources.snapshots.len(), 1);

    // Delete snapshot after 2 hours
    storage.delete_snapshot("test", "snap-789").await.unwrap();

    let usage = storage.get_account("test").await.unwrap();
    assert!(usage.active_resources.snapshots[0].deleted_at.is_some());
    // 100 GB for 2 hours = 0.2 TB-hours = 0.04 MCUs (0.2/5)
    assert!(usage.mcus_consumed > 0.0);
}

#[tokio::test]
async fn test_concurrent_instance_limits() {
    let storage = Arc::new(InMemoryStorage::new());
    storage.create_account("test".to_string(), Tier::Developer).await.unwrap();

    let tracker = UsageTracker::new(storage.clone());

    // Start first instance with 32 vCPUs
    let instance1 = InstanceRecord {
        instance_id: "inst-1".to_string(),
        vcpus: 32,
        ram_gb: 128,
        disk_gb: 500,
        started_at: Utc::now(),
        stopped_at: None,
    };

    tracker.start_instance("test", instance1).await.unwrap();

    // Try to start second instance with 33 vCPUs - should fail (32 + 33 > 64)
    let instance2 = InstanceRecord {
        instance_id: "inst-2".to_string(),
        vcpus: 33,
        ram_gb: 128,
        disk_gb: 500,
        started_at: Utc::now(),
        stopped_at: None,
    };

    let result = tracker.start_instance("test", instance2).await;
    assert!(result.is_err());

    // But starting with 32 vCPUs should work (32 + 32 = 64)
    let instance3 = InstanceRecord {
        instance_id: "inst-3".to_string(),
        vcpus: 32,
        ram_gb: 128,
        disk_gb: 500,
        started_at: Utc::now(),
        stopped_at: None,
    };

    tracker.start_instance("test", instance3).await.unwrap();

    let usage = tracker.get_usage("test").await.unwrap();
    assert_eq!(usage.active_resources.instances.len(), 2);
}

#[tokio::test]
async fn test_usage_history() {
    let storage = Arc::new(InMemoryStorage::new());
    storage.create_account("test".to_string(), Tier::Team).await.unwrap();

    let start = Utc::now() - Duration::days(7);
    let end = Utc::now();

    let history = storage.get_usage_history("test", start, end).await.unwrap();
    assert_eq!(history.len(), 1); // In-memory storage returns current snapshot
    assert_eq!(history[0].account_id, "test");
    assert_eq!(history[0].tier, Tier::Team);
}

#[tokio::test]
async fn test_account_not_found() {
    let storage = Arc::new(InMemoryStorage::new());
    let tracker = UsageTracker::new(storage);

    let result = tracker.get_usage("nonexistent").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), UsageError::AccountNotFound(_)));
}