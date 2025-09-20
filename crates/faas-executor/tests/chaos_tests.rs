//! Edge case and failure mode tests
//! Tests resource exhaustion, race conditions, error recovery, and system limits

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use anyhow::Result;

/// Resource manager for testing resource exhaustion scenarios
pub struct ResourceManager {
    cpu_cores: AtomicUsize,
    memory_mb: AtomicUsize,
    disk_gb: AtomicUsize,
    network_bandwidth_mbps: AtomicUsize,
    max_containers: usize,
    active_containers: Arc<Mutex<Vec<String>>>,
}

impl ResourceManager {
    pub fn new(cpu_cores: usize, memory_mb: usize, disk_gb: usize) -> Self {
        Self {
            cpu_cores: AtomicUsize::new(cpu_cores),
            memory_mb: AtomicUsize::new(memory_mb),
            disk_gb: AtomicUsize::new(disk_gb),
            network_bandwidth_mbps: AtomicUsize::new(1000),
            max_containers: 100,
            active_containers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn allocate_resources(
        &self,
        cpu: usize,
        memory: usize,
        disk: usize,
    ) -> Result<ResourceAllocation> {
        // Check CPU
        let current_cpu = self.cpu_cores.load(Ordering::SeqCst);
        if current_cpu < cpu {
            return Err(anyhow::anyhow!("Insufficient CPU resources: {} available, {} requested",
                current_cpu, cpu));
        }

        // Check memory
        let current_memory = self.memory_mb.load(Ordering::SeqCst);
        if current_memory < memory {
            return Err(anyhow::anyhow!("Insufficient memory: {} MB available, {} MB requested",
                current_memory, memory));
        }

        // Check disk
        let current_disk = self.disk_gb.load(Ordering::SeqCst);
        if current_disk < disk {
            return Err(anyhow::anyhow!("Insufficient disk: {} GB available, {} GB requested",
                current_disk, disk));
        }

        // Atomically allocate resources
        self.cpu_cores.fetch_sub(cpu, Ordering::SeqCst);
        self.memory_mb.fetch_sub(memory, Ordering::SeqCst);
        self.disk_gb.fetch_sub(disk, Ordering::SeqCst);

        Ok(ResourceAllocation {
            cpu_count: cpu,
            memory_mb: memory,
            disk_gb: disk,
            allocated_at: Instant::now(),
        })
    }

    pub fn release_resources(&self, allocation: ResourceAllocation) {
        self.cpu_cores.fetch_add(allocation.cpu_count, Ordering::SeqCst);
        self.memory_mb.fetch_add(allocation.memory_mb, Ordering::SeqCst);
        self.disk_gb.fetch_add(allocation.disk_gb, Ordering::SeqCst);
    }

    pub fn get_available_resources(&self) -> (usize, usize, usize) {
        (
            self.cpu_cores.load(Ordering::SeqCst),
            self.memory_mb.load(Ordering::SeqCst),
            self.disk_gb.load(Ordering::SeqCst),
        )
    }
}

#[derive(Debug)]
pub struct ResourceAllocation {
    cpu_count: usize,
    memory_mb: usize,
    disk_gb: usize,
    allocated_at: Instant,
}

/// Circuit breaker for testing failure recovery
pub struct CircuitBreaker {
    failure_count: AtomicUsize,
    success_count: AtomicUsize,
    state: Arc<RwLock<CircuitState>>,
    failure_threshold: usize,
    success_threshold: usize,
    timeout: Duration,
    last_state_change: Arc<RwLock<Instant>>,
}

#[derive(Clone, Debug, PartialEq)]
enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: usize, success_threshold: usize, timeout: Duration) -> Self {
        Self {
            failure_count: AtomicUsize::new(0),
            success_count: AtomicUsize::new(0),
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_threshold,
            success_threshold,
            timeout,
            last_state_change: Arc::new(RwLock::new(Instant::now())),
        }
    }

    pub async fn call<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        let current_state = self.state.read().await.clone();
        
        match current_state {
            CircuitState::Open => {
                // Check if timeout has passed
                let last_change = *self.last_state_change.read().await;
                if last_change.elapsed() > self.timeout {
                    // Transition to half-open
                    *self.state.write().await = CircuitState::HalfOpen;
                    *self.last_state_change.write().await = Instant::now();
                } else {
                    return Err(anyhow::anyhow!("Circuit breaker is open"));
                }
            }
            _ => {}
        }
        
        // Try the operation
        match f() {
            Ok(result) => {
                self.on_success().await;
                Ok(result)
            }
            Err(e) => {
                self.on_failure().await;
                Err(e)
            }
        }
    }

    async fn on_success(&self) {
        let current_state = self.state.read().await.clone();
        
        match current_state {
            CircuitState::HalfOpen => {
                let count = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;
                if count >= self.success_threshold {
                    // Close the circuit
                    *self.state.write().await = CircuitState::Closed;
                    self.failure_count.store(0, Ordering::SeqCst);
                    self.success_count.store(0, Ordering::SeqCst);
                    *self.last_state_change.write().await = Instant::now();
                }
            }
            CircuitState::Closed => {
                self.failure_count.store(0, Ordering::SeqCst);
            }
            _ => {}
        }
    }

    async fn on_failure(&self) {
        let current_state = self.state.read().await.clone();
        
        match current_state {
            CircuitState::Closed => {
                let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
                if count >= self.failure_threshold {
                    // Open the circuit
                    *self.state.write().await = CircuitState::Open;
                    *self.last_state_change.write().await = Instant::now();
                }
            }
            CircuitState::HalfOpen => {
                // Immediately open on failure in half-open state
                *self.state.write().await = CircuitState::Open;
                self.success_count.store(0, Ordering::SeqCst);
                *self.last_state_change.write().await = Instant::now();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resource_exhaustion() {
        let manager = ResourceManager::new(4, 8192, 100);
        
        // Allocate all CPU cores
        let alloc1 = manager.allocate_resources(2, 2048, 10).await.unwrap();
        let alloc2 = manager.allocate_resources(2, 2048, 10).await.unwrap();
        
        // Try to allocate more - should fail
        let result = manager.allocate_resources(1, 1024, 10).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Insufficient CPU"));
        
        // Release one allocation
        manager.release_resources(alloc1);
        
        // Now allocation should succeed
        let alloc3 = manager.allocate_resources(1, 1024, 10).await.unwrap();
        assert_eq!(manager.get_available_resources().0, 1); // 1 CPU core left
    }

    #[tokio::test]
    async fn test_memory_exhaustion() {
        let manager = ResourceManager::new(8, 4096, 100);
        
        // Allocate most memory
        let alloc1 = manager.allocate_resources(1, 3500, 10).await.unwrap();
        
        // Try to allocate more than available
        let result = manager.allocate_resources(1, 1000, 10).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Insufficient memory"));
        
        // Smaller allocation should work
        let alloc2 = manager.allocate_resources(1, 500, 10).await.unwrap();
        let (_, memory, _) = manager.get_available_resources();
        assert_eq!(memory, 96); // 4096 - 3500 - 500
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_on_failures() {
        let breaker = CircuitBreaker::new(3, 2, Duration::from_millis(100));
        let failure_count = Arc::new(AtomicUsize::new(0));
        
        // Simulate failures
        for _ in 0..3 {
            let count = failure_count.clone();
            let result = breaker.call(|| {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(anyhow::anyhow!("Service unavailable"))
            }).await;
            assert!(result.is_err());
        }
        
        // Circuit should be open now
        let result = breaker.call(|| Ok("test")).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Circuit breaker is open"));
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_recovery() {
        let breaker = CircuitBreaker::new(2, 2, Duration::from_millis(50));
        
        // Open the circuit
        for _ in 0..2 {
            let _ = breaker.call(|| Err::<(), _>(anyhow::anyhow!("fail"))).await;
        }
        
        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(60)).await;
        
        // Circuit should be half-open, successes should close it
        for _ in 0..2 {
            let result = breaker.call(|| Ok("success")).await;
            assert!(result.is_ok());
        }
        
        // Circuit should be closed now
        assert_eq!(*breaker.state.read().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_concurrent_resource_allocation_race() {
        let manager = Arc::new(ResourceManager::new(4, 4096, 100));
        let mut handles = vec![];
        
        // Spawn 10 tasks trying to allocate resources concurrently
        for i in 0..10 {
            let mgr = manager.clone();
            let handle = tokio::spawn(async move {
                mgr.allocate_resources(1, 500, 5).await
            });
            handles.push(handle);
        }
        
        // Collect results
        let mut successful = 0;
        let mut failed = 0;
        
        for handle in handles {
            match handle.await.unwrap() {
                Ok(_) => successful += 1,
                Err(_) => failed += 1,
            }
        }
        
        // Only 4 should succeed (4 CPU cores)
        assert_eq!(successful, 4);
        assert_eq!(failed, 6);
    }

    #[tokio::test]
    async fn test_deadlock_prevention() {
        // Test that multiple resources can be acquired without deadlock
        let resource_a = Arc::new(Mutex::new(100));
        let resource_b = Arc::new(Mutex::new(200));
        
        let mut handles = vec![];
        
        // Task 1: Acquires A then B
        let a1 = resource_a.clone();
        let b1 = resource_b.clone();
        handles.push(tokio::spawn(async move {
            let _lock_a = a1.lock().await;
            tokio::time::sleep(Duration::from_millis(10)).await;
            let _lock_b = b1.lock().await;
            "task1"
        }));
        
        // Task 2: Also acquires A then B (same order - no deadlock)
        let a2 = resource_a.clone();
        let b2 = resource_b.clone();
        handles.push(tokio::spawn(async move {
            let _lock_a = a2.lock().await;
            tokio::time::sleep(Duration::from_millis(10)).await;
            let _lock_b = b2.lock().await;
            "task2"
        }));
        
        // Both tasks should complete
        let results = futures::future::join_all(handles).await;
        assert_eq!(results.len(), 2);
        for result in results {
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_cascade_failure_prevention() {
        // Simulate cascading failure scenario
        struct Service {
            name: String,
            dependencies: Arc<RwLock<Vec<Arc<Service>>>>,
            healthy: Arc<RwLock<bool>>,
            breaker: CircuitBreaker,
        }

        impl Service {
            fn new(name: String) -> Arc<Self> {
                Arc::new(Self {
                    name,
                    dependencies: Arc::new(RwLock::new(Vec::new())),
                    healthy: Arc::new(RwLock::new(true)),
                    breaker: CircuitBreaker::new(2, 1, Duration::from_millis(50)),
                })
            }
            
            async fn call(&self) -> Result<String> {
                if !*self.healthy.read().await {
                    return Err(anyhow::anyhow!("Service {} is unhealthy", self.name));
                }

                // Check dependencies directly (not recursively to avoid boxing)
                let deps = self.dependencies.read().await;
                for dep in deps.iter() {
                    // Check dependency health
                    if !*dep.healthy.read().await {
                        return Err(anyhow::anyhow!("Dependency {} is unhealthy", dep.name));
                    }
                    // Also check dependency's dependencies
                    let dep_deps = dep.dependencies.read().await;
                    for dep_dep in dep_deps.iter() {
                        if !*dep_dep.healthy.read().await {
                            return Err(anyhow::anyhow!("Transitive dependency {} is unhealthy", dep_dep.name));
                        }
                    }
                }

                Ok(format!("Response from {}", self.name))
            }
        }
        
        let service_a = Service::new("A".to_string());
        let service_b = Service::new("B".to_string());
        let service_c = Service::new("C".to_string());

        // C depends on B, B depends on A
        service_b.dependencies.write().await.push(service_a.clone());
        service_c.dependencies.write().await.push(service_b.clone());
        
        // A fails
        *service_a.healthy.write().await = false;
        
        // C should fail due to dependency
        let result = service_c.call().await;
        assert!(result.is_err());
        
        // But C itself remains healthy
        assert!(*service_c.healthy.read().await);
    }

    #[tokio::test]
    async fn test_retry_with_exponential_backoff() {
        let attempt_count = Arc::new(AtomicUsize::new(0));
        let start = Instant::now();
        
        async fn retry_with_backoff<F, T>(
            mut f: F,
            max_attempts: usize,
        ) -> Result<T>
        where
            F: FnMut() -> Result<T>,
        {
            let mut attempt = 0;
            let mut backoff = Duration::from_millis(10);
            
            loop {
                match f() {
                    Ok(result) => return Ok(result),
                    Err(e) if attempt >= max_attempts - 1 => return Err(e),
                    _ => {
                        tokio::time::sleep(backoff).await;
                        backoff *= 2; // Exponential backoff
                        attempt += 1;
                    }
                }
            }
        }
        
        let count = attempt_count.clone();
        let result = retry_with_backoff(
            || {
                let current = count.fetch_add(1, Ordering::SeqCst);
                if current < 3 {
                    Err(anyhow::anyhow!("Temporary failure"))
                } else {
                    Ok("Success")
                }
            },
            5,
        ).await;
        
        assert!(result.is_ok());
        assert_eq!(attempt_count.load(Ordering::SeqCst), 4); // Failed 3 times, succeeded on 4th
        
        // Should have taken at least 10 + 20 + 40 = 70ms
        assert!(start.elapsed() >= Duration::from_millis(70));
    }

    #[tokio::test]
    async fn test_rate_limiting_under_load() {
        use tokio::sync::Semaphore;
        
        // Rate limiter allowing 10 requests per second
        let rate_limiter = Arc::new(Semaphore::new(10));
        let request_count = Arc::new(AtomicUsize::new(0));
        
        // Spawn 100 concurrent requests
        let mut handles = vec![];
        for _ in 0..100 {
            let limiter = rate_limiter.clone();
            let count = request_count.clone();
            
            handles.push(tokio::spawn(async move {
                let _permit = limiter.acquire().await.unwrap();
                count.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(100)).await;
                // Permit dropped here
            }));
        }
        
        // After 50ms, only 10 should be processing
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(request_count.load(Ordering::SeqCst) <= 10);
        
        // Wait for all to complete
        for handle in handles {
            handle.await.unwrap();
        }
        
        assert_eq!(request_count.load(Ordering::SeqCst), 100);
    }
}
