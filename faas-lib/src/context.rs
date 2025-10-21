use blueprint_sdk::{
    alloy::{
        primitives::{keccak256, Address, Bytes, U256},
        providers::{Provider, ProviderBuilder, RootProvider},
        rpc::types::{
            eth::{
                transaction::{TransactionInput, TransactionRequest},
                BlockNumberOrTag,
            },
            BlockId,
        },
    },
    contexts::tangle::TangleClientContext as TangleClientContextTrait,
    crypto::sp_core::SpEcdsa,
    error::Error as SdkError,
    info,
    keystore::{backends::Backend, Error as KeystoreError, Keystore},
    macros::context::{KeystoreContext, ServicesContext, TangleClientContext},
    runner::config::BlueprintEnvironment,
    tangle_subxt::tangle_testnet_runtime::api,
};
use faas_executor::platform::Executor as PlatformExecutor;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::PublicKey;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use thiserror::Error;
use tokio::{sync::Mutex, time::sleep};
use tracing::warn;

const ASSIGNMENT_SIGNATURE: &str = "getAssignedOperatorForJob(uint64)";
static ASSIGNMENT_SELECTOR: OnceLock<[u8; 4]> = OnceLock::new();

#[derive(Error, Debug)]
pub enum BlueprintLibError {
    #[error("Platform executor initialization failed: {0}")]
    PlatformExecutor(String),
    #[error("Blueprint SDK error: {0}")]
    Sdk(#[from] SdkError),
    #[error("Keystore error: {0}")]
    Keystore(#[from] KeystoreError),
    #[error("Contract error: {0}")]
    Contract(String),
}

#[derive(Clone, TangleClientContext, ServicesContext, KeystoreContext)]
pub struct FaaSContext {
    #[config]
    pub config: BlueprintEnvironment,
    pub executor: Arc<PlatformExecutor>,
    pub state: Arc<ContextState>,
    assignment: Option<AssignmentClient>,
}

impl FaaSContext {
    pub async fn new(config: BlueprintEnvironment) -> Result<Self, BlueprintLibError> {
        info!(
            "Initializing platform executor (data_dir: {}, keystore: {})",
            config.data_dir.to_string_lossy(),
            config.keystore_uri
        );

        let executor = PlatformExecutor::new()
            .await
            .map_err(|e| BlueprintLibError::PlatformExecutor(e.to_string()))?;

        let force_enable_assignment = std::env::var("FAAS_ENABLE_CONTRACT_ASSIGNMENT").is_ok();
        let disable_assignment_env = std::env::var("FAAS_DISABLE_CONTRACT_ASSIGNMENT").is_ok();

        let disable_assignment = if force_enable_assignment {
            false
        } else if disable_assignment_env {
            true
        } else {
            config.test_mode
        };

        let assignment = if disable_assignment {
            info!("Contract assignment disabled for this context");
            None
        } else if matches!(
            config.protocol_settings,
            blueprint_sdk::runner::config::ProtocolSettings::Tangle(_)
        ) {
            let assignment = AssignmentClient::new(&config).await?;
            info!(
                "Configured contract load balancer (contract: {}, operator: {})",
                assignment.contract_hex(),
                assignment.operator_hex()
            );
            Some(assignment)
        } else {
            info!("No Tangle protocol configured; skipping contract assignment setup");
            None
        };

        Ok(Self {
            config,
            executor: Arc::new(executor),
            state: Arc::new(ContextState::default()),
            assignment,
        })
    }

    /// Check if this operator is assigned to execute a job
    /// Returns true if assigned, false if should skip
    pub async fn is_assigned_to_job(&self, job_call_id: u64) -> Result<bool, BlueprintLibError> {
        info!("Job assignment check for job_call_id: {}", job_call_id);

        let Some(assignment) = &self.assignment else {
            return Ok(true);
        };

        match assignment.assigned_operator(job_call_id).await {
            Ok(Address::ZERO) => Ok(false),
            Ok(operator) => {
                if operator == assignment.operator_address() {
                    info!(
                        "Job {} assigned to local operator {} (executing)",
                        job_call_id,
                        assignment.operator_hex()
                    );
                    Ok(true)
                } else {
                    info!(
                        "Job {} assigned to {}, local operator {} will skip",
                        job_call_id,
                        format!("{:#x}", operator),
                        assignment.operator_hex()
                    );
                    Ok(false)
                }
            }
            Err(err) => {
                warn!(
                    "Failed to query assignment for job {}: {} (executing to stay compatible)",
                    job_call_id, err
                );
                Ok(true)
            }
        }
    }
}

#[derive(Clone)]
struct AssignmentClient {
    provider: RootProvider,
    rpc_endpoint: String,
    contract_address: Address,
    contract_address_hex: String,
    operator_address: Address,
}

impl AssignmentClient {
    async fn new(config: &BlueprintEnvironment) -> Result<Self, BlueprintLibError> {
        let keystore = config.keystore();
        let operator_address = operator_evm_address(&keystore)?;
        let contract_address = fetch_contract_address(config).await?;
        let contract_address_hex = format!("{:#x}", contract_address);

        let rpc_endpoint = config.http_rpc_endpoint.to_string();
        let url = Url::parse(&rpc_endpoint)
            .map_err(|e| BlueprintLibError::Contract(format!("Invalid HTTP RPC endpoint: {e}")))?;
        let provider = ProviderBuilder::new()
            .disable_recommended_fillers()
            .connect_http(url)
            .root()
            .clone();

        Ok(Self {
            provider,
            rpc_endpoint,
            contract_address,
            contract_address_hex,
            operator_address,
        })
    }

    async fn assigned_operator(&self, job_call_id: u64) -> Result<Address, BlueprintLibError> {
        const MAX_RETRIES: usize = 10;

        for attempt in 0..MAX_RETRIES {
            match self.query_assigned_operator(job_call_id).await {
                Ok(address) if address == Address::ZERO && attempt + 1 < MAX_RETRIES => {
                    let backoff_ms = 100 * (attempt as u64 + 1);
                    sleep(Duration::from_millis(backoff_ms)).await;
                    continue;
                }
                Ok(address) => return Ok(address),
                Err(err) if attempt + 1 < MAX_RETRIES => {
                    warn!(
                        "Retrying assignment lookup for job {} (attempt {}): {}",
                        job_call_id,
                        attempt + 1,
                        err
                    );
                    sleep(Duration::from_millis(25 * (attempt as u64 + 1))).await;
                }
                Err(err) => return Err(err),
            }
        }

        Ok(Address::ZERO)
    }

    fn operator_address(&self) -> Address {
        self.operator_address
    }

    fn operator_hex(&self) -> String {
        format!("{:#x}", self.operator_address)
    }

    fn contract_hex(&self) -> String {
        self.contract_address_hex.clone()
    }

    async fn query_assigned_operator(
        &self,
        job_call_id: u64,
    ) -> Result<Address, BlueprintLibError> {
        if self.rpc_endpoint.is_empty() {
            return Err(BlueprintLibError::Contract(
                "RPC endpoint not configured for assignment lookup".into(),
            ));
        }

        let call_data = build_assignment_call(job_call_id);
        let tx = TransactionRequest::default()
            .to(self.contract_address)
            .input(TransactionInput::new(call_data));

        let result = self
            .provider
            .call(tx)
            .block(BlockId::Number(BlockNumberOrTag::Latest))
            .await
            .map_err(|e| BlueprintLibError::Contract(format!("eth_call failed: {e}")))?;

        parse_address_from_bytes(result.as_ref())
            .map_err(|e| BlueprintLibError::Contract(format!("Failed to parse assignment: {e}")))
    }
}

fn build_assignment_call(job_call_id: u64) -> Bytes {
    let selector = ASSIGNMENT_SELECTOR.get_or_init(|| {
        let hash = keccak256(ASSIGNMENT_SIGNATURE.as_bytes());
        [hash[0], hash[1], hash[2], hash[3]]
    });

    let mut payload = [0u8; 36];
    payload[..4].copy_from_slice(selector);
    payload[4..].copy_from_slice(&U256::from(job_call_id).to_be_bytes::<32>());
    Bytes::copy_from_slice(&payload)
}

fn parse_address_from_bytes(raw: &[u8]) -> Result<Address, String> {
    match raw.len() {
        0 => Ok(Address::ZERO),
        20 => {
            let mut address = [0u8; 20];
            address.copy_from_slice(raw);
            Ok(Address::from_slice(&address))
        }
        32 => {
            let mut address = [0u8; 20];
            address.copy_from_slice(&raw[12..]);
            Ok(Address::from_slice(&address))
        }
        len => Err(format!("unexpected result length {len}")),
    }
}

fn operator_evm_address(keystore: &Keystore) -> Result<Address, BlueprintLibError> {
    let ecdsa_pub = keystore.first_local::<SpEcdsa>()?;
    let compressed = ecdsa_pub.0 .0;

    let public_key = PublicKey::from_sec1_bytes(&compressed)
        .map_err(|e| BlueprintLibError::Contract(format!("Invalid compressed ECDSA key: {e}")))?;

    let uncompressed = public_key.to_encoded_point(false);
    let bytes = uncompressed.as_bytes();
    if bytes.len() != 65 {
        return Err(BlueprintLibError::Contract(format!(
            "Unexpected uncompressed key length {}",
            bytes.len()
        )));
    }

    let hash = keccak256(bytes);
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..]);
    Ok(Address::from_slice(&address))
}

async fn fetch_contract_address(
    config: &BlueprintEnvironment,
) -> Result<Address, BlueprintLibError> {
    let settings = config
        .protocol_settings
        .tangle()
        .map_err(|e| BlueprintLibError::Contract(e.to_string()))?;

    let client = match TangleClientContextTrait::tangle_client(config).await {
        Ok(client) => client,
        Err(err) => {
            return Err(BlueprintLibError::Contract(format!(
                "Failed to create Tangle client: {err}"
            )))
        }
    };

    let storage = client
        .services_client()
        .rpc_client
        .storage()
        .at_latest()
        .await
        .map_err(|e| BlueprintLibError::Contract(format!("Failed to access storage: {e}")))?;

    let call = api::storage().services().blueprints(settings.blueprint_id);
    let blueprint = storage
        .fetch(&call)
        .await
        .map_err(|e| BlueprintLibError::Contract(format!("Failed to fetch blueprint: {e}")))?
        .ok_or_else(|| {
            BlueprintLibError::Contract(format!(
                "Blueprint {} not found in storage",
                settings.blueprint_id
            ))
        })?
        .1;

    use api::runtime_types::tangle_primitives::services::service::BlueprintServiceManager;

    match blueprint.manager {
        BlueprintServiceManager::Evm(addr) => Ok(Address::from_slice(&addr.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_assignment_call_encodes_selector_and_argument() {
        let job_id = 0x1234_u64;
        let payload = build_assignment_call(job_id);
        assert_eq!(payload.len(), 36);

        let selector = &keccak256(ASSIGNMENT_SIGNATURE.as_bytes())[..4];
        assert_eq!(&payload[..4], selector);

        let expected_argument = U256::from(job_id).to_be_bytes::<32>();
        assert_eq!(&payload[4..], &expected_argument[..]);
    }

    #[test]
    fn parse_address_from_bytes_handles_common_variants() {
        assert_eq!(parse_address_from_bytes(&[]).unwrap(), Address::ZERO);

        let raw = vec![0x11u8; 20];
        let parsed = parse_address_from_bytes(&raw).unwrap();
        assert_eq!(format!("{:#x}", parsed), format!("0x{}", "11".repeat(20)));

        let mut padded = vec![0u8; 12];
        padded.extend_from_slice(&raw);
        let parsed = parse_address_from_bytes(&padded).unwrap();
        assert_eq!(format!("{:#x}", parsed), format!("0x{}", "11".repeat(20)));
    }
}

#[derive(Default, Debug)]
pub struct ContextState {
    pub snapshots: Mutex<HashMap<String, SnapshotRecord>>,
    pub branches: Mutex<HashMap<String, BranchRecord>>,
    pub instances: Mutex<HashMap<String, InstanceRecord>>,
    pub checkpoints: Mutex<HashMap<String, CheckpointRecord>>,
    pub exposures: Mutex<HashMap<String, ExposureRecord>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnapshotRecord {
    pub snapshot_id: String,
    pub container_id: String,
    pub name: String,
    pub description: Option<String>,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BranchRecord {
    pub branch_id: String,
    pub parent_snapshot_id: String,
    pub name: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum InstanceStatus {
    Running,
    Paused,
    Stopped,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstanceRecord {
    pub instance_id: String,
    pub image: Option<String>,
    pub snapshot_id: Option<String>,
    pub cpu_cores: u32,
    pub memory_mb: u32,
    pub disk_gb: u32,
    pub enable_ssh: bool,
    pub status: InstanceStatus,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointRecord {
    pub checkpoint_id: String,
    pub instance_id: String,
    pub created_from_snapshot: Option<String>,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExposureRecord {
    pub exposure_id: String,
    pub instance_id: String,
    pub url: String,
    pub protocol: String,
    pub subdomain: Option<String>,
    pub path: PathBuf,
}
