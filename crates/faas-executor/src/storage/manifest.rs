//! Snapshot manifests that reference content-addressed blobs

use super::BlobId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Snapshot manifest - references blobs without duplicating content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub kind: ManifestKind,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub entries: Vec<ManifestEntry>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ManifestKind {
    /// CRIU process checkpoint (Linux only)
    #[cfg(target_os = "linux")]
    CriuCheckpoint { pid: u32, images_dir: String },
    /// Firecracker VM snapshot
    FirecrackerSnapshot {
        vm_id: String,
        memory_blob: BlobId,
        state_blob: BlobId,
    },
    /// Docker container layers
    DockerLayers {
        container_id: String,
        base_image: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Path within the snapshot
    pub path: String,
    /// Content-addressed blob ID
    pub blob_id: BlobId,
    /// Original uncompressed size
    pub size: u64,
    /// Optional: file mode for POSIX systems
    pub mode: Option<u32>,
}

impl Manifest {
    pub fn new(id: String, kind: ManifestKind) -> Self {
        Self {
            id,
            kind,
            created_at: chrono::Utc::now(),
            entries: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    pub fn add_entry(&mut self, path: String, blob_id: BlobId, size: u64, mode: Option<u32>) {
        self.entries.push(ManifestEntry {
            path,
            blob_id,
            size,
            mode,
        });
    }

    pub fn total_size(&self) -> u64 {
        self.entries.iter().map(|e| e.size).sum()
    }

    /// Get all unique blob IDs (for deduplication analysis)
    pub fn blob_ids(&self) -> Vec<&BlobId> {
        self.entries.iter().map(|e| &e.blob_id).collect()
    }
}
