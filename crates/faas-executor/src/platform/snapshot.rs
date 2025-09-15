use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};



#[derive(Debug, Clone)]
pub struct Snapshot {
    pub id: String,
    pub exec_id: String,
    pub backend: Backend,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub created_at: std::time::Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Backend {
    Criu,
    Firecracker,
}

pub struct SnapshotStore {
    snapshots: Arc<RwLock<HashMap<String, Snapshot>>>,
    storage_path: PathBuf,
    criu: CriuManager,
    firecracker: FirecrackerSnapshots,
}

struct CriuManager {
    bin_path: PathBuf,
    work_dir: PathBuf,
}

struct FirecrackerSnapshots {
    snapshots_dir: PathBuf,
}

impl SnapshotStore {
    pub async fn new() -> Result<Self> {
        let storage_path = PathBuf::from("/var/lib/faas/snapshots");
        tokio::fs::create_dir_all(&storage_path).await?;

        Ok(Self {
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            storage_path: storage_path.clone(),
            criu: CriuManager {
                bin_path: PathBuf::from("/usr/sbin/criu"),
                work_dir: storage_path.join("criu"),
            },
            firecracker: FirecrackerSnapshots {
                snapshots_dir: storage_path.join("firecracker"),
            },
        })
    }

    pub async fn create(&self, exec_id: &str) -> Result<String> {
        let snapshot_id = format!("snap-{}-{}", exec_id, uuid::Uuid::new_v4());

        // Determine backend based on execution type
        let backend = Backend::Criu; // Start with CRIU

        let snapshot = match backend {
            Backend::Criu => self.create_criu_snapshot(exec_id, &snapshot_id).await?,
            Backend::Firecracker => self.create_firecracker_snapshot(exec_id, &snapshot_id).await?,
        };

        let mut snapshots = self.snapshots.write().await;
        snapshots.insert(snapshot_id.clone(), snapshot);

        Ok(snapshot_id)
    }

    pub async fn restore(&self, snapshot_id: &str) -> Result<String> {
        let snapshots = self.snapshots.read().await;
        let snapshot = snapshots.get(snapshot_id)
            .ok_or_else(|| anyhow::anyhow!("Snapshot not found: {}", snapshot_id))?;

        let exec_id = match snapshot.backend {
            Backend::Criu => self.restore_criu_snapshot(snapshot).await?,
            Backend::Firecracker => self.restore_firecracker_snapshot(snapshot).await?,
        };

        Ok(exec_id)
    }

    async fn create_criu_snapshot(&self, exec_id: &str, snapshot_id: &str) -> Result<Snapshot> {
        let snapshot_dir = self.criu.work_dir.join(snapshot_id);
        tokio::fs::create_dir_all(&snapshot_dir).await?;

        // Execute CRIU checkpoint
        let output = tokio::process::Command::new(&self.criu.bin_path)
            .args(&[
                "dump",
                "--tree", exec_id,
                "--images-dir", snapshot_dir.to_str().unwrap(),
                "--leave-running",
                "--tcp-established",
                "--ext-unix-sk",
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("CRIU checkpoint failed: {}",
                String::from_utf8_lossy(&output.stderr)));
        }

        // Calculate snapshot size
        let size_bytes = Self::dir_size(&snapshot_dir).await?;

        Ok(Snapshot {
            id: snapshot_id.to_string(),
            exec_id: exec_id.to_string(),
            backend: Backend::Criu,
            path: snapshot_dir,
            size_bytes,
            created_at: std::time::Instant::now(),
        })
    }

    async fn restore_criu_snapshot(&self, snapshot: &Snapshot) -> Result<String> {
        let new_exec_id = format!("exec-{}", uuid::Uuid::new_v4());

        // Execute CRIU restore
        let output = tokio::process::Command::new(&self.criu.bin_path)
            .args(&[
                "restore",
                "--images-dir", snapshot.path.to_str().unwrap(),
                "--pidfile", &format!("/tmp/{}.pid", new_exec_id),
                "--tcp-established",
                "--ext-unix-sk",
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("CRIU restore failed: {}",
                String::from_utf8_lossy(&output.stderr)));
        }

        Ok(new_exec_id)
    }

    async fn create_firecracker_snapshot(&self, exec_id: &str, snapshot_id: &str) -> Result<Snapshot> {
        let snapshot_dir = self.firecracker.snapshots_dir.join(snapshot_id);
        tokio::fs::create_dir_all(&snapshot_dir).await?;

        // Create Firecracker snapshot
        // This would integrate with Firecracker's snapshot API
        let snapshot_path = snapshot_dir.join("vm_state");
        let mem_path = snapshot_dir.join("memory");

        // Placeholder implementation
        tokio::fs::write(&snapshot_path, b"firecracker_state").await?;
        tokio::fs::write(&mem_path, b"memory_content").await?;

        let size_bytes = Self::dir_size(&snapshot_dir).await?;

        Ok(Snapshot {
            id: snapshot_id.to_string(),
            exec_id: exec_id.to_string(),
            backend: Backend::Firecracker,
            path: snapshot_dir,
            size_bytes,
            created_at: std::time::Instant::now(),
        })
    }

    async fn restore_firecracker_snapshot(&self, snapshot: &Snapshot) -> Result<String> {
        let new_exec_id = format!("vm-{}", uuid::Uuid::new_v4());

        // Restore Firecracker VM from snapshot
        // This would use Firecracker's restore API

        Ok(new_exec_id)
    }

    async fn dir_size(path: &PathBuf) -> Result<u64> {
        let mut size = 0;
        let mut entries = tokio::fs::read_dir(path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_file() {
                size += metadata.len();
            } else if metadata.is_dir() {
                // Use non-recursive approach to avoid stack overflow
                let dir_size = std::fs::read_dir(entry.path())?
                    .map(|entry| entry.unwrap().metadata().unwrap().len())
                    .sum::<u64>();
                size += dir_size;
            }
        }

        Ok(size)
    }

    pub async fn cleanup_old(&self, max_age: std::time::Duration) -> Result<usize> {
        let mut removed = 0;
        let mut snapshots = self.snapshots.write().await;
        let now = std::time::Instant::now();

        let expired: Vec<String> = snapshots
            .iter()
            .filter(|(_, snapshot)| now.duration_since(snapshot.created_at) > max_age)
            .map(|(id, _)| id.clone())
            .collect();

        for id in expired {
            if let Some(snapshot) = snapshots.remove(&id) {
                tokio::fs::remove_dir_all(&snapshot.path).await.ok();
                removed += 1;
            }
        }

        Ok(removed)
    }
}