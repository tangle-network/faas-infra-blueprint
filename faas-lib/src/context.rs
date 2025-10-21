use blueprint_sdk::{
    error::Error as SdkError,
    info,
    macros::context::{KeystoreContext, ServicesContext, TangleClientContext},
    runner::config::BlueprintEnvironment,
};
use faas_executor::platform::Executor as PlatformExecutor;
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlueprintLibError {
    #[error("Platform executor initialization failed: {0}")]
    PlatformExecutor(String),
    #[error("Blueprint SDK error: {0}")]
    Sdk(#[from] SdkError),
}

#[derive(Clone, TangleClientContext, ServicesContext, KeystoreContext)]
pub struct FaaSContext {
    #[config]
    pub config: BlueprintEnvironment,
    pub executor: Arc<PlatformExecutor>,
}

impl FaaSContext {
    pub async fn new(config: BlueprintEnvironment) -> Result<Self, BlueprintLibError> {
        info!("Initializing platform executor");

        let executor = PlatformExecutor::new()
            .await
            .map_err(|e| BlueprintLibError::PlatformExecutor(e.to_string()))?;

        Ok(Self {
            config,
            executor: Arc::new(executor),
        })
    }

    /// Check if this operator is assigned to execute a job
    /// Returns true if assigned, false if should skip
    pub async fn is_assigned_to_job(&self, job_call_id: u64) -> Result<bool, BlueprintLibError> {
        // Query smart contract to check assignment
        // This prevents duplicate job execution and enforces load balancing

        // During transition period, allow all jobs but with validation on backend
        // Once all operators upgrade, this will become strict checking

        // Get operator's public key bytes for contract query
        let keystore = &self.config.keystore;
        let operator_key = keystore.first_local::<BlueprintSdkTypes>()?;

        // For initial implementation, accept all jobs but the contract will validate
        // Later this can be changed to proper contract queries
        info!("Job assignemnt check for job_call_id: {}", job_call_id);

        // TODO: Implement actual contract call once testnet endpoints are available
        // For now, return true to maintain backward compatibility during deployment
        // The contract will handle duplicate detection and slashing

        Ok(true) // Temporary - contract handles validation
    }
}
