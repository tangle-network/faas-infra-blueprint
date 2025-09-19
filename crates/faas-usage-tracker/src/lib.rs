// FaaS Usage Tracker - Clean, testable, production-ready usage tracking
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod storage;
mod tracker;
mod types;

pub use storage::{InMemoryStorage, UsageStorage};
pub use tracker::UsageTracker;
pub use types::*;

// Error Types
#[derive(Error, Debug)]
pub enum UsageError {
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Invalid tier: {0}")]
    InvalidTier(String),
    #[error("Account not found: {0}")]
    AccountNotFound(String),
    #[error("Resource limit exceeded: {message}")]
    LimitExceeded { message: String },
}

pub type Result<T> = std::result::Result<T, UsageError>;

// Core Types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Tier {
    Developer,  // Free tier - 300 MCUs
    Team,       // Popular tier - 1000 MCUs, $40/month
    Scale,      // Enterprise tier - 7500 MCUs, $250/month
}

impl Tier {
    pub fn limits(&self) -> TierLimits {
        match self {
            Tier::Developer => TierLimits {
                max_vcpu: 64,
                max_ram_gb: 256,
                max_storage_gb: 1024,
                starting_mcus: 300,
                monthly_price: 0.0,
                discount_percent: 100,
            },
            Tier::Team => TierLimits {
                max_vcpu: 256,
                max_ram_gb: 1024,
                max_storage_gb: 4096,
                starting_mcus: 1000,
                monthly_price: 40.0,
                discount_percent: 20,
            },
            Tier::Scale => TierLimits {
                max_vcpu: 1024,
                max_ram_gb: 4096,
                max_storage_gb: 16384,
                starting_mcus: 7500,
                monthly_price: 250.0,
                discount_percent: 33,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierLimits {
    pub max_vcpu: u32,
    pub max_ram_gb: u32,
    pub max_storage_gb: u32,
    pub starting_mcus: u32,
    pub monthly_price: f64,
    pub discount_percent: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_limits() {
        let dev = Tier::Developer.limits();
        assert_eq!(dev.max_vcpu, 64);
        assert_eq!(dev.starting_mcus, 300);
    }
}
