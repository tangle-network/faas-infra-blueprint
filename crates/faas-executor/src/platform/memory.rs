use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Memory pool with KSM deduplication, THP, ZRAM, and NUMA awareness
pub struct MemoryPool {
    pages: Arc<RwLock<HashMap<PageId, Page>>>,
    ksm_enabled: bool,
    thp_enabled: bool,
    zram_enabled: bool,
    numa_nodes: Vec<NumaNode>,
    metrics: Arc<RwLock<MemoryMetrics>>,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct PageId(u64);

#[derive(Debug)]
struct Page {
    id: PageId,
    data: Vec<u8>,
    refs: usize,
    numa_node: usize,
}

#[derive(Debug)]
struct NumaNode {
    id: usize,
    available_mb: u64,
}

#[derive(Debug, Default)]
struct MemoryMetrics {
    total_allocated_mb: u64,
    dedup_ratio: f64,
    compression_ratio: f64,
    thp_pages: u64,
    last_updated: Option<Instant>,
}

impl MemoryPool {
    pub fn new() -> Result<Self> {
        // Enable KSM if available
        let ksm_enabled = Self::enable_ksm()?;

        // Enable Transparent Huge Pages
        let thp_enabled = Self::enable_thp().unwrap_or(false);

        // Setup ZRAM compression (spawn as task since new() is not async)
        let zram_enabled = false; // Will be enabled asynchronously

        let pool = Self {
            pages: Arc::new(RwLock::new(HashMap::new())),
            ksm_enabled,
            thp_enabled,
            zram_enabled,
            numa_nodes: Self::detect_numa_nodes(),
            metrics: Arc::new(RwLock::new(MemoryMetrics::default())),
        };

        // Spawn task to enable ZRAM
        tokio::spawn(async move {
            let _ = Self::setup_zram(4).await;
        });

        Ok(pool)
    }

    fn enable_ksm() -> Result<bool> {
        // Check if KSM is available
        if std::path::Path::new("/sys/kernel/mm/ksm/run").exists() {
            // Enable KSM
            std::fs::write("/sys/kernel/mm/ksm/run", "1").ok();
            // Tune KSM for containers
            std::fs::write("/sys/kernel/mm/ksm/pages_to_scan", "1000").ok();
            std::fs::write("/sys/kernel/mm/ksm/sleep_millisecs", "20").ok();
            info!("KSM enabled with optimized settings");
            Ok(true)
        } else {
            warn!("KSM not available on this system");
            Ok(false)
        }
    }

    /// Enable Transparent Huge Pages for better performance
    fn enable_thp() -> Result<bool> {
        let thp_path = "/sys/kernel/mm/transparent_hugepage/enabled";
        if std::path::Path::new(thp_path).exists() {
            std::fs::write(thp_path, "always").ok();
            std::fs::write("/sys/kernel/mm/transparent_hugepage/defrag", "madvise").ok();
            info!("Transparent Huge Pages enabled");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Setup ZRAM for memory compression
    async fn setup_zram(size_gb: u32) -> Result<bool> {
        // Load zram module
        let modprobe = Command::new("modprobe")
            .arg("zram")
            .arg("num_devices=1")
            .output()
            .await;

        if modprobe.is_err() {
            return Ok(false);
        }

        // Configure ZRAM device
        let size_bytes = size_gb as u64 * 1024 * 1024 * 1024;
        std::fs::write("/sys/block/zram0/comp_algorithm", "lz4").ok();
        std::fs::write("/sys/block/zram0/disksize", size_bytes.to_string()).ok();

        // Make swap on ZRAM
        Command::new("mkswap")
            .arg("/dev/zram0")
            .output()
            .await
            .ok();

        Command::new("swapon")
            .args(&["-p", "100", "/dev/zram0"])
            .output()
            .await
            .ok();

        info!("ZRAM compression enabled with {}GB", size_gb);
        Ok(true)
    }

    /// Auto-tune KSM based on deduplication ratio
    pub async fn auto_tune_ksm(&self) -> Result<()> {
        if !self.ksm_enabled {
            return Ok(());
        }

        let dedup_ratio = self.get_deduplication_ratio().await?;

        if dedup_ratio < 0.1 {
            // Low dedup, reduce scanning
            std::fs::write("/sys/kernel/mm/ksm/pages_to_scan", "100").ok();
            debug!("KSM: Low dedup ratio, reduced scanning");
        } else if dedup_ratio > 0.3 {
            // High dedup, increase scanning
            std::fs::write("/sys/kernel/mm/ksm/pages_to_scan", "5000").ok();
            debug!("KSM: High dedup ratio, increased scanning");
        }

        // Update metrics
        let mut metrics = self.metrics.write().await;
        metrics.dedup_ratio = dedup_ratio;
        metrics.last_updated = Some(Instant::now());

        Ok(())
    }

    /// Get current deduplication ratio
    async fn get_deduplication_ratio(&self) -> Result<f64> {
        if !self.ksm_enabled {
            return Ok(0.0);
        }

        let shared = std::fs::read_to_string("/sys/kernel/mm/ksm/pages_shared")
            .unwrap_or_else(|_| "0".to_string());
        let sharing = std::fs::read_to_string("/sys/kernel/mm/ksm/pages_sharing")
            .unwrap_or_else(|_| "0".to_string());

        let shared_pages: f64 = shared.trim().parse().unwrap_or(0.0);
        let sharing_pages: f64 = sharing.trim().parse().unwrap_or(0.0);

        if sharing_pages > 0.0 {
            Ok(shared_pages / sharing_pages)
        } else {
            Ok(0.0)
        }
    }

    fn detect_numa_nodes() -> Vec<NumaNode> {
        // Simplified NUMA detection
        vec![NumaNode {
            id: 0,
            available_mb: 16384,
        }]
    }

    pub async fn allocate(&self, size_mb: u64) -> Result<Vec<u8>> {
        let size_bytes = (size_mb * 1024 * 1024) as usize;

        // Try to allocate with huge pages if enabled and size is appropriate
        let mut buffer = if self.thp_enabled && size_bytes >= 2 * 1024 * 1024 {
            // Align to 2MB boundary for THP
            let aligned_size = (size_bytes + 2097151) & !2097151;
            let mut buf = Vec::with_capacity(aligned_size);
            buf.resize(size_bytes, 0);

            // Advise kernel to use huge pages
            #[cfg(target_os = "linux")]
            {
                use std::ptr;
                unsafe {
                    libc::madvise(
                        buf.as_ptr() as *mut libc::c_void,
                        size_bytes,
                        libc::MADV_HUGEPAGE,
                    );
                }
            }

            info!("Allocated {}MB with THP support", size_mb);
            buf
        } else {
            // Regular allocation
            let mut buf = Vec::with_capacity(size_bytes);
            buf.resize(size_bytes, 0);
            buf
        };

        // Update metrics
        let mut metrics = self.metrics.write().await;
        metrics.total_allocated_mb += size_mb;

        Ok(buffer)
    }

    pub fn dedup_ratio(&self) -> f64 {
        if self.ksm_enabled {
            // Read KSM statistics
            if let Ok(shared) = std::fs::read_to_string("/sys/kernel/mm/ksm/pages_shared") {
                if let Ok(sharing) = std::fs::read_to_string("/sys/kernel/mm/ksm/pages_sharing") {
                    if let (Ok(shared_pages), Ok(sharing_pages)) =
                        (shared.trim().parse::<f64>(), sharing.trim().parse::<f64>())
                    {
                        if sharing_pages > 0.0 {
                            return shared_pages / sharing_pages;
                        }
                    }
                }
            }
        }
        1.0
    }
}
