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
}