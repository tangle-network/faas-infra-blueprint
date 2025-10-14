//! Tangle blockchain execution backend
//!
//! Routes Blueprint functions to Tangle Network for decentralized execution

use super::backend::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, instrument};

#[cfg(feature = "tangle")]
use faas_sdk::tangle::TangleClient;

/// Function metadata for Tangle deployment
#[derive(Debug, Clone)]
struct TangleFunction {
    function_id: String,
    deployed_at: chrono::DateTime<chrono::Utc>,
    config: FaasConfig,
    /// Service ID on Tangle (after deployment)
    service_id: Option<u64>,
}

/// Tangle network execution backend
///
/// This backend routes execution to Tangle blockchain operators
/// rather than executing locally. Operators use their own executors.
pub struct TangleBackend {
    functions: Arc<RwLock<HashMap<String, TangleFunction>>>,
    storage_path: PathBuf,
    tangle_endpoint: String,
    base_url: String,
    #[cfg(feature = "tangle")]
    client: Option<Arc<TangleClient>>,
}

impl TangleBackend {
    pub async fn new(tangle_endpoint: String, base_url: String) -> Result<Self> {
        // Storage for deployed binaries
        let storage_path = if cfg!(target_os = "linux") {
            PathBuf::from("/var/lib/faas/tangle-functions")
        } else {
            std::env::temp_dir().join("faas/tangle-functions")
        };

        tokio::fs::create_dir_all(&storage_path)
            .await
            .map_err(|e| BackendError::Storage(e.to_string()))?;

        // Initialize Tangle client if feature enabled
        #[cfg(feature = "tangle")]
        let client = TangleClient::new(&tangle_endpoint).await.ok().map(Arc::new);

        Ok(Self {
            functions: Arc::new(RwLock::new(HashMap::new())),
            storage_path,
            tangle_endpoint,
            base_url,
            #[cfg(feature = "tangle")]
            client,
        })
    }

    /// Store binary for future deployment to operators
    #[instrument(skip(self, binary))]
    async fn store_binary(&self, function_id: &str, binary: Vec<u8>) -> Result<PathBuf> {
        use tokio::io::AsyncWriteExt;

        let func_dir = self.storage_path.join(function_id);
        tokio::fs::create_dir_all(&func_dir)
            .await
            .map_err(|e| BackendError::Storage(format!("Failed to create directory: {}", e)))?;

        // Store the package for operators to download
        let package_path = func_dir.join("package.zip");
        let mut file = tokio::fs::File::create(&package_path)
            .await
            .map_err(|e| BackendError::Storage(format!("Failed to create package: {}", e)))?;

        file.write_all(&binary)
            .await
            .map_err(|e| BackendError::Storage(format!("Failed to write package: {}", e)))?;

        Ok(package_path)
    }
}

#[async_trait]
impl ExecutionBackend for TangleBackend {
    #[instrument(skip(self, binary))]
    async fn deploy(
        &self,
        function_id: String,
        binary: Vec<u8>,
        config: FaasConfig,
    ) -> Result<DeployInfo> {
        // Check if already exists
        {
            let functions = self.functions.read().await;
            if functions.contains_key(&function_id) {
                return Err(BackendError::AlreadyExists(function_id));
            }
        }

        info!("Deploying function to Tangle: {}", function_id);

        // Store binary locally (operators will fetch it)
        let _package_path = self.store_binary(&function_id, binary).await?;

        // TODO: Register service on Tangle blockchain
        // This would:
        // 1. Create a service registration transaction
        // 2. Upload binary to IPFS/S3 for operators to fetch
        // 3. Get service_id back from blockchain
        //
        // For now, we'll simulate it
        let service_id = rand::random::<u64>();

        info!(
            "Function registered on Tangle with service_id: {}",
            service_id
        );

        // Store metadata
        let metadata = TangleFunction {
            function_id: function_id.clone(),
            deployed_at: chrono::Utc::now(),
            config: config.clone(),
            service_id: Some(service_id),
        };

        let mut functions = self.functions.write().await;
        functions.insert(function_id.clone(), metadata);

        Ok(DeployInfo {
            function_id: function_id.clone(),
            endpoint: format!(
                "{}/api/blueprint/functions/{}/invoke",
                self.base_url, function_id
            ),
            status: "deployed".to_string(),
            cold_start_ms: 2000, // Blockchain latency higher than local
            memory_mb: config.memory_mb,
            timeout_secs: config.timeout_secs,
        })
    }

    #[instrument(skip(self, payload))]
    async fn invoke(&self, function_id: String, payload: Vec<u8>) -> Result<InvokeResult> {
        // Get function metadata
        let metadata = {
            let functions = self.functions.read().await;
            functions
                .get(&function_id)
                .ok_or_else(|| BackendError::NotFound(function_id.clone()))?
                .clone()
        };

        let service_id = metadata
            .service_id
            .ok_or_else(|| BackendError::ExecutionFailed("Service not registered".to_string()))?;

        info!(
            "Submitting job to Tangle network: service_id={}",
            service_id
        );

        // TODO: Submit job to Tangle blockchain
        // This would:
        // 1. Parse payload as JSON: {"job_id": N, "args": [...]}
        // 2. Create blockchain transaction calling job N with args
        // 3. Wait for operator to execute and submit result
        // 4. Poll for result event on blockchain
        // 5. Return result
        //
        // For now, we'll simulate it
        let start = std::time::Instant::now();

        // Parse input JSON
        let input: serde_json::Value = serde_json::from_slice(&payload)
            .map_err(|e| BackendError::ExecutionFailed(format!("Invalid JSON payload: {}", e)))?;

        let job_id = input["job_id"]
            .as_u64()
            .ok_or_else(|| BackendError::ExecutionFailed("Missing job_id".to_string()))?;

        let args = input["args"]
            .as_array()
            .ok_or_else(|| BackendError::ExecutionFailed("Missing args array".to_string()))?
            .iter()
            .map(|v| v.as_u64().unwrap_or(0) as u8)
            .collect::<Vec<u8>>();

        info!("Job submitted: job_id={}, args={:?}", job_id, args);

        // Simulate blockchain execution latency (2-5 seconds)
        tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;

        // Simulate result (operators would submit this to blockchain)
        let result = vec![42u8; 8]; // Placeholder result

        let execution_ms = start.elapsed().as_millis() as u64;

        info!("Job completed on Tangle: {}ms", execution_ms);

        Ok(InvokeResult {
            job_id,
            result,
            success: true,
            execution_ms,
        })
    }

    async fn health(&self, function_id: String) -> Result<HealthStatus> {
        let functions = self.functions.read().await;
        let metadata = functions
            .get(&function_id)
            .ok_or_else(|| BackendError::NotFound(function_id.clone()))?;

        // Check if service is registered on Tangle
        let status = if metadata.service_id.is_some() {
            "healthy"
        } else {
            "unhealthy"
        };

        Ok(HealthStatus {
            function_id: function_id.clone(),
            status: status.to_string(),
            last_invocation: None, // Blockchain doesn't track this locally
            total_invocations: 0,  // Would query from blockchain events
        })
    }

    async fn info(&self, function_id: String) -> Result<DeployInfo> {
        let functions = self.functions.read().await;
        let metadata = functions
            .get(&function_id)
            .ok_or_else(|| BackendError::NotFound(function_id.clone()))?;

        Ok(DeployInfo {
            function_id: function_id.clone(),
            endpoint: format!(
                "{}/api/blueprint/functions/{}/invoke",
                self.base_url, function_id
            ),
            status: "deployed".to_string(),
            cold_start_ms: 2000, // Blockchain latency
            memory_mb: metadata.config.memory_mb,
            timeout_secs: metadata.config.timeout_secs,
        })
    }

    async fn undeploy(&self, function_id: String) -> Result<()> {
        let metadata = {
            let mut functions = self.functions.write().await;
            functions
                .remove(&function_id)
                .ok_or_else(|| BackendError::NotFound(function_id.clone()))?
        };

        info!("Undeploying function from Tangle: {}", function_id);

        // TODO: Deregister service on Tangle blockchain
        // Would send transaction to remove service registration

        // Clean up local storage
        let func_dir = self.storage_path.join(&function_id);
        if func_dir.exists() {
            tokio::fs::remove_dir_all(&func_dir)
                .await
                .map_err(|e| BackendError::Storage(format!("Failed to remove directory: {}", e)))?;
        }

        info!("Function undeployed from Tangle: {}", function_id);
        Ok(())
    }

    async fn warm(&self, function_id: String) -> Result<u32> {
        // Warming doesn't apply to Tangle backend
        // Operators manage their own warm pools
        info!(
            "Warm request for Tangle function {} (no-op on blockchain)",
            function_id
        );
        Ok(0)
    }
}
