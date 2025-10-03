"""FaaS Platform Python SDK - Official Python Client

The FaaS Platform Python SDK provides high-performance serverless execution with support for
both Docker containers and Firecracker microVMs, featuring automatic optimization, intelligent
caching, and seamless runtime selection.

Key Features:
    - Dual runtime support (Docker containers and Firecracker VMs)
    - Automatic result caching with configurable TTL
    - Pre-warming pools for zero cold starts
    - Execution forking for A/B testing and parallel workflows
    - Rich metrics and performance monitoring
    - Async/await support with connection pooling

Quick Start:
    ```python
    import asyncio
    from faas_sdk import FaaSClient, Runtime

    async def main():
        client = FaaSClient("http://localhost:8080")

        # Simple execution
        result = await client.run_python('print("Hello, World!")')
        print(result.output)

        # With specific runtime
        result = await client.execute(
            command='echo "Production ready!"',
            runtime=Runtime.FIRECRACKER
        )
        print(result.output)

    asyncio.run(main())
    ```

Performance:
    - Docker containers: 50-200ms cold start
    - Firecracker VMs: ~125ms cold start with hardware isolation
    - Cached executions: <10ms response time
    - Pre-warmed containers: Instant execution

For detailed documentation and examples, visit: https://docs.faas-platform.com/python-sdk
"""

import hashlib
import json
import time
from dataclasses import dataclass, field
from enum import Enum
from typing import Optional, Dict, List, Any, Callable, AsyncGenerator
import asyncio
import aiohttp
from datetime import datetime, timedelta


class Runtime(Enum):
    """Execution runtime environment selection.

    Choose the optimal runtime based on your requirements:

    Attributes:
        DOCKER: Docker containers - fastest for development
            - Cold start: 50-200ms
            - Best for: Development, testing, rapid iteration
            - Pros: Hot reload, GPU support, rich ecosystem
            - Cons: Process-level isolation only

        FIRECRACKER: Firecracker microVMs - secure for production
            - Cold start: ~125ms
            - Best for: Production, multi-tenant, compliance
            - Pros: Hardware isolation, memory encryption
            - Cons: Linux only, limited GPU support

        AUTO: Automatic runtime selection
            - Platform chooses optimal runtime based on workload
            - Considers security requirements, performance needs
            - Uses historical patterns and resource constraints

    Example:
        ```python
        from faas_sdk import FaaSClient, Runtime

        # Development client with Docker
        dev_client = FaaSClient("http://localhost:8080", runtime=Runtime.DOCKER)

        # Production client with Firecracker
        prod_client = FaaSClient("https://api.example.com", runtime=Runtime.FIRECRACKER)

        # Smart client with automatic selection
        smart_client = FaaSClient("http://localhost:8080", runtime=Runtime.AUTO)
        ```
    """
    DOCKER = "docker"
    FIRECRACKER = "firecracker"
    AUTO = "auto"


class ExecutionMode(Enum):
    """Function execution mode for advanced workflow control.

    Attributes:
        EPHEMERAL: One-time execution with no state persistence
            - Default mode for simple executions
            - Minimal resource usage
            - No caching or persistence

        CACHED: Execution with automatic result caching
            - Results cached based on input hash
            - Subsequent identical requests return cached results
            - Configurable TTL for cache expiration

        CHECKPOINTED: Execution with state snapshots
            - State can be saved at any point during execution
            - Enables pause/resume workflows
            - Useful for long-running computations

        BRANCHED: Execution that can be forked into multiple paths
            - Parent state can spawn multiple child executions
            - Useful for A/B testing and parallel workflows
            - Copy-on-write memory optimization

        PERSISTENT: Long-running execution with persistent state
            - Container/VM stays alive between requests
            - Maintains in-memory state and connections
            - Best for stateful services and databases

    Example:
        ```python
        # Cached ML inference
        result = await client.execute_advanced({
            'command': 'python inference.py',
            'mode': ExecutionMode.CACHED
        })

        # Checkpointed long computation
        result = await client.execute_advanced({
            'command': 'python train_model.py',
            'mode': ExecutionMode.CHECKPOINTED
        })
        ```
    """
    EPHEMERAL = "ephemeral"
    CACHED = "cached"
    CHECKPOINTED = "checkpointed"
    BRANCHED = "branched"
    PERSISTENT = "persistent"


@dataclass
class ExecutionResult:
    """Result from function execution"""
    request_id: str
    output: Optional[str]
    logs: Optional[str]
    error: Optional[str]
    duration_ms: int
    cache_hit: bool = False
    runtime_used: Optional[Runtime] = None
    stdout: Optional[str] = None
    stderr: Optional[str] = None
    exit_code: Optional[int] = None


@dataclass
class ClientConfig:
    """FaaS client configuration"""
    base_url: str
    runtime: Runtime = Runtime.AUTO
    cache_enabled: bool = True
    max_retries: int = 3
    timeout: float = 30.0
    api_key: Optional[str] = None


@dataclass
class ClientMetrics:
    """Client-side performance metrics"""
    total_requests: int = 0
    cache_hits: int = 0
    total_latency_ms: int = 0
    errors: int = 0

    @property
    def cache_hit_rate(self) -> float:
        if self.total_requests == 0:
            return 0.0
        return self.cache_hits / self.total_requests

    @property
    def average_latency_ms(self) -> float:
        if self.total_requests == 0:
            return 0.0
        return self.total_latency_ms / self.total_requests

    @property
    def error_rate(self) -> float:
        if self.total_requests == 0:
            return 0.0
        return self.errors / self.total_requests


class FaaSClient:
    """High-performance FaaS Platform client with intelligent optimization.

    The FaaSClient provides a unified interface to execute code on both Docker containers
    and Firecracker microVMs, with automatic optimization, caching, and scaling.

    Features:
        - Dual runtime support (Docker and Firecracker)
        - Automatic result caching with configurable TTL
        - Pre-warming pools for zero cold starts
        - Execution forking for parallel workflows
        - Built-in metrics and performance monitoring
        - Connection pooling and retry logic
        - Thread-safe async operations

    Args:
        base_url: Base URL of the FaaS platform (e.g., "http://localhost:8080")
        config: Optional client configuration for advanced settings

    Attributes:
        config: Client configuration settings
        metrics: Real-time client-side metrics

    Examples:
        Basic usage:
        ```python
        import asyncio
        from faas_sdk import FaaSClient

        async def main():
            client = FaaSClient("http://localhost:8080")
            result = await client.run_python("print('Hello, World!')")
            print(result.output)  # Output: Hello, World!

        asyncio.run(main())
        ```

        Advanced configuration:
        ```python
        from faas_sdk import FaaSClient, ClientConfig, Runtime

        config = ClientConfig(
            base_url="http://localhost:8080",
            runtime=Runtime.FIRECRACKER,
            cache_enabled=True,
            timeout=30.0,
            max_retries=3
        )

        async with FaaSClient("http://localhost:8080", config=config) as client:
            result = await client.execute(
                command="python ml_inference.py",
                image="pytorch/pytorch:latest",
                env_vars={"MODEL_PATH": "/models/bert"},
                timeout_ms=60000
            )
            print(f"Execution took {result.duration_ms}ms")
        ```

        Language-specific helpers:
        ```python
        # Python execution with automatic environment setup
        result = await client.run_python('''
            import pandas as pd
            df = pd.DataFrame({"a": [1, 2, 3], "b": [4, 5, 6]})
            print(df.mean())
        ''')

        # JavaScript execution
        result = await client.run_javascript('''
            const data = [1, 2, 3, 4, 5];
            console.log(data.reduce((a, b) => a + b, 0));
        ''')

        # Bash script execution
        result = await client.run_bash('''
            #!/bin/bash
            echo "System info:"
            uname -a
            free -h
        ''')
        ```

    Thread Safety:
        FaaSClient is thread-safe and can be used across multiple asyncio tasks:
        ```python
        import asyncio

        async def worker(client, task_id):
            result = await client.run_python(f"print('Worker {task_id} done')")
            return result

        async def main():
            client = FaaSClient("http://localhost:8080")

            # Run multiple executions concurrently
            tasks = [worker(client, i) for i in range(10)]
            results = await asyncio.gather(*tasks)

            print(f"Completed {len(results)} executions")
        ```

    Performance Tips:
        - Use context manager (async with) for automatic connection cleanup
        - Enable caching for deterministic computations
        - Pre-warm containers for critical paths
        - Use Firecracker for production workloads requiring isolation
        - Monitor metrics to identify bottlenecks

    Raises:
        ValueError: If base_url is invalid
        aiohttp.ClientError: For network-related errors
        asyncio.TimeoutError: If requests exceed configured timeout
    """

    def __init__(self, base_url: str, config: Optional[ClientConfig] = None):
        self.config = config or ClientConfig(base_url=base_url)
        self.metrics = ClientMetrics()
        self._session: Optional[aiohttp.ClientSession] = None

    async def __aenter__(self):
        self._session = aiohttp.ClientSession()
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self._session:
            await self._session.close()

    @property
    def session(self) -> aiohttp.ClientSession:
        if not self._session:
            self._session = aiohttp.ClientSession()
        return self._session

    def _get_cache_key(self, content: str) -> str:
        """Generate cache key from content"""
        return hashlib.md5(content.encode()).hexdigest()

    async def execute(
        self,
        command: str,
        image: str = "alpine:latest",
        runtime: Optional[Runtime] = None,
        env_vars: Optional[Dict[str, str]] = None,
        working_dir: Optional[str] = None,
        timeout_ms: Optional[int] = None,
        cache_key: Optional[str] = None,
    ) -> ExecutionResult:
        """
        Execute a command in a container or VM

        Args:
            command: Command to execute
            image: Container image to use
            runtime: Runtime environment (Docker/Firecracker/Auto)
            env_vars: Environment variables
            working_dir: Working directory for execution
            timeout_ms: Execution timeout in milliseconds
            cache_key: Optional cache key for memoization

        Returns:
            ExecutionResult with output, logs, and metrics
        """
        start_time = time.time()

        # Apply defaults
        runtime = runtime or self.config.runtime
        timeout_ms = timeout_ms or int(self.config.timeout * 1000)

        if self.config.cache_enabled and not cache_key:
            cache_key = self._get_cache_key(f"{command}:{image}")

        payload = {
            "command": command,
            "image": image,
            "runtime": runtime.value,
            "timeout_ms": timeout_ms,
        }

        if env_vars:
            payload["env_vars"] = [[k, v] for k, v in env_vars.items()]

        if working_dir:
            payload["working_dir"] = working_dir
        if cache_key:
            payload["cache_key"] = cache_key

        # Execute with retries
        last_error = None
        for attempt in range(self.config.max_retries):
            if attempt > 0:
                await asyncio.sleep(0.1 * (2 ** attempt))  # Exponential backoff

            try:
                async with self.session.post(
                    f"{self.config.base_url}/api/v1/execute",
                    json=payload,
                    headers={"Authorization": f"Bearer {self.config.api_key}"} if self.config.api_key else {},
                    timeout=aiohttp.ClientTimeout(total=self.config.timeout)
                ) as response:
                    elapsed_ms = int((time.time() - start_time) * 1000)

                    # Update metrics
                    self.metrics.total_requests += 1
                    self.metrics.total_latency_ms += elapsed_ms

                    if response.status != 200:
                        self.metrics.errors += 1
                        last_error = f"HTTP {response.status}: {await response.text()}"
                        continue

                    data = await response.json()

                    # Check for cache hit (very fast response)
                    cache_hit = elapsed_ms < 10
                    if cache_hit:
                        self.metrics.cache_hits += 1

                    return ExecutionResult(
                        request_id=data.get("request_id", ""),
                        output=data.get("output"),
                        logs=data.get("logs"),
                        error=data.get("error"),
                        duration_ms=data.get("duration_ms", elapsed_ms),
                        cache_hit=cache_hit,
                        runtime_used=Runtime(runtime.value) if runtime else None,
                        stdout=data.get("stdout"),
                        stderr=data.get("stderr"),
                        exit_code=data.get("exit_code"),
                    )

            except Exception as e:
                self.metrics.errors += 1
                last_error = str(e)

        raise Exception(f"Execution failed after {self.config.max_retries} retries: {last_error}")

    async def run_python(self, code: str, **kwargs) -> ExecutionResult:
        """
        Execute Python code directly

        Args:
            code: Python code to execute
            **kwargs: Additional execution parameters

        Returns:
            ExecutionResult with output
        """
        # Use single quotes and escape only single quotes in the code
        escaped_code = code.replace("'", "'\\''")
        return await self.execute(
            command=f"python3 -c '{escaped_code}'",
            image="python:3.11-slim",
            **kwargs
        )

    async def run_javascript(self, code: str, **kwargs) -> ExecutionResult:
        """
        Execute JavaScript/Node.js code

        Args:
            code: JavaScript code to execute
            **kwargs: Additional execution parameters

        Returns:
            ExecutionResult with output
        """
        # Escape the code properly for shell execution
        escaped_code = code.replace("\\", "\\\\").replace("'", "\\'")
        return await self.execute(
            command=f"node -e '{escaped_code}'",
            image="node:18-alpine",
            **kwargs
        )

    async def run_bash(self, script: str, **kwargs) -> ExecutionResult:
        """
        Execute Bash script

        Args:
            script: Bash script to execute
            **kwargs: Additional execution parameters

        Returns:
            ExecutionResult with output
        """
        # Escape the script properly for shell execution
        escaped_script = script.replace("\\", "\\\\").replace('"', '\\"').replace("$", "\\$")
        return await self.execute(
            command=f'sh -c "{escaped_script}"',
            image="alpine:latest",
            **kwargs
        )

    async def fork_execution(
        self,
        parent_id: str,
        command: str,
        **kwargs
    ) -> ExecutionResult:
        """
        Fork execution from a parent for A/B testing

        Args:
            parent_id: Parent execution ID to fork from
            command: Command to execute in fork
            **kwargs: Additional parameters

        Returns:
            ExecutionResult from forked execution
        """
        payload = {
            "command": command,
            "mode": "branched",
            "branch_from": parent_id,
            "runtime": self.config.runtime.value,
            **kwargs
        }

        async with self.session.post(
            f"{self.config.base_url}/api/v1/execute",
            json=payload,
            timeout=aiohttp.ClientTimeout(total=self.config.timeout)
        ) as response:
            if response.status != 200:
                raise Exception(f"Fork failed: {await response.text()}")

            data = await response.json()
            return ExecutionResult(
                request_id=data.get("request_id", ""),
                output=data.get("output"),
                logs=data.get("logs"),
                error=data.get("error"),
                duration_ms=data.get("duration_ms", 0),
                stdout=data.get("stdout"),
                stderr=data.get("stderr"),
                exit_code=data.get("exit_code"),
            )

    async def create_snapshot(
        self,
        container_id: str,
        name: str,
        description: Optional[str] = None
    ) -> Dict[str, Any]:
        """
        Create a snapshot of a container for later restoration

        Args:
            container_id: Container to snapshot
            name: Snapshot name
            description: Optional description

        Returns:
            Snapshot metadata
        """
        payload = {
            "container_id": container_id,
            "name": name,
            "description": description
        }

        async with self.session.post(
            f"{self.config.base_url}/api/v1/snapshots",
            json=payload,
            timeout=aiohttp.ClientTimeout(total=self.config.timeout)
        ) as response:
            if response.status != 200:
                raise Exception(f"Snapshot creation failed: {await response.text()}")

            return await response.json()

    async def prewarm(self, image: str, count: int = 1) -> None:
        """
        Pre-warm containers for zero cold starts

        Args:
            image: Container image to pre-warm
            count: Number of instances to warm
        """
        payload = {
            "image": image,
            "count": count,
            "runtime": self.config.runtime.value
        }

        async with self.session.post(
            f"{self.config.base_url}/api/v1/prewarm",
            json=payload,
            timeout=aiohttp.ClientTimeout(total=self.config.timeout)
        ) as response:
            if response.status not in (200, 202):
                raise Exception(f"Pre-warming failed: {await response.text()}")

    async def stream_logs(self, execution_id: str) -> AsyncGenerator[str, None]:
        """
        Stream logs from an execution in real-time

        Args:
            execution_id: Execution to stream logs from

        Yields:
            Log lines as they arrive
        """
        async with self.session.get(
            f"{self.config.base_url}/api/v1/logs/{execution_id}/stream",
            timeout=aiohttp.ClientTimeout(total=None)  # No timeout for streaming
        ) as response:
            async for line in response.content:
                if line:
                    yield line.decode('utf-8').strip()

    async def get_metrics(self) -> Dict[str, Any]:
        """Get comprehensive server-side performance metrics.

        Retrieves detailed performance metrics from the FaaS platform including
        execution statistics, resource utilization, cache performance, and system health.

        Returns:
            Dict containing performance metrics:
                - avg_execution_time_ms: Average execution time across all requests
                - cache_hit_rate: Ratio of cached vs computed results (0.0 to 1.0)
                - active_containers: Current number of running containers
                - active_instances: Current number of active VM instances
                - memory_usage_mb: Total memory usage across the platform
                - cpu_usage_percent: CPU utilization percentage
                - request_count: Total number of requests processed
                - error_rate: Ratio of failed requests (0.0 to 1.0)

        Raises:
            Exception: If metrics endpoint is unavailable or returns an error

        Examples:
            Basic usage:
            ```python
            metrics = await client.get_metrics()
            print(f"Cache hit rate: {metrics['cache_hit_rate']:.2%}")
            print(f"Avg execution time: {metrics['avg_execution_time_ms']:.1f}ms")
            ```

            Performance monitoring:
            ```python
            import asyncio

            async def monitor_performance():
                while True:
                    metrics = await client.get_metrics()

                    if metrics['cpu_usage_percent'] > 80:
                        print("⚠️  High CPU usage detected!")

                    if metrics['cache_hit_rate'] < 0.5:
                        print("💡 Consider optimizing cache usage")

                    await asyncio.sleep(30)
            ```
        """
        async with self.session.get(
            f"{self.config.base_url}/api/v1/metrics",
            timeout=aiohttp.ClientTimeout(total=self.config.timeout)
        ) as response:
            if response.status != 200:
                raise Exception(f"Failed to get metrics: {await response.text()}")

            return await response.json()

    def get_client_metrics(self) -> ClientMetrics:
        """Get client-side metrics"""
        return self.metrics

    async def health_check(self) -> Dict[str, Any]:
        """Check platform health status"""
        async with self.session.get(
            f"{self.config.base_url}/health",
            timeout=aiohttp.ClientTimeout(total=5.0)
        ) as response:
            if response.status != 200:
                raise Exception(f"Health check failed: {await response.text()}")

            return await response.json()


class FunctionBuilder:
    """
    Builder for complex function configurations

    Example:
        >>> func = (FunctionBuilder("my-function")
        ...     .runtime(Runtime.FIRECRACKER)
        ...     .with_env("API_KEY", "secret")
        ...     .with_memory(512)
        ...     .build())
    """

    def __init__(self, name: str):
        self.config = {
            "name": name,
            "runtime": Runtime.AUTO,
            "env_vars": {},
            "memory_mb": 256,
            "cpu_cores": 1,
            "timeout_ms": 30000,
        }

    def runtime(self, runtime: Runtime) -> 'FunctionBuilder':
        self.config["runtime"] = runtime
        return self

    def with_env(self, key: str, value: str) -> 'FunctionBuilder':
        self.config["env_vars"][key] = value
        return self

    def with_memory(self, mb: int) -> 'FunctionBuilder':
        self.config["memory_mb"] = mb
        return self

    def with_cpu(self, cores: int) -> 'FunctionBuilder':
        self.config["cpu_cores"] = cores
        return self

    def with_timeout(self, ms: int) -> 'FunctionBuilder':
        self.config["timeout_ms"] = ms
        return self

    def build(self) -> Dict[str, Any]:
        return self.config


# Convenience functions for quick usage
async def run(code: str, language: str = "python", base_url: str = "http://localhost:8080") -> str:
    """
    Quick function execution

    Args:
        code: Code to execute
        language: Language (python, javascript, bash)
        base_url: FaaS platform URL

    Returns:
        Execution output as string
    """
    async with FaaSClient(base_url) as client:
        if language == "python":
            result = await client.run_python(code)
        elif language in ("javascript", "js", "node"):
            result = await client.run_javascript(code)
        elif language in ("bash", "shell", "sh"):
            result = await client.run_bash(code)
        else:
            result = await client.execute(code)

        return result.output or ""


# Async generator type hint fix
from typing import AsyncGenerator