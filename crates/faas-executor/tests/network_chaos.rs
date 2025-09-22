//! Network partition and chaos testing

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Network conditions simulator
pub struct NetworkChaos {
    latency_ms: Arc<RwLock<u64>>,
    packet_loss_rate: Arc<RwLock<f64>>,
    partitioned: Arc<RwLock<bool>>,
    bandwidth_kbps: Arc<RwLock<Option<u64>>>,
}

impl NetworkChaos {
    pub fn new() -> Self {
        Self {
            latency_ms: Arc::new(RwLock::new(0)),
            packet_loss_rate: Arc::new(RwLock::new(0.0)),
            partitioned: Arc::new(RwLock::new(false)),
            bandwidth_kbps: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn partition(&self) {
        *self.partitioned.write().await = true;
    }

    pub async fn heal(&self) {
        *self.partitioned.write().await = false;
    }

    pub async fn add_latency(&self, ms: u64) {
        *self.latency_ms.write().await = ms;
    }

    pub async fn set_packet_loss(&self, rate: f64) {
        *self.packet_loss_rate.write().await = rate.clamp(0.0, 1.0);
    }

    pub async fn throttle_bandwidth(&self, kbps: u64) {
        *self.bandwidth_kbps.write().await = Some(kbps);
    }

    pub async fn simulate_request(&self, data: Vec<u8>) -> Result<Vec<u8>> {
        // Check partition
        if *self.partitioned.read().await {
            return Err(anyhow::anyhow!("Network partitioned"));
        }

        // Simulate packet loss
        if rand::random::<f64>() < *self.packet_loss_rate.read().await {
            return Err(anyhow::anyhow!("Packet lost"));
        }

        // Add latency
        let latency = *self.latency_ms.read().await;
        if latency > 0 {
            tokio::time::sleep(Duration::from_millis(latency)).await;
        }

        // Simulate bandwidth throttling
        if let Some(kbps) = *self.bandwidth_kbps.read().await {
            let bytes_per_ms = kbps * 1024 / 8000;
            let transfer_time_ms = data.len() as u64 / bytes_per_ms.max(1);
            tokio::time::sleep(Duration::from_millis(transfer_time_ms)).await;
        }

        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_network_partition() {
        let chaos = NetworkChaos::new();

        // Normal operation
        assert!(chaos.simulate_request(vec![1, 2, 3]).await.is_ok());

        // Partition network
        chaos.partition().await;
        assert!(chaos.simulate_request(vec![1, 2, 3]).await.is_err());

        // Heal partition
        chaos.heal().await;
        assert!(chaos.simulate_request(vec![1, 2, 3]).await.is_ok());
    }

    #[tokio::test]
    async fn test_packet_loss() {
        let chaos = NetworkChaos::new();
        chaos.set_packet_loss(0.5).await;

        let mut successes = 0;
        let mut failures = 0;

        for _ in 0..100 {
            match chaos.simulate_request(vec![1]).await {
                Ok(_) => successes += 1,
                Err(_) => failures += 1,
            }
        }

        // With 50% packet loss, expect roughly half to fail
        assert!(failures > 30 && failures < 70);
    }

    #[tokio::test]
    async fn test_latency_injection() {
        let chaos = NetworkChaos::new();
        chaos.add_latency(100).await;

        let start = std::time::Instant::now();
        chaos.simulate_request(vec![1, 2, 3]).await.unwrap();

        assert!(start.elapsed() >= Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_bandwidth_throttling() {
        let chaos = NetworkChaos::new();
        chaos.throttle_bandwidth(10).await; // 10 kbps

        let large_data = vec![0u8; 10_000]; // 10KB
        let start = std::time::Instant::now();
        chaos.simulate_request(large_data).await.unwrap();

        // 10KB at 10kbps should take ~8 seconds
        assert!(start.elapsed() >= Duration::from_secs(7));
    }

    #[tokio::test]
    async fn test_combined_chaos() {
        let chaos = NetworkChaos::new();
        chaos.add_latency(50).await;
        chaos.set_packet_loss(0.1).await;

        let mut results = vec![];
        for _ in 0..10 {
            let start = std::time::Instant::now();
            let result = chaos.simulate_request(vec![1, 2, 3]).await;
            results.push((result.is_ok(), start.elapsed()));
        }

        // Some should fail, all successful ones should have latency
        let successes: Vec<_> = results.iter().filter(|(ok, _)| *ok).collect();

        assert!(!successes.is_empty());
        for (_, duration) in successes {
            assert!(*duration >= Duration::from_millis(50));
        }
    }
}
