use serde::{Deserialize, Serialize};

/// Represents a container snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub name: Option<String>,
    pub container_id: String,
    pub created_at: String,
    pub size_bytes: u64,
}

/// Represents a running instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: Option<String>,
    pub image: String,
    pub status: String,
    pub created_at: String,
    pub cpu_cores: Option<u32>,
    pub memory_mb: Option<u32>,
}