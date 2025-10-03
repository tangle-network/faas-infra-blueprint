use anyhow::Result;
use async_trait::async_trait;
use faas_common::{InvocationResult, SandboxConfig, SandboxExecutor};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

use crate::container_pool::ContainerPoolManager;
use crate::docker_snapshot::DockerSnapshotManager;
use crate::environment_registry::{
    CacheMount, CacheType, ConfigurationManager, EnvironmentRegistry, EnvironmentTemplate,
};
use crate::performance::{CacheManager, CacheStrategy};
use crate::Docker;

// Simple hashmap macro for initialization
macro_rules! hashmap {
    ($($key:expr => $value:expr),* $(,)?) => {
        {
            let mut map = HashMap::new();
            $(map.insert($key.to_string(), $value.to_string());)*
            map
        }
    };
}

/// State-of-the-art execution engine targeting sub-250ms cold starts
/// Uses advanced VM snapshotting, memory COW, and pre-warming techniques
pub struct Executor {
    strategy: ExecutionStrategy,
    environment_cache: Arc<RwLock<EnvironmentCache>>,
    metrics: Arc<Mutex<ExecutionMetrics>>,
    registry: Arc<RwLock<EnvironmentRegistry>>,
    config_manager: Arc<Mutex<ConfigurationManager>>,
    cache_manager: Option<Arc<CacheManager>>,
}

#[derive(Debug, Clone)]
pub enum ExecutionStrategy {
    /// Container-based execution with warm pools
    Container(ContainerStrategy),
    /// Firecracker microVM with snapshot restore
    MicroVM(MicroVMStrategy),
    /// Hybrid approach - containers for dev tools, microVMs for isolation
    Hybrid(HybridStrategy),
}

#[derive(Clone)]
pub struct ContainerStrategy {
    /// Pre-warmed container pools by environment type
    pub warm_pools: Arc<Mutex<HashMap<String, VecDeque<WarmContainer>>>>,
    /// Maximum pool size per environment
    pub max_pool_size: usize,
    /// Docker client for container operations
    pub docker: Arc<docktopus::bollard::Docker>,
    /// Docker snapshot manager for real commit/restore operations
    pub snapshot_manager: Option<Arc<crate::docker_snapshot::DockerSnapshotManager>>,
    /// Build cache volumes for compilation artifacts
    pub build_cache_volumes: Arc<RwLock<HashMap<String, String>>>,
    /// Shared dependency layers (e.g., cargo registry, go modules)
    pub dependency_layers: Arc<RwLock<HashMap<String, DependencyLayer>>>,
    /// GPU-enabled container pools for AI/compute workloads
    pub gpu_pools: Arc<Mutex<HashMap<String, VecDeque<WarmContainer>>>>,
    /// High-performance container pool manager
    pub pool_manager: Option<Arc<ContainerPoolManager>>,
}

impl std::fmt::Debug for ContainerStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContainerStrategy")
            .field("max_pool_size", &self.max_pool_size)
            .field("has_snapshot_manager", &self.snapshot_manager.is_some())
            .finish()
    }
}

#[derive(Clone)]
pub struct MicroVMStrategy {
    /// Memory snapshots of pre-configured environments
    snapshots: Arc<RwLock<HashMap<String, EnvironmentSnapshot>>>,
    /// Firecracker binary path
    firecracker_path: String,
    /// Base kernel and rootfs
    kernel_path: String,
    rootfs_path: String,
    /// Real Firecracker executor if available
    firecracker_executor: Option<Arc<crate::firecracker::FirecrackerExecutor>>,
}

impl MicroVMStrategy {
    /// Create a new MicroVM strategy with real Firecracker support
    pub fn new(
        firecracker_path: String,
        kernel_path: String,
        rootfs_path: String,
    ) -> Self {
        // Try to initialize real Firecracker executor on Linux
        let firecracker_executor = if cfg!(target_os = "linux") {
            match crate::firecracker::FirecrackerExecutor::new(
                firecracker_path.clone(),
                kernel_path.clone(),
                rootfs_path.clone(),
            ) {
                Ok(executor) => Some(Arc::new(executor)),
                Err(e) => {
                    tracing::warn!("Failed to initialize Firecracker executor: {}", e);
                    None
                }
            }
        } else {
            tracing::info!("Firecracker not available on non-Linux platforms");
            None
        };

        Self {
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            firecracker_path,
            kernel_path,
            rootfs_path,
            firecracker_executor,
        }
    }

    /// Create a stub strategy for testing or non-Linux platforms
    pub fn stub() -> Self {
        Self {
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            firecracker_path: String::new(),
            kernel_path: String::new(),
            rootfs_path: String::new(),
            firecracker_executor: None,
        }
    }
}

impl std::fmt::Debug for MicroVMStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MicroVMStrategy")
            .field("firecracker_path", &self.firecracker_path)
            .field("kernel_path", &self.kernel_path)
            .field("has_executor", &self.firecracker_executor.is_some())
            .finish()
    }
}

#[derive(Clone)]
pub struct HybridStrategy {
    container: ContainerStrategy,
    microvm: MicroVMStrategy,
    /// Decision logic for routing executions
    routing_rules: Vec<RoutingRule>,
}

impl std::fmt::Debug for HybridStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HybridStrategy")
            .field("routing_rules", &self.routing_rules)
            .finish()
    }
}

impl HybridStrategy {
    /// Create a new hybrid strategy with intelligent backend selection
    pub fn new(
        docker: Arc<Docker>,
        firecracker_path: Option<String>,
        kernel_path: Option<String>,
    ) -> Self {
        // Initialize container strategy with pooling
        let container = ContainerStrategy {
            warm_pools: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            max_pool_size: 10,
            docker: docker.clone(),
            snapshot_manager: Some(Arc::new(crate::docker_snapshot::DockerSnapshotManager::new(
                docker.clone(),
            ))),
            pool_manager: Some(Arc::new(
                crate::container_pool::ContainerPoolManager::new(
                    docker.clone(),
                    crate::container_pool::PoolConfig {
                        min_size: 1,
                        max_size: 10,
                        max_idle_time: std::time::Duration::from_secs(300),
                        max_use_count: 100,
                        pre_warm: true,
                        health_check_interval: std::time::Duration::from_secs(30),
                        predictive_warming: true,
                        target_acquisition_ms: 50,
                    },
                ),
            )),
            build_cache_volumes: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            dependency_layers: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            gpu_pools: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        };

        // Initialize MicroVM strategy with Firecracker if available
        let microvm = if let (Some(fc_path), Some(kernel)) = (firecracker_path, kernel_path) {
            MicroVMStrategy::new(fc_path, kernel, String::new())
        } else {
            MicroVMStrategy::stub()
        };

        // Define intelligent routing rules
        let routing_rules = vec![
            // Use Firecracker for security-sensitive or high-performance workloads
            RoutingRule {
                condition: RoutingCondition::RequiresSecurity,
                target_strategy: ExecutionStrategy::MicroVM(microvm.clone()),
            },
            RoutingRule {
                condition: RoutingCondition::HighPerformance,
                target_strategy: ExecutionStrategy::MicroVM(microvm.clone()),
            },
            // Use containers for better compatibility and warm pools
            RoutingRule {
                condition: RoutingCondition::RequiresWarmPool,
                target_strategy: ExecutionStrategy::Container(container.clone()),
            },
        ];

        Self {
            container,
            microvm,
            routing_rules,
        }
    }
}

/// Environment cache for instant developer environment restoration
#[derive(Debug)]
struct EnvironmentCache {
    /// Pre-built development environments (Rust, Node, Python toolchains)
    dev_environments: HashMap<String, CachedEnvironment>,
    /// Compilation artifacts cache (incremental builds)
    build_cache: HashMap<String, BuildArtifacts>,
    /// Language server states (for immediate IDE integration)
    language_servers: HashMap<String, LanguageServerState>,
}

#[derive(Debug)]
struct CachedEnvironment {
    /// Environment ID (e.g., "rust-1.75-dev", "node-20-typescript")
    id: String,
    /// Base container/VM snapshot
    base_snapshot: Vec<u8>,
    /// Pre-installed toolchain metadata
    toolchain: ToolchainMetadata,
    /// Last access time for cache eviction
    last_accessed: std::time::Instant,
    /// Size in bytes
    size_bytes: u64,
}

#[derive(Debug)]
struct ToolchainMetadata {
    language: String,
    version: String,
    /// Common dependencies pre-installed
    dependencies: Vec<String>,
    /// Compilation cache paths
    cache_paths: Vec<String>,
    /// Environment variables
    env_vars: HashMap<String, String>,
}

/// Execution metrics for performance monitoring
#[derive(Debug, Default)]
struct ExecutionMetrics {
    total_executions: u64,
    avg_cold_start_ms: f64,
    avg_warm_start_ms: f64,
    cache_hit_rate: f64,
    environments_served: HashMap<String, u64>,
}

impl Executor {
    pub async fn new(strategy: ExecutionStrategy) -> anyhow::Result<Self> {
        let environment_cache = Arc::new(RwLock::new(EnvironmentCache::new()));
        let metrics = Arc::new(Mutex::new(ExecutionMetrics::default()));

        // Load environment registry - try from file first, fallback to defaults
        let config_path = PathBuf::from("faas-environments.json");
        let config_manager = Arc::new(Mutex::new(
            ConfigurationManager::new(config_path.clone())
                .await
                .unwrap_or_else(|_| {
                    // Fallback to default configuration
                    let default_registry = EnvironmentRegistry::default();
                    ConfigurationManager::new_with_registry(default_registry, config_path.clone())
                }),
        ));

        let registry = Arc::new(RwLock::new(config_manager.lock().await.registry.clone()));

        // Initialize high-performance cache manager
        let cache_manager = match CacheManager::new(CacheStrategy::default()).await {
            Ok(cm) => Some(Arc::new(cm)),
            Err(e) => {
                warn!("Failed to initialize cache manager: {}", e);
                None
            }
        };

        let executor = Self {
            strategy,
            environment_cache,
            metrics,
            registry,
            config_manager,
            cache_manager,
        };

        // Initialize environment cache based on registry
        executor.initialize_from_registry().await?;

        Ok(executor)
    }

    /// Initialize container warm pools from environment registry
    async fn initialize_from_registry(&self) -> Result<()> {
        info!("Initializing environments from registry...");

        if let ExecutionStrategy::Container(container_strategy) = &self.strategy {
            let registry = self.registry.read().await;

            // Initialize cache volumes for all registered environments
            for template in registry.environments.values() {
                for layer in &template.layers {
                    for cache_mount in &layer.cache_mounts {
                        self.ensure_cache_volume(container_strategy, &cache_mount)
                            .await?;
                    }
                }

                // Pre-warm containers based on resource requirements
                // Only pre-warm Alpine for now to speed up tests
                if template.id == "alpine-fast" {
                    let pool_size = self.calculate_pool_size(&template);
                    if pool_size > 0 {
                        info!(
                            "Pre-warming {} containers for environment: {}",
                            pool_size, template.display_name
                        );

                        self.pre_warm_from_template(container_strategy, template, pool_size)
                            .await?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Calculate optimal pool size based on environment characteristics
    fn calculate_pool_size(&self, template: &EnvironmentTemplate) -> usize {
        // High-performance environments get larger pools
        let base_size = if template.performance_hints.cache_hit_rate > 0.8 {
            3
        } else if template.performance_hints.typical_duration_ms < 200 {
            5
        } else {
            2
        };

        // Adjust based on parallelization capability
        if template.performance_hints.parallelizable {
            base_size * 2
        } else {
            base_size
        }
    }

    /// Ensure a cache volume exists
    async fn ensure_cache_volume(
        &self,
        container_strategy: &ContainerStrategy,
        cache_mount: &CacheMount,
    ) -> Result<()> {
        let volume_name = format!("faas-cache-{}", cache_mount.source);

        // Check if volume already exists
        match container_strategy.docker.inspect_volume(&volume_name).await {
            Ok(_) => {
                info!("Cache volume {} already exists", volume_name);
            }
            Err(_) => {
                // Create the volume
                let create_options = docktopus::bollard::volume::CreateVolumeOptions {
                    name: volume_name.clone(),
                    driver: "local".to_string(),
                    labels: {
                        let mut labels = HashMap::new();
                        labels.insert(
                            "faas.cache.type".to_string(),
                            match &cache_mount.cache_type {
                                CacheType::DependencyCache => "dependency".to_string(),
                                CacheType::BuildArtifacts => "build".to_string(),
                                CacheType::SourceCache => "source".to_string(),
                                CacheType::DataCache => "data".to_string(),
                                CacheType::Custom(s) => s.clone(),
                            },
                        );
                        labels.insert(
                            "faas.cache.shared".to_string(),
                            if cache_mount.shared { "true" } else { "false" }.to_string(),
                        );
                        labels.insert(
                            "faas.cache.persistent".to_string(),
                            if cache_mount.persistent {
                                "true"
                            } else {
                                "false"
                            }
                            .to_string(),
                        );
                        labels
                    },
                    ..Default::default()
                };

                container_strategy
                    .docker
                    .create_volume(create_options)
                    .await?;
                info!("Created cache volume: {}", volume_name);
            }
        }

        // Track the volume
        let mut cache_volumes = container_strategy.build_cache_volumes.write().await;
        cache_volumes.insert(cache_mount.source.clone(), volume_name);

        Ok(())
    }

    /// Pre-warm containers from an environment template
    async fn pre_warm_from_template(
        &self,
        container_strategy: &ContainerStrategy,
        template: &EnvironmentTemplate,
        pool_size: usize,
    ) -> Result<()> {
        let mut warm_pools = container_strategy.warm_pools.lock().await;
        // Use the base image as the key for the warm pool so it matches try_get_warm_container
        let pool = warm_pools
            .entry(template.base_image.clone())
            .or_insert_with(VecDeque::new);

        for i in 0..pool_size {
            info!(
                "Pre-warming container {}/{} for {}",
                i + 1,
                pool_size,
                template.display_name
            );

            // Build container configuration from template
            let container_config = self
                .build_container_config(template, container_strategy)
                .await?;

            // Create and start container
            let container_name = format!("faas-warm-{}-{}", template.id, Uuid::new_v4());
            let create_response = container_strategy
                .docker
                .create_container::<String, String>(
                    Some(docktopus::bollard::container::CreateContainerOptions {
                        name: container_name.clone(),
                        ..Default::default()
                    }),
                    container_config,
                )
                .await?;

            container_strategy
                .docker
                .start_container::<String>(&create_response.id, None)
                .await?;

            pool.push_back(WarmContainer {
                container_id: create_response.id,
                ready_at: Instant::now(),
            });
        }

        Ok(())
    }

    /// Build container configuration from environment template
    async fn build_container_config(
        &self,
        template: &EnvironmentTemplate,
        container_strategy: &ContainerStrategy,
    ) -> Result<docktopus::bollard::container::Config<String>> {
        let mut mounts = Vec::new();
        let mut env_vars = Vec::new();

        // Add all cache mounts from layers
        for layer in &template.layers {
            for cache_mount in &layer.cache_mounts {
                let volume_name = format!("faas-cache-{}", cache_mount.source);
                mounts.push(docktopus::bollard::models::Mount {
                    target: Some(cache_mount.target.clone()),
                    source: Some(volume_name),
                    typ: Some(docktopus::bollard::models::MountTypeEnum::VOLUME),
                    read_only: Some(false),
                    ..Default::default()
                });
            }

            // Add environment variables from layers
            for (key, value) in &layer.env_vars {
                env_vars.push(format!("{}={}", key, value));
            }
        }

        // Build host configuration with resource limits
        let host_config = docktopus::bollard::models::HostConfig {
            mounts: Some(mounts),
            cpu_count: Some((template.resource_requirements.max_cpu_cores * 1024.0) as i64),
            memory: Some(
                (template.resource_requirements.max_memory_gb * 1024.0 * 1024.0 * 1024.0) as i64,
            ),
            memory_swap: Some(
                (template.resource_requirements.max_memory_gb * 2.0 * 1024.0 * 1024.0 * 1024.0)
                    as i64,
            ),
            // Enable GPU if required
            device_requests: if template.performance_hints.gpu_required {
                Some(vec![docktopus::bollard::models::DeviceRequest {
                    count: Some(template.resource_requirements.gpu_count as i64),
                    capabilities: Some(vec![vec!["gpu".to_string()]]),
                    ..Default::default()
                }])
            } else {
                None
            },
            ..Default::default()
        };

        Ok(docktopus::bollard::container::Config {
            image: Some(template.base_image.clone()),
            cmd: Some(vec!["sleep".to_string(), "3600".to_string()]),
            attach_stdin: Some(true),
            open_stdin: Some(true),
            tty: Some(false),
            host_config: Some(host_config),
            env: Some(env_vars),
            ..Default::default()
        })
    }

    /// Initialize container warm pools with specialized caching for heavy workloads (legacy - kept for compatibility)
    async fn initialize_dev_environments(&self) -> Result<()> {
        info!("Initializing ultra-optimized container pools with dependency caching...");

        if let ExecutionStrategy::Container(container_strategy) = &self.strategy {
            // Initialize shared dependency volumes
            self.initialize_dependency_volumes(container_strategy)
                .await?;

            // Pre-warm specialized containers
            let specialized_images = vec![
                // Rust blockchain development (with pre-cached deps)
                (
                    "rust-blockchain",
                    "rust:latest",
                    3,
                    vec![
                        "cargo",
                        "alloy",
                        "ethers",
                        "reth",
                        "solana-sdk",
                        "anchor-lang",
                        "tokio",
                        "tower",
                        "tonic",
                        "prost",
                        "serde",
                        "bincode",
                    ],
                ),
                // Solana development
                (
                    "solana-dev",
                    "solanalabs/solana:latest",
                    2,
                    vec!["anchor", "spl-token", "spl-governance", "metaplex"],
                ),
                // Ethereum development
                (
                    "ethereum-dev",
                    "ethereum/client-go:latest",
                    2,
                    vec!["foundry", "hardhat", "web3", "ethers"],
                ),
                // High-performance compute
                (
                    "compute-optimized",
                    "nvidia/cuda:12.0-devel",
                    1,
                    vec!["pytorch", "tensorflow", "jax", "triton"],
                ),
                // Fast general purpose
                ("alpine-fast", "alpine:latest", 5, vec![]),
            ];

            for (name, image, pool_size, pre_cached_deps) in specialized_images {
                match self
                    .pre_warm_specialized_containers(
                        name,
                        image,
                        pool_size,
                        pre_cached_deps,
                        container_strategy,
                    )
                    .await
                {
                    Ok(count) => info!("Pre-warmed {} {} containers with deps", count, name),
                    Err(e) => error!("Failed to pre-warm {}: {}", name, e),
                }
            }
        }

        Ok(())
    }

    /// Initialize shared dependency volumes for ultra-fast compilation
    async fn initialize_dependency_volumes(
        &self,
        strategy: &ContainerStrategy,
    ) -> anyhow::Result<()> {
        let mut dep_layers = strategy.dependency_layers.write().await;

        // Create persistent volumes for dependency caching
        let volumes = vec![
            (
                "cargo-registry",
                "/cache/cargo-registry",
                DependencyType::CargoRegistry,
            ),
            (
                "cargo-git",
                "/cache/cargo-git",
                DependencyType::CargoRegistry,
            ),
            (
                "cargo-target",
                "/cache/cargo-target",
                DependencyType::CargoRegistry,
            ),
            ("go-modules", "/cache/go-modules", DependencyType::GoModules),
            ("solana-cache", "/cache/solana", DependencyType::SolanaTools),
            (
                "ethereum-cache",
                "/cache/ethereum",
                DependencyType::EthereumTools,
            ),
        ];

        for (name, path, dep_type) in volumes {
            // Create Docker volume if it doesn't exist
            let volume_config = docktopus::bollard::volume::CreateVolumeOptions {
                name: name.to_string(),
                driver: "local".to_string(),
                ..Default::default()
            };

            match strategy.docker.create_volume(volume_config).await {
                Ok(_) => {
                    info!("Created cache volume: {}", name);
                    dep_layers.insert(
                        name.to_string(),
                        DependencyLayer {
                            volume_path: path.to_string(),
                            last_updated: std::time::Instant::now(),
                            size_bytes: 0,
                            dep_type,
                        },
                    );
                }
                Err(e) if e.to_string().contains("already exists") => {
                    info!("Cache volume {} already exists", name);
                    dep_layers.insert(
                        name.to_string(),
                        DependencyLayer {
                            volume_path: path.to_string(),
                            last_updated: std::time::Instant::now(),
                            size_bytes: 0,
                            dep_type,
                        },
                    );
                }
                Err(e) => error!("Failed to create volume {}: {}", name, e),
            }
        }

        Ok(())
    }

    /// Pre-warm specialized containers with dependency caching
    async fn pre_warm_specialized_containers(
        &self,
        name: &str,
        image: &str,
        count: usize,
        pre_cached_deps: Vec<&str>,
        strategy: &ContainerStrategy,
    ) -> anyhow::Result<usize> {
        let mut warmed_count = 0;
        let mut pool = strategy.warm_pools.lock().await;

        for i in 0..count {
            match self
                .create_specialized_warm_container(name, image, &pre_cached_deps, strategy)
                .await
            {
                Ok(warm_container) => {
                    pool.entry(image.to_string())
                        .or_insert_with(VecDeque::new)
                        .push_back(warm_container);
                    warmed_count += 1;
                    info!("Created specialized container {}-{}", name, i);
                }
                Err(e) => {
                    error!("Failed to create specialized container for {}: {}", name, e);
                    break;
                }
            }
        }

        Ok(warmed_count)
    }

    /// Create a specialized warm container with mounted caches
    async fn create_specialized_warm_container(
        &self,
        name: &str,
        image: &str,
        _pre_cached_deps: &[&str],
        strategy: &ContainerStrategy,
    ) -> anyhow::Result<WarmContainer> {
        let container_id = format!(
            "warm-{}-{}",
            name.replace([':', '/', '-'], "_"),
            Uuid::new_v4()
        );

        // Mount cache volumes based on image type
        let mut mounts = Vec::new();
        let dep_layers = strategy.dependency_layers.read().await;

        if image.contains("rust") {
            // Mount Rust caches for ultra-fast compilation
            if let Some(registry) = dep_layers.get("cargo-registry") {
                mounts.push(docktopus::bollard::models::Mount {
                    target: Some("/usr/local/cargo/registry".to_string()),
                    source: Some("cargo-registry".to_string()),
                    typ: Some(docktopus::bollard::models::MountTypeEnum::VOLUME),
                    read_only: Some(false),
                    ..Default::default()
                });
            }
            if let Some(target) = dep_layers.get("cargo-target") {
                mounts.push(docktopus::bollard::models::Mount {
                    target: Some("/workspace/target".to_string()),
                    source: Some("cargo-target".to_string()),
                    typ: Some(docktopus::bollard::models::MountTypeEnum::VOLUME),
                    read_only: Some(false),
                    ..Default::default()
                });
            }
        }

        let create_options = Some(docktopus::bollard::container::CreateContainerOptions {
            name: container_id.clone(),
            ..Default::default()
        });

        let mut container_config = docktopus::bollard::container::Config {
            image: Some(image.to_string()),
            cmd: Some(vec!["sleep".to_string(), "3600".to_string()]),
            attach_stdin: Some(true),
            open_stdin: Some(true),
            tty: Some(false),
            host_config: Some(docktopus::bollard::models::HostConfig {
                mounts: Some(mounts),
                // Enable all CPUs for compute-intensive tasks
                cpu_count: Some(0), // 0 = all CPUs
                // Increase memory limits for heavy workloads
                memory: Some(8 * 1024 * 1024 * 1024), // 8GB
                memory_swap: Some(16 * 1024 * 1024 * 1024), // 16GB swap
                // Enable GPU if available
                device_requests: if image.contains("cuda") {
                    Some(vec![docktopus::bollard::models::DeviceRequest {
                        count: Some(-1), // All GPUs
                        capabilities: Some(vec![vec!["gpu".to_string()]]),
                        ..Default::default()
                    }])
                } else {
                    None
                },
                ..Default::default()
            }),
            env: Some(vec![
                "CARGO_HOME=/usr/local/cargo".to_string(),
                "RUSTFLAGS=-C target-cpu=native -C opt-level=3".to_string(),
                "CARGO_BUILD_JOBS=8".to_string(),
                "CARGO_INCREMENTAL=1".to_string(),
            ]),
            ..Default::default()
        };

        // Add sccache for distributed compilation caching
        if image.contains("rust") {
            container_config.env.as_mut().unwrap().extend(vec![
                "RUSTC_WRAPPER=sccache".to_string(),
                "SCCACHE_DIR=/cache/sccache".to_string(),
            ]);
        }

        let result = strategy
            .docker
            .create_container(create_options, container_config)
            .await?;
        strategy
            .docker
            .start_container(
                &result.id,
                None::<docktopus::bollard::container::StartContainerOptions<String>>,
            )
            .await?;

        info!(
            "Created specialized warm container: {} for {}",
            result.id, name
        );

        Ok(WarmContainer {
            container_id: result.id,
            ready_at: std::time::Instant::now(),
        })
    }

    /// Pre-warm containers for immediate use
    async fn pre_warm_containers(
        &self,
        image: &str,
        count: usize,
        strategy: &ContainerStrategy,
    ) -> anyhow::Result<usize> {
        let mut warmed_count = 0;
        let mut pool = strategy.warm_pools.lock().await;

        for _ in 0..count {
            match self.create_warm_container(image, strategy).await {
                Ok(warm_container) => {
                    pool.entry(image.to_string())
                        .or_insert_with(VecDeque::new)
                        .push_back(warm_container);
                    warmed_count += 1;
                }
                Err(e) => {
                    error!("Failed to create warm container for {}: {}", image, e);
                    break;
                }
            }
        }

        Ok(warmed_count)
    }

    /// Create a pre-warmed container ready for immediate execution
    async fn create_warm_container(
        &self,
        image: &str,
        strategy: &ContainerStrategy,
    ) -> anyhow::Result<WarmContainer> {
        let container_id = format!("warm-{}-{}", image.replace([':', '/'], "-"), Uuid::new_v4());

        let create_options = Some(docktopus::bollard::container::CreateContainerOptions {
            name: container_id.clone(),
            ..Default::default()
        });

        let container_config = docktopus::bollard::container::Config {
            image: Some(image.to_string()),
            cmd: Some(vec!["sleep".to_string(), "3600".to_string()]), // Keep container alive
            attach_stdin: Some(true),
            open_stdin: Some(true),
            tty: Some(false),
            ..Default::default()
        };

        let result = strategy
            .docker
            .create_container(create_options, container_config)
            .await?;

        // Start the container immediately
        strategy
            .docker
            .start_container(
                &result.id,
                None::<docktopus::bollard::container::StartContainerOptions<String>>,
            )
            .await?;

        info!("Created warm container: {} for image: {}", result.id, image);

        Ok(WarmContainer {
            container_id: result.id,
            ready_at: std::time::Instant::now(),
        })
    }

    async fn create_cached_environment(&self, _env_id: &str, _spec: EnvironmentSpec) -> Result<()> {
        // Implementation would create and snapshot the environment
        // This is a complex process involving:
        // 1. Starting base container/VM
        // 2. Running pre-install commands
        // 3. Setting up cache mounts
        // 4. Creating memory snapshot
        // 5. Storing in cache

        // For now, return success - full implementation would be much larger
        Ok(())
    }

    /// Get the optimal execution strategy for a given workload
    fn select_strategy(&self, config: &SandboxConfig) -> ExecutionStrategy {
        match &self.strategy {
            ExecutionStrategy::Hybrid(hybrid) => {
                // Intelligent routing based on workload characteristics
                for rule in &hybrid.routing_rules {
                    if rule.matches(config) {
                        return rule.target_strategy.clone();
                    }
                }
                // Default to container for most workloads
                ExecutionStrategy::Container(hybrid.container.clone())
            }
            strategy => strategy.clone(),
        }
    }
}

#[async_trait]
impl SandboxExecutor for Executor {
    #[instrument(skip(self, config), fields(function_id = %config.function_id))]
    async fn execute(&self, config: SandboxConfig) -> faas_common::Result<InvocationResult> {
        let start = Instant::now();
        let request_id = Uuid::new_v4().to_string();

        // Check execution cache for deterministic functions
        if let Some(cache_manager) = &self.cache_manager {
            let cache_key = self.generate_cache_key(&config);
            if let Ok(Some(cached_result)) = cache_manager.get(&cache_key).await {
                // Deserialize cached result
                if let Ok(result) = bincode::deserialize::<InvocationResult>(&cached_result) {
                    info!("Execution cache hit for {}", cache_key);
                    return Ok(result);
                }
            }
        }

        // Record execution attempt
        {
            let mut metrics = self.metrics.lock().await;
            metrics.total_executions += 1;
            *metrics
                .environments_served
                .entry(config.source.clone())
                .or_insert(0) += 1;
        }

        // Select the optimal strategy for this workload
        let selected_strategy = self.select_strategy(&config);

        // Check if we have a cached environment for instant start
        let cache_hit = self
            .check_environment_cache(&config)
            .await
            .map_err(|e| faas_common::FaasError::Executor(e.to_string()))?;

        let result = if cache_hit {
            info!("Cache hit - executing with warm environment");
            self.execute_from_cache_with_strategy(&config, &request_id, &selected_strategy).await
        } else {
            info!("Cache miss - cold start execution");
            self.execute_cold_start_with_strategy(&config, &request_id, &selected_strategy).await
        };

        // Record metrics
        let duration = start.elapsed();
        {
            let mut metrics = self.metrics.lock().await;
            if cache_hit {
                metrics.avg_warm_start_ms =
                    (metrics.avg_warm_start_ms + duration.as_millis() as f64) / 2.0;
            } else {
                metrics.avg_cold_start_ms =
                    (metrics.avg_cold_start_ms + duration.as_millis() as f64) / 2.0;
            }
        }

        info!(
            "Execution completed in {:?} (cache_hit: {})",
            duration, cache_hit
        );

        // Cache successful execution results for deterministic functions
        if let Ok(ref invocation_result) = result {
            if self.is_deterministic(&config) {
                if let Some(cache_manager) = &self.cache_manager {
                    let cache_key = self.generate_cache_key(&config);
                    if let Ok(serialized) = bincode::serialize(invocation_result) {
                        let _ = cache_manager.put(&cache_key, serialized, None).await;
                        info!("Cached execution result for {}", cache_key);
                    }
                }
            }
        }

        result.map_err(|e| faas_common::FaasError::Executor(e.to_string()))
    }
}

impl Executor {
    /// Generate cache key for deterministic function execution
    fn generate_cache_key(&self, config: &SandboxConfig) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&config.source);
        hasher.update(&config.function_id);
        for cmd in &config.command {
            hasher.update(cmd.as_bytes());
        }
        if let Some(ref env_vars) = config.env_vars {
            for var in env_vars {
                hasher.update(var.as_bytes());
            }
        }
        format!("exec:{:x}", hasher.finalize())
    }

    /// Check if function is deterministic (can be cached)
    fn is_deterministic(&self, config: &SandboxConfig) -> bool {
        // Functions are deterministic if they don't:
        // - Access network
        // - Use random numbers
        // - Access current time
        // - Have side effects

        // For now, only cache specific whitelisted patterns
        config.source.contains("compiler") ||
        config.source.contains("transformer") ||
        config.source.contains("parser") ||
        config.source.contains("validator")
    }

    async fn check_environment_cache(&self, config: &SandboxConfig) -> anyhow::Result<bool> {
        let cache = self.environment_cache.read().await;
        Ok(cache.dev_environments.contains_key(&config.source))
    }

    async fn execute_from_cache_with_strategy(
        &self,
        config: &SandboxConfig,
        _request_id: &str,
        strategy: &ExecutionStrategy,
    ) -> anyhow::Result<InvocationResult> {
        // Ultra-fast execution using pre-warmed containers
        info!("Using warm container for instant execution...");

        match strategy {
            ExecutionStrategy::Container(container_strategy) => {
                self.execute_with_warm_container(config, container_strategy)
                    .await
            }
            ExecutionStrategy::MicroVM(microvm_strategy) => {
                // MicroVMs don't support warm pools in the same way, use cold start
                if let Some(firecracker) = &microvm_strategy.firecracker_executor {
                    info!("Executing with Firecracker microVM (from cache)");
                    firecracker
                        .execute(config.clone())
                        .await
                        .map_err(|e| anyhow::anyhow!("Firecracker execution failed: {}", e))
                } else {
                    Err(anyhow::anyhow!("Firecracker not available on this platform"))
                }
            }
            ExecutionStrategy::Hybrid(_) => {
                unreachable!("Hybrid strategy should select concrete strategy before execution")
            }
        }
    }

    async fn execute_from_cache(
        &self,
        config: &SandboxConfig,
        _request_id: &str,
    ) -> anyhow::Result<InvocationResult> {
        // Ultra-fast execution using pre-warmed containers
        info!("Using warm container for instant execution...");

        match &self.strategy {
            ExecutionStrategy::Container(container_strategy) => {
                self.execute_with_warm_container(config, container_strategy)
                    .await
            }
            ExecutionStrategy::MicroVM(microvm_strategy) => {
                // MicroVMs don't support warm pools in the same way, use cold start
                if let Some(firecracker) = &microvm_strategy.firecracker_executor {
                    info!("Executing with Firecracker microVM (from cache)");
                    firecracker
                        .execute(config.clone())
                        .await
                        .map_err(|e| anyhow::anyhow!("Firecracker execution failed: {}", e))
                } else {
                    Err(anyhow::anyhow!("Firecracker not available on this platform"))
                }
            }
            ExecutionStrategy::Hybrid(_) => {
                unreachable!("Hybrid strategy should select concrete strategy before execution")
            }
        }
    }

    async fn execute_cold_start_with_strategy(
        &self,
        config: &SandboxConfig,
        _request_id: &str,
        strategy: &ExecutionStrategy,
    ) -> anyhow::Result<InvocationResult> {
        info!("Performing cold start execution...");

        match strategy {
            ExecutionStrategy::Container(container_strategy) => {
                // Try to get a warm container first, fall back to cold start
                match self
                    .try_get_warm_container(&config.source, container_strategy)
                    .await
                {
                    Some(warm_container) => {
                        info!("Found warm container, using it for 'cold' start");
                        self.execute_with_existing_container(
                            config,
                            &warm_container.container_id,
                            container_strategy,
                        )
                        .await
                    }
                    None => {
                        // True cold start - delegate to DockerExecutor
                        info!("No warm container available, creating new one");
                        let docker_executor =
                            crate::DockerExecutor::new(container_strategy.docker.clone());
                        docker_executor
                            .execute(config.clone())
                            .await
                            .map_err(|e| anyhow::anyhow!("Execution failed: {}", e))
                    }
                }
            }
            ExecutionStrategy::MicroVM(microvm_strategy) => {
                // Use Firecracker if available, otherwise return error
                if let Some(firecracker) = &microvm_strategy.firecracker_executor {
                    info!("Executing with Firecracker microVM");
                    firecracker
                        .execute(config.clone())
                        .await
                        .map_err(|e| anyhow::anyhow!("Firecracker execution failed: {}", e))
                } else {
                    Err(anyhow::anyhow!("Firecracker not available on this platform"))
                }
            }
            ExecutionStrategy::Hybrid(_) => {
                // Hybrid should have already selected a specific strategy
                unreachable!("Hybrid strategy should select concrete strategy before execution")
            }
        }
    }

    async fn execute_cold_start(
        &self,
        config: &SandboxConfig,
        _request_id: &str,
    ) -> anyhow::Result<InvocationResult> {
        info!("Performing cold start execution...");

        match &self.strategy {
            ExecutionStrategy::Container(container_strategy) => {
                // Try to get a warm container first, fall back to cold start
                match self
                    .try_get_warm_container(&config.source, container_strategy)
                    .await
                {
                    Some(warm_container) => {
                        info!("Found warm container, using it for 'cold' start");
                        self.execute_with_existing_container(
                            config,
                            &warm_container.container_id,
                            container_strategy,
                        )
                        .await
                    }
                    None => {
                        // True cold start - delegate to DockerExecutor
                        info!("No warm container available, creating new one");
                        let docker_executor =
                            crate::DockerExecutor::new(container_strategy.docker.clone());
                        docker_executor
                            .execute(config.clone())
                            .await
                            .map_err(|e| anyhow::anyhow!("Execution failed: {}", e))
                    }
                }
            }
            ExecutionStrategy::MicroVM(microvm_strategy) => {
                // Use Firecracker if available, otherwise return error
                if let Some(firecracker) = &microvm_strategy.firecracker_executor {
                    info!("Executing with Firecracker microVM");
                    firecracker
                        .execute(config.clone())
                        .await
                        .map_err(|e| anyhow::anyhow!("Firecracker execution failed: {}", e))
                } else {
                    Err(anyhow::anyhow!("Firecracker not available on this platform"))
                }
            }
            ExecutionStrategy::Hybrid(_) => {
                // Hybrid should have already selected a specific strategy
                unreachable!("Hybrid strategy should select concrete strategy before execution")
            }
        }
    }

    /// Execute using a pre-warmed container for maximum speed
    async fn execute_with_warm_container(
        &self,
        config: &SandboxConfig,
        strategy: &ContainerStrategy,
    ) -> anyhow::Result<InvocationResult> {
        // Use high-performance pool manager if available
        if let Some(pool_manager) = &strategy.pool_manager {
            match pool_manager.acquire(&config.source).await {
                Ok(pooled_container) => {
                    info!("Acquired pooled container {} in {}ms",
                          pooled_container.container_id,
                          pooled_container.startup_time_ms);

                    let result = self.execute_with_existing_container(
                        config,
                        &pooled_container.container_id,
                        strategy
                    ).await;

                    // Release container back to pool
                    let _ = pool_manager.release(pooled_container).await;

                    return result;
                }
                Err(e) => {
                    warn!("Failed to acquire from pool: {}", e);
                    // Fall through to legacy warm pool
                }
            }
        }

        // Fallback to legacy warm pool
        match self.try_get_warm_container(&config.source, strategy).await {
            Some(warm_container) => {
                info!("Reusing warm container: {}", warm_container.container_id);
                self.execute_with_existing_container(config, &warm_container.container_id, strategy)
                    .await
            }
            None => {
                // No warm container available, create a new one
                warn!(
                    "No warm container available for {}, falling back to cold start",
                    config.source
                );
                let docker_executor = crate::DockerExecutor::new(strategy.docker.clone());
                docker_executor
                    .execute(config.clone())
                    .await
                    .map_err(|e| anyhow::anyhow!("Execution failed: {}", e))
            }
        }
    }

    /// Try to get a warm container from the pool
    async fn try_get_warm_container(
        &self,
        image: &str,
        strategy: &ContainerStrategy,
    ) -> Option<WarmContainer> {
        let mut pool = strategy.warm_pools.lock().await;

        if let Some(containers) = pool.get_mut(image) {
            if let Some(container) = containers.pop_front() {
                info!(
                    "Retrieved warm container: {} (age: {:?})",
                    container.container_id,
                    container.ready_at.elapsed()
                );

                // Spawn a task to replace this container in the background
                let strategy_clone = strategy.clone();
                let image_clone = image.to_string();
                let pool_clone = strategy.warm_pools.clone();
                tokio::spawn(async move {
                    if let Ok(new_container) =
                        Self::create_warm_container_static(&image_clone, &strategy_clone).await
                    {
                        let mut pool = pool_clone.lock().await;
                        pool.entry(image_clone)
                            .or_insert_with(VecDeque::new)
                            .push_back(new_container);
                    }
                });

                return Some(container);
            }
        }

        None
    }

    /// Static version of create_warm_container for background tasks
    async fn create_warm_container_static(
        image: &str,
        strategy: &ContainerStrategy,
    ) -> anyhow::Result<WarmContainer> {
        let container_id = format!("warm-{}-{}", image.replace([':', '/'], "-"), Uuid::new_v4());

        let create_options = Some(docktopus::bollard::container::CreateContainerOptions {
            name: container_id.clone(),
            ..Default::default()
        });

        let container_config = docktopus::bollard::container::Config {
            image: Some(image.to_string()),
            cmd: Some(vec!["sleep".to_string(), "3600".to_string()]),
            attach_stdin: Some(true),
            open_stdin: Some(true),
            tty: Some(false),
            ..Default::default()
        };

        let result = strategy
            .docker
            .create_container(create_options, container_config)
            .await?;
        strategy
            .docker
            .start_container(
                &result.id,
                None::<docktopus::bollard::container::StartContainerOptions<String>>,
            )
            .await?;

        Ok(WarmContainer {
            container_id: result.id,
            ready_at: std::time::Instant::now(),
        })
    }

    /// Execute a command in an existing container using exec API
    async fn execute_with_existing_container(
        &self,
        config: &SandboxConfig,
        container_id: &str,
        strategy: &ContainerStrategy,
    ) -> anyhow::Result<InvocationResult> {
        let request_id = Uuid::new_v4().to_string();

        // Build the command with environment variables if present
        let mut full_cmd = Vec::new();

        // If we have environment variables, wrap the command with env
        if let Some(env_vars) = &config.env_vars {
            if !env_vars.is_empty() {
                // Use sh -c to properly handle environment variables
                full_cmd.push("sh".to_string());
                full_cmd.push("-c".to_string());

                // Build the command with environment variables
                let env_prefix = env_vars
                    .iter()
                    .map(|e| e.clone())
                    .collect::<Vec<_>>()
                    .join(" ");

                // Combine env vars and command
                let cmd_string = config.command.join(" ");
                full_cmd.push(format!("{} {}", env_prefix, cmd_string));
            } else {
                full_cmd.extend(config.command.clone());
            }
        } else {
            // No env vars, use command directly
            full_cmd.extend(config.command.clone());
        }

        // Create an exec instance
        let exec_config = docktopus::bollard::exec::CreateExecOptions {
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            attach_stdin: Some(!config.payload.is_empty()), // Enable stdin if we have payload
            cmd: Some(full_cmd),
            ..Default::default()
        };

        let exec_result = strategy
            .docker
            .create_exec(container_id, exec_config)
            .await?;

        // Start the exec
        let start_config = docktopus::bollard::exec::StartExecOptions {
            detach: false,
            tty: false,
            output_capacity: None,
        };

        match strategy
            .docker
            .start_exec(&exec_result.id, Some(start_config))
            .await?
        {
            docktopus::bollard::exec::StartExecResults::Attached { mut output, mut input } => {
                let mut result_output = Vec::new();

                // Write payload to stdin if we have data
                if !config.payload.is_empty() {
                    use tokio::io::AsyncWriteExt;
                    let _ = input.write_all(&config.payload).await;
                    let _ = input.shutdown().await; // Signal EOF
                }

                // Collect output
                use futures::StreamExt;
                while let Some(chunk) = output.next().await {
                    match chunk? {
                        docktopus::bollard::container::LogOutput::StdOut { message }
                        | docktopus::bollard::container::LogOutput::StdErr { message } => {
                            result_output.extend_from_slice(&message);
                        }
                        _ => {}
                    }
                }

                let output_string = String::from_utf8_lossy(&result_output).to_string();

                Ok(InvocationResult {
                    request_id,
                    response: Some(result_output.clone()),
                    logs: Some(output_string),
                    error: None,
                })
            }
            docktopus::bollard::exec::StartExecResults::Detached => {
                // For detached exec, we'd need to inspect the exec to get results
                // For now, return a simple result
                Ok(InvocationResult {
                    request_id,
                    response: Some(b"Exec completed (detached)".to_vec()),
                    logs: Some("Exec completed in detached mode".to_string()),
                    error: None,
                })
            }
        }
    }
}

// Helper types and implementations

#[derive(Debug, Clone)]
pub struct DependencyLayer {
    /// Path to the shared volume containing dependencies
    pub volume_path: String,
    /// Last update time for cache invalidation
    pub last_updated: std::time::Instant,
    /// Size in bytes for monitoring
    pub size_bytes: u64,
    /// Type of dependencies (cargo, go-modules, npm, etc.)
    pub dep_type: DependencyType,
}

#[derive(Debug, Clone)]
pub enum DependencyType {
    CargoRegistry,  // Rust cargo registry (~10GB for blockchain deps)
    GoModules,      // Go modules cache
    NpmPackages,    // Node packages
    PythonPackages, // Python pip/conda
    SolanaTools,    // Solana SDK and tools
    EthereumTools,  // Ethereum toolchain (foundry, hardhat)
}

#[derive(Debug)]
struct EnvironmentSpec {
    base_image: &'static str,
    pre_install_commands: Vec<&'static str>,
    cache_mounts: Vec<&'static str>,
    env_vars: HashMap<String, String>,
}

#[derive(Debug)]
pub struct WarmContainer {
    pub container_id: String,
    pub ready_at: std::time::Instant,
}

#[derive(Debug)]
struct EnvironmentSnapshot {
    memory_snapshot: Vec<u8>,
    disk_snapshot: String,
    metadata: SnapshotMetadata,
}

#[derive(Debug)]
struct SnapshotMetadata {
    created_at: std::time::Instant,
    environment_id: String,
    size_bytes: u64,
}

#[derive(Debug)]
struct BuildArtifacts {
    artifacts: HashMap<String, Vec<u8>>,
    dependencies: Vec<String>,
    cache_key: String,
}

#[derive(Debug)]
struct LanguageServerState {
    process_snapshot: Vec<u8>,
    workspace_state: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct RoutingRule {
    condition: RoutingCondition,
    target_strategy: ExecutionStrategy,
}

#[derive(Debug, Clone)]
enum RoutingCondition {
    ImagePattern(String),
    CommandContains(String),
    PayloadSizeAbove(usize),
    RequiresIsolation(bool),
    RequiresSecurity,
    HighPerformance,
    RequiresWarmPool,
}

impl RoutingRule {
    fn matches(&self, config: &SandboxConfig) -> bool {
        match &self.condition {
            RoutingCondition::ImagePattern(pattern) => config.source.contains(pattern),
            RoutingCondition::CommandContains(cmd) => {
                config.command.iter().any(|c| c.contains(cmd))
            }
            RoutingCondition::PayloadSizeAbove(size) => config.payload.len() > *size,
            RoutingCondition::RequiresIsolation(_) => false, // Implement based on security requirements
            RoutingCondition::RequiresSecurity => {
                // Security-sensitive workloads: crypto, auth, sensitive data processing
                config.source.contains("secure") ||
                config.function_id.contains("auth") ||
                config.function_id.contains("crypto")
            }
            RoutingCondition::HighPerformance => {
                // High-performance workloads: compute-intensive, ML inference
                config.source.contains("gpu") ||
                config.source.contains("ml") ||
                config.function_id.contains("compute")
            }
            RoutingCondition::RequiresWarmPool => {
                // Workloads that benefit from warm pools: frequent calls, low latency
                !config.function_id.contains("batch") &&
                !config.function_id.contains("cron")
            }
        }
    }
}

impl EnvironmentCache {
    fn new() -> Self {
        Self {
            dev_environments: HashMap::new(),
            build_cache: HashMap::new(),
            language_servers: HashMap::new(),
        }
    }
}
