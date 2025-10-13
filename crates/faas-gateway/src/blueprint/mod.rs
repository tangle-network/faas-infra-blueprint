//! Blueprint SDK integration for FaaS Gateway
//!
//! Provides HTTP API compatible with Blueprint SDK's HttpFaasExecutor.
//! Supports both local execution and Tangle blockchain routing.

pub mod backend;
pub mod handlers;
pub mod local_backend;
pub mod tangle_backend;

pub use backend::{
    BackendError, BackendType, DeployInfo, ExecutionBackend, FaasConfig, HealthStatus,
    InvokeResult, Result,
};
pub use handlers::{blueprint_routes, AppState};
pub use local_backend::LocalBackend;
pub use tangle_backend::TangleBackend;

use async_trait::async_trait;
use std::sync::Arc;
use tracing::info;

/// Router that selects backend based on request header
pub struct BackendRouter {
    local: Arc<LocalBackend>,
    tangle: Arc<TangleBackend>,
    default_backend: BackendType,
}

impl BackendRouter {
    pub async fn new(base_url: String, tangle_endpoint: String) -> Result<Self> {
        let local = Arc::new(LocalBackend::new(base_url.clone()).await?);
        let tangle = Arc::new(TangleBackend::new(tangle_endpoint, base_url).await?);

        Ok(Self {
            local,
            tangle,
            default_backend: BackendType::Local, // Fast local execution by default
        })
    }

    /// Select backend based on type
    fn select(&self, backend_type: BackendType) -> Arc<dyn ExecutionBackend> {
        match backend_type {
            BackendType::Local => self.local.clone(),
            BackendType::Tangle => self.tangle.clone(),
        }
    }

    /// Get backend from request header or use default
    pub fn get_backend(&self, backend_type: Option<BackendType>) -> Arc<dyn ExecutionBackend> {
        self.select(backend_type.unwrap_or(self.default_backend))
    }
}

#[async_trait]
impl ExecutionBackend for BackendRouter {
    async fn deploy(
        &self,
        function_id: String,
        binary: Vec<u8>,
        config: FaasConfig,
    ) -> Result<DeployInfo> {
        // Deploy to default backend
        self.select(self.default_backend)
            .deploy(function_id, binary, config)
            .await
    }

    async fn invoke(&self, function_id: String, payload: Vec<u8>) -> Result<InvokeResult> {
        // Invoke on default backend
        self.select(self.default_backend)
            .invoke(function_id, payload)
            .await
    }

    async fn health(&self, function_id: String) -> Result<HealthStatus> {
        self.select(self.default_backend).health(function_id).await
    }

    async fn info(&self, function_id: String) -> Result<DeployInfo> {
        self.select(self.default_backend).info(function_id).await
    }

    async fn undeploy(&self, function_id: String) -> Result<()> {
        self.select(self.default_backend).undeploy(function_id).await
    }

    async fn warm(&self, function_id: String) -> Result<u32> {
        self.select(self.default_backend).warm(function_id).await
    }
}
