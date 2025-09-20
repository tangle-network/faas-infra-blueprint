//! Firecracker microVM integration module

pub mod vm_manager;

pub use vm_manager::{FirecrackerManager, VmConfig, VmInstance, VmState, NetworkConfig};