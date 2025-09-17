"""
FaaS Platform Python SDK
Integration with Rust-based multi-mode execution platform
"""

import os
import json
import time
import asyncio
import hashlib
from typing import Optional, Dict, Any, List, Union, Callable, TypeVar, Generic
from dataclasses import dataclass, asdict
from enum import Enum
from datetime import datetime
import aiohttp
import websockets
from substrate import SubstrateInterface, Keypair
from substrate.exceptions import SubstrateRequestException

T = TypeVar('T')
R = TypeVar('R')

# Configuration
DEFAULT_PLATFORM_URL = os.getenv('FAAS_PLATFORM_URL', 'http://localhost:8080/api/v1')
DEFAULT_EXECUTOR_URL = os.getenv('FAAS_EXECUTOR_URL', 'http://localhost:8081')
DEFAULT_TANGLE_URL = os.getenv('TANGLE_RPC_URL', 'ws://localhost:9944')


class ExecutionMode(str, Enum):
    """Execution modes matching Rust platform implementation"""
    EPHEMERAL = 'ephemeral'
    CACHED = 'cached'
    CHECKPOINTED = 'checkpointed'
    BRANCHED = 'branched'
    PERSISTENT = 'persistent'


@dataclass
class ExecutionRequest:
    """Request for code execution"""
    id: str
    code: str
    mode: ExecutionMode
    env: str
    timeout: Optional[int] = None
    checkpoint: Optional[str] = None
    branch_from: Optional[str] = None


@dataclass
class ExecutionResponse:
    """Response from code execution"""
    request_id: str
    exit_code: int
    stdout: bytes
    stderr: bytes
    duration: float
    snapshot: Optional[str] = None
    cached: bool = False
    memory_used: Optional[int] = None


@dataclass
class Snapshot:
    """Execution snapshot with CRIU checkpoint"""
    id: str
    parent_id: Optional[str]
    mode: ExecutionMode
    created_at: float
    size: int
    memory_pages: Optional[int]
    checksum: str
    metadata: Optional[Dict[str, Any]] = None


@dataclass
class Branch:
    """Execution branch for parallel operations"""
    id: str
    snapshot_id: str
    parent_branch: Optional[str]
    created_at: float
    divergence_point: str
    metadata: Optional[Dict[str, Any]] = None


@dataclass
class ResourceLimits:
    """Resource constraints for execution"""
    cpu_cores: Optional[int] = None
    memory_mb: Optional[int] = None
    disk_mb: Optional[int] = None
    network_mbps: Optional[int] = None


class PlatformError(Exception):
    """Platform-specific errors"""
    def __init__(self, message: str, code: Optional[str] = None, details: Any = None):
        super().__init__(message)
        self.code = code
        self.details = details


class FaaSPlatformClient:
    """
    Main client for FaaS platform operations
    Integrates with Rust-based execution system
    """

    def __init__(
        self,
        api_key: Optional[str] = None,
        platform_url: str = DEFAULT_PLATFORM_URL,
        executor_url: str = DEFAULT_EXECUTOR_URL,
        timeout: int = 30,
        max_retries: int = 3
    ):
        self.api_key = api_key or os.getenv('FAAS_API_KEY', '')
        if not self.api_key:
            raise PlatformError('API key required. Set FAAS_API_KEY or pass api_key')

        self.platform_url = platform_url
        self.executor_url = executor_url
        self.timeout = timeout
        self.max_retries = max_retries
        self._session: Optional[aiohttp.ClientSession] = None

    async def __aenter__(self):
        """Async context manager entry"""
        self._session = aiohttp.ClientSession(
            timeout=aiohttp.ClientTimeout(total=self.timeout)
        )
        return self

    async def __aexit__(self, *args):
        """Async context manager exit"""
        if self._session:
            await self._session.close()

    async def _request(
        self,
        method: str,
        url: str,
        json_data: Optional[Dict] = None,
        **kwargs
    ) -> Dict[str, Any]:
        """Make HTTP request with retry logic"""
        if not self._session:
            self._session = aiohttp.ClientSession(
                timeout=aiohttp.ClientTimeout(total=self.timeout)
            )

        headers = {
            'Authorization': f'Bearer {self.api_key}',
            'Content-Type': 'application/json',
            **kwargs.get('headers', {})
        }

        for attempt in range(self.max_retries):
            try:
                async with self._session.request(
                    method, url, json=json_data, headers=headers, **kwargs
                ) as response:
                    if response.status >= 500:
                        if attempt < self.max_retries - 1:
                            await asyncio.sleep(2 ** attempt)
                            continue
                        raise PlatformError(f'Server error: {response.status}')

                    if not response.ok:
                        error_data = await response.text()
                        raise PlatformError(
                            f'Request failed: {response.status}',
                            code=str(response.status),
                            details=error_data
                        )

                    return await response.json()

            except asyncio.TimeoutError:
                raise PlatformError(f'Request timeout after {self.timeout}s', code='TIMEOUT')
            except aiohttp.ClientError as e:
                if attempt < self.max_retries - 1:
                    await asyncio.sleep(2 ** attempt)
                    continue
                raise PlatformError(f'Network error: {e}')

        raise PlatformError(f'Max retries ({self.max_retries}) exceeded')

    # Core execution methods

    async def execute(self, request: ExecutionRequest) -> ExecutionResponse:
        """Execute code with specified mode"""
        url = f'{self.executor_url}/execute'
        data = asdict(request)

        result = await self._request('POST', url, data)

        return ExecutionResponse(
            request_id=result['request_id'],
            exit_code=result['exit_code'],
            stdout=bytes(result['stdout']),
            stderr=bytes(result['stderr']),
            duration=result['duration'],
            snapshot=result.get('snapshot'),
            cached=result.get('cached', False),
            memory_used=result.get('memory_used')
        )

    async def execute_ephemeral(self, code: str, env: str = 'alpine:latest') -> ExecutionResponse:
        """Execute code in ephemeral mode"""
        request = ExecutionRequest(
            id=self._generate_id('ephemeral'),
            code=code,
            mode=ExecutionMode.EPHEMERAL,
            env=env
        )
        return await self.execute(request)

    async def execute_cached(self, code: str, env: str = 'alpine:latest') -> ExecutionResponse:
        """Execute code with caching"""
        request = ExecutionRequest(
            id=self._generate_id('cached'),
            code=code,
            mode=ExecutionMode.CACHED,
            env=env
        )
        return await self.execute(request)

    async def execute_checkpointed(
        self,
        code: str,
        env: str = 'alpine:latest',
        checkpoint: Optional[str] = None
    ) -> ExecutionResponse:
        """Execute code with checkpoint/restore"""
        request = ExecutionRequest(
            id=self._generate_id('checkpoint'),
            code=code,
            mode=ExecutionMode.CHECKPOINTED,
            env=env,
            checkpoint=checkpoint
        )
        return await self.execute(request)

    async def execute_branched(
        self,
        code: str,
        branch_from: str,
        env: str = 'alpine:latest'
    ) -> ExecutionResponse:
        """Execute code in branched mode"""
        request = ExecutionRequest(
            id=self._generate_id('branch'),
            code=code,
            mode=ExecutionMode.BRANCHED,
            env=env,
            branch_from=branch_from
        )
        return await self.execute(request)

    # Snapshot operations

    async def create_snapshot(self, execution_id: str) -> Snapshot:
        """Create CRIU snapshot from execution"""
        url = f'{self.platform_url}/snapshots'
        result = await self._request('POST', url, {'execution_id': execution_id})

        return Snapshot(**result)

    async def restore_snapshot(self, snapshot_id: str) -> ExecutionResponse:
        """Restore execution from snapshot"""
        url = f'{self.platform_url}/snapshots/{snapshot_id}/restore'
        result = await self._request('POST', url)

        return ExecutionResponse(**result)

    async def get_snapshot(self, snapshot_id: str) -> Snapshot:
        """Get snapshot details"""
        url = f'{self.platform_url}/snapshots/{snapshot_id}'
        result = await self._request('GET', url)

        return Snapshot(**result)

    async def list_snapshots(
        self,
        mode: Optional[ExecutionMode] = None,
        parent_id: Optional[str] = None
    ) -> List[Snapshot]:
        """List available snapshots"""
        url = f'{self.platform_url}/snapshots'
        params = {}
        if mode:
            params['mode'] = mode.value
        if parent_id:
            params['parent_id'] = parent_id

        results = await self._request('GET', url, params=params)
        return [Snapshot(**r) for r in results]

    async def delete_snapshot(self, snapshot_id: str) -> None:
        """Delete snapshot"""
        url = f'{self.platform_url}/snapshots/{snapshot_id}'
        await self._request('DELETE', url)

    # Branch operations

    async def create_branch(
        self,
        snapshot_id: str,
        metadata: Optional[Dict[str, Any]] = None
    ) -> Branch:
        """Create execution branch for parallel operations"""
        url = f'{self.platform_url}/branches'
        data = {'snapshot_id': snapshot_id}
        if metadata:
            data['metadata'] = metadata

        result = await self._request('POST', url, data)
        return Branch(**result)

    async def list_branches(self, snapshot_id: Optional[str] = None) -> List[Branch]:
        """List execution branches"""
        url = f'{self.platform_url}/branches'
        params = {'snapshot_id': snapshot_id} if snapshot_id else {}

        results = await self._request('GET', url, params=params)
        return [Branch(**r) for r in results]

    async def merge_branches(self, branch_ids: List[str]) -> Snapshot:
        """Merge multiple execution branches"""
        url = f'{self.platform_url}/branches/merge'
        result = await self._request('POST', url, {'branch_ids': branch_ids})

        return Snapshot(**result)

    # Streaming execution

    async def stream_execution(self, request: ExecutionRequest):
        """Stream execution output via WebSocket"""
        ws_url = self.executor_url.replace('http', 'ws') + '/stream'

        async with websockets.connect(ws_url) as websocket:
            # Send execution request
            await websocket.send(json.dumps({
                **asdict(request),
                'api_key': self.api_key
            }))

            # Stream output
            while True:
                message = await websocket.recv()
                data = json.loads(message)

                if data['type'] == 'stdout':
                    yield ('stdout', data['content'])
                elif data['type'] == 'stderr':
                    yield ('stderr', data['content'])
                elif data['type'] == 'exit':
                    yield ('exit', data['code'])
                    break

    # Parallel execution helpers

    async def parallel_map(
        self,
        items: List[T],
        fn: Callable[[T, Snapshot], R],
        base_snapshot: Optional[str] = None
    ) -> List[R]:
        """Execute function on items in parallel using branches"""
        # Create branches
        if base_snapshot:
            branches = await asyncio.gather(*[
                self.create_branch(base_snapshot) for _ in items
            ])
            snapshots = [Snapshot(id=b.snapshot_id, **{}) for b in branches]
        else:
            snapshots = await asyncio.gather(*[
                self.create_snapshot(self._generate_id('parallel'))
                for _ in items
            ])

        try:
            # Execute in parallel
            results = await asyncio.gather(*[
                fn(item, snapshot) for item, snapshot in zip(items, snapshots)
            ])
            return results
        finally:
            # Cleanup
            await asyncio.gather(*[
                self.delete_snapshot(s.id) for s in snapshots
            ], return_exceptions=True)

    async def race(self, executions: List[ExecutionRequest]) -> ExecutionResponse:
        """Race multiple executions, return first to complete"""
        tasks = [self.execute(req) for req in executions]
        done, pending = await asyncio.wait(tasks, return_when=asyncio.FIRST_COMPLETED)

        # Cancel pending tasks
        for task in pending:
            task.cancel()

        return done.pop().result()

    # Metrics

    async def get_metrics(self) -> Dict[str, Any]:
        """Get platform performance metrics"""
        url = f'{self.platform_url}/metrics'
        return await self._request('GET', url)

    # Helper methods

    def _generate_id(self, prefix: str) -> str:
        """Generate unique ID for requests"""
        timestamp = int(time.time() * 1000)
        random_suffix = hashlib.md5(os.urandom(16)).hexdigest()[:8]
        return f'{prefix}-{timestamp}-{random_suffix}'

    async def close(self):
        """Close client connections"""
        if self._session:
            await self._session.close()


class TangleJobClient:
    """
    Tangle network integration for job submission
    Uses Polkadot/Substrate for blockchain job management
    """

    def __init__(
        self,
        node_url: str = DEFAULT_TANGLE_URL,
        blueprint_id: int = 1,
        keypair: Optional[Keypair] = None
    ):
        self.substrate = SubstrateInterface(url=node_url)
        self.blueprint_id = blueprint_id
        self.keypair = keypair

    def submit_job(
        self,
        code: str,
        mode: ExecutionMode = ExecutionMode.EPHEMERAL,
        env: str = 'alpine:latest',
        checkpoint_id: Optional[str] = None,
        branch_from: Optional[str] = None,
        max_gas: int = 1000000000000
    ) -> Dict[str, Any]:
        """Submit job to Tangle network"""
        if not self.keypair:
            raise PlatformError('Keypair required for job submission')

        # Prepare job arguments
        job_args = {
            'execution_mode': mode.value,
            'code': code.encode('utf-8').hex(),
            'environment': env,
            'resource_requirements': {
                'cpu_cores': 1,
                'memory_mb': 512,
                'timeout_ms': 30000
            },
            'checkpoint_id': checkpoint_id,
            'branch_from': branch_from
        }

        # Create and submit extrinsic
        call = self.substrate.compose_call(
            call_module='Jobs',
            call_function='submit_job',
            call_params={
                'blueprint_id': self.blueprint_id,
                'args': json.dumps(job_args).encode('utf-8').hex(),
                'max_gas': max_gas
            }
        )

        extrinsic = self.substrate.create_signed_extrinsic(
            call=call,
            keypair=self.keypair
        )

        receipt = self.substrate.submit_extrinsic(extrinsic, wait_for_finalization=True)

        if not receipt.is_success:
            raise PlatformError(f'Job submission failed: {receipt.error_message}')

        # Extract job ID from events
        job_id = None
        for event in receipt.triggered_events:
            if event.value['event']['module_id'] == 'Jobs' and \
               event.value['event']['event_id'] == 'JobSubmitted':
                job_id = event.value['event']['attributes'][1]
                break

        return {
            'job_id': job_id,
            'block_hash': receipt.block_hash,
            'extrinsic_hash': receipt.extrinsic_hash
        }

    async def wait_for_job(self, job_id: int, timeout: int = 60) -> Dict[str, Any]:
        """Wait for job completion"""
        start_time = time.time()

        while time.time() - start_time < timeout:
            result = self.substrate.query(
                module='Jobs',
                storage_function='Jobs',
                params=[job_id]
            )

            if result and result.value['status'] == 'Completed':
                return {
                    'job_id': job_id,
                    'exit_code': result.value['result']['exit_code'],
                    'stdout': bytes.fromhex(result.value['result']['stdout']),
                    'stderr': bytes.fromhex(result.value['result']['stderr']),
                    'execution_time_ms': result.value['result']['execution_time_ms'],
                    'snapshot_id': result.value['result'].get('snapshot_id')
                }

            await asyncio.sleep(1)

        raise PlatformError(f'Job {job_id} timeout after {timeout}s')


class FluentExecutor:
    """Fluent API for chaining operations"""

    def __init__(self, client: FaaSPlatformClient):
        self.client = client
        self.operations: List[Callable] = []
        self.current_snapshot: Optional[str] = None

    def from_env(self, env: str) -> 'FluentExecutor':
        """Start from environment"""
        async def op():
            result = await self.client.execute_cached('', env)
            self.current_snapshot = result.snapshot

        self.operations.append(op)
        return self

    def exec(self, code: str) -> 'FluentExecutor':
        """Execute code"""
        async def op():
            result = await self.client.execute_checkpointed(
                code, checkpoint=self.current_snapshot
            )
            self.current_snapshot = result.snapshot

        self.operations.append(op)
        return self

    def branch(self) -> 'FluentExecutor':
        """Create branch"""
        async def op():
            if not self.current_snapshot:
                raise PlatformError('No snapshot to branch from')
            branch = await self.client.create_branch(self.current_snapshot)
            self.current_snapshot = branch.snapshot_id

        self.operations.append(op)
        return self

    async def run(self) -> Optional[ExecutionResponse]:
        """Execute all operations"""
        last_result = None
        for op in self.operations:
            last_result = await op()
        return last_result

    async def snapshot(self) -> str:
        """Get final snapshot"""
        await self.run()
        if not self.current_snapshot:
            raise PlatformError('No snapshot created')
        return self.current_snapshot


# Convenience functions

def create_client(**kwargs) -> FaaSPlatformClient:
    """Create platform client"""
    return FaaSPlatformClient(**kwargs)


def fluent(client: FaaSPlatformClient) -> FluentExecutor:
    """Create fluent executor"""
    return FluentExecutor(client)