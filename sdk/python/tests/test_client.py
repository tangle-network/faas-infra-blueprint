"""
Tests for FaaS Platform Python SDK
"""

import pytest
import asyncio
from unittest.mock import Mock, AsyncMock, patch
from faas_sdk.client import (
    FaaSPlatformClient,
    ExecutionMode,
    ExecutionRequest,
    ExecutionResponse,
    Snapshot,
    Branch,
    PlatformError,
    TangleJobClient,
    FluentExecutor
)


@pytest.fixture
async def client():
    """Create test client"""
    async with FaaSPlatformClient(api_key='test-key') as client:
        yield client


@pytest.fixture
def mock_session():
    """Mock aiohttp session"""
    with patch('aiohttp.ClientSession') as mock:
        session = AsyncMock()
        mock.return_value = session
        yield session


class TestFaaSPlatformClient:
    """Test FaaS platform client"""

    @pytest.mark.asyncio
    async def test_client_initialization(self):
        """Test client initialization"""
        client = FaaSPlatformClient(api_key='test-key')
        assert client.api_key == 'test-key'
        assert client.timeout == 30

    @pytest.mark.asyncio
    async def test_client_requires_api_key(self):
        """Test API key requirement"""
        with pytest.raises(PlatformError, match='API key required'):
            FaaSPlatformClient(api_key='')

    @pytest.mark.asyncio
    async def test_execute_ephemeral(self, client, mock_session):
        """Test ephemeral execution"""
        mock_response = AsyncMock()
        mock_response.status = 200
        mock_response.ok = True
        mock_response.json = AsyncMock(return_value={
            'request_id': 'test-123',
            'exit_code': 0,
            'stdout': [72, 101, 108, 108, 111],  # "Hello"
            'stderr': [],
            'duration': 0.5,
            'cached': False
        })

        mock_session.request = AsyncMock(return_value=mock_response)
        mock_session.__aenter__ = AsyncMock(return_value=mock_response)
        mock_session.__aexit__ = AsyncMock()

        result = await client.execute_ephemeral('echo "Hello"')

        assert result.request_id == 'test-123'
        assert result.exit_code == 0
        assert result.stdout == b'Hello'
        assert result.duration == 0.5
        assert not result.cached

    @pytest.mark.asyncio
    async def test_execute_cached(self, client, mock_session):
        """Test cached execution"""
        mock_response = AsyncMock()
        mock_response.status = 200
        mock_response.ok = True
        mock_response.json = AsyncMock(return_value={
            'request_id': 'cached-123',
            'exit_code': 0,
            'stdout': [],
            'stderr': [],
            'duration': 0.1,
            'cached': True,
            'snapshot': 'snap-123'
        })

        mock_session.request = AsyncMock(return_value=mock_response)
        mock_session.__aenter__ = AsyncMock(return_value=mock_response)
        mock_session.__aexit__ = AsyncMock()

        result = await client.execute_cached('ls')

        assert result.cached
        assert result.snapshot == 'snap-123'
        assert result.duration == 0.1

    @pytest.mark.asyncio
    async def test_execute_checkpointed(self, client, mock_session):
        """Test checkpointed execution"""
        mock_response = AsyncMock()
        mock_response.status = 200
        mock_response.ok = True
        mock_response.json = AsyncMock(return_value={
            'request_id': 'checkpoint-123',
            'exit_code': 0,
            'stdout': [],
            'stderr': [],
            'duration': 0.2,
            'snapshot': 'snap-456',
            'cached': False
        })

        mock_session.request = AsyncMock(return_value=mock_response)
        mock_session.__aenter__ = AsyncMock(return_value=mock_response)
        mock_session.__aexit__ = AsyncMock()

        result = await client.execute_checkpointed(
            'apt-get update',
            checkpoint='snap-123'
        )

        assert result.snapshot == 'snap-456'

    @pytest.mark.asyncio
    async def test_execute_branched(self, client, mock_session):
        """Test branched execution"""
        mock_response = AsyncMock()
        mock_response.status = 200
        mock_response.ok = True
        mock_response.json = AsyncMock(return_value={
            'request_id': 'branch-123',
            'exit_code': 0,
            'stdout': [],
            'stderr': [],
            'duration': 0.3,
            'snapshot': 'snap-789'
        })

        mock_session.request = AsyncMock(return_value=mock_response)
        mock_session.__aenter__ = AsyncMock(return_value=mock_response)
        mock_session.__aexit__ = AsyncMock()

        result = await client.execute_branched(
            'pip install numpy',
            branch_from='snap-parent'
        )

        assert result.request_id == 'branch-123'

    @pytest.mark.asyncio
    async def test_create_snapshot(self, client, mock_session):
        """Test snapshot creation"""
        mock_response = AsyncMock()
        mock_response.status = 200
        mock_response.ok = True
        mock_response.json = AsyncMock(return_value={
            'id': 'snap-new',
            'parent_id': None,
            'mode': 'checkpointed',
            'created_at': 1234567890.0,
            'size': 1024,
            'checksum': 'abc123'
        })

        mock_session.request = AsyncMock(return_value=mock_response)
        mock_session.__aenter__ = AsyncMock(return_value=mock_response)
        mock_session.__aexit__ = AsyncMock()

        snapshot = await client.create_snapshot('exec-123')

        assert snapshot.id == 'snap-new'
        assert snapshot.size == 1024
        assert snapshot.checksum == 'abc123'

    @pytest.mark.asyncio
    async def test_list_snapshots(self, client, mock_session):
        """Test listing snapshots"""
        mock_response = AsyncMock()
        mock_response.status = 200
        mock_response.ok = True
        mock_response.json = AsyncMock(return_value=[
            {
                'id': 'snap-1',
                'parent_id': None,
                'mode': 'cached',
                'created_at': 1234567890.0,
                'size': 512,
                'checksum': 'aaa'
            },
            {
                'id': 'snap-2',
                'parent_id': 'snap-1',
                'mode': 'checkpointed',
                'created_at': 1234567891.0,
                'size': 768,
                'checksum': 'bbb'
            }
        ])

        mock_session.request = AsyncMock(return_value=mock_response)
        mock_session.__aenter__ = AsyncMock(return_value=mock_response)
        mock_session.__aexit__ = AsyncMock()

        snapshots = await client.list_snapshots(mode=ExecutionMode.CHECKPOINTED)

        assert len(snapshots) == 2
        assert snapshots[0].id == 'snap-1'
        assert snapshots[1].id == 'snap-2'

    @pytest.mark.asyncio
    async def test_create_branch(self, client, mock_session):
        """Test branch creation"""
        mock_response = AsyncMock()
        mock_response.status = 200
        mock_response.ok = True
        mock_response.json = AsyncMock(return_value={
            'id': 'branch-123',
            'snapshot_id': 'snap-123',
            'parent_branch': None,
            'created_at': 1234567890.0,
            'divergence_point': 'snap-base'
        })

        mock_session.request = AsyncMock(return_value=mock_response)
        mock_session.__aenter__ = AsyncMock(return_value=mock_response)
        mock_session.__aexit__ = AsyncMock()

        branch = await client.create_branch('snap-123')

        assert branch.id == 'branch-123'
        assert branch.snapshot_id == 'snap-123'
        assert branch.divergence_point == 'snap-base'

    @pytest.mark.asyncio
    async def test_merge_branches(self, client, mock_session):
        """Test branch merging"""
        mock_response = AsyncMock()
        mock_response.status = 200
        mock_response.ok = True
        mock_response.json = AsyncMock(return_value={
            'id': 'snap-merged',
            'parent_id': None,
            'mode': 'checkpointed',
            'created_at': 1234567892.0,
            'size': 2048,
            'checksum': 'merged'
        })

        mock_session.request = AsyncMock(return_value=mock_response)
        mock_session.__aenter__ = AsyncMock(return_value=mock_response)
        mock_session.__aexit__ = AsyncMock()

        snapshot = await client.merge_branches(['branch-1', 'branch-2'])

        assert snapshot.id == 'snap-merged'
        assert snapshot.size == 2048

    @pytest.mark.asyncio
    async def test_parallel_map(self, client, mock_session):
        """Test parallel map execution"""
        # Mock branch creation
        branch_response = AsyncMock()
        branch_response.status = 200
        branch_response.ok = True
        branch_response.json = AsyncMock(side_effect=[
            {'id': 'branch-1', 'snapshot_id': 'snap-1', 'parent_branch': None,
             'created_at': 1234567890.0, 'divergence_point': 'base'},
            {'id': 'branch-2', 'snapshot_id': 'snap-2', 'parent_branch': None,
             'created_at': 1234567890.0, 'divergence_point': 'base'},
        ])

        mock_session.request = AsyncMock(return_value=branch_response)
        mock_session.__aenter__ = AsyncMock(return_value=branch_response)
        mock_session.__aexit__ = AsyncMock()

        items = ['item1', 'item2']

        async def process_item(item, snapshot):
            return f'{item}-{snapshot.id}'

        results = await client.parallel_map(items, process_item, 'base')

        # Note: This test is simplified due to mocking complexity
        assert len(results) == 2

    @pytest.mark.asyncio
    async def test_race_execution(self, client, mock_session):
        """Test race execution"""
        mock_response = AsyncMock()
        mock_response.status = 200
        mock_response.ok = True
        mock_response.json = AsyncMock(return_value={
            'request_id': 'fast-123',
            'exit_code': 0,
            'stdout': [],
            'stderr': [],
            'duration': 0.1
        })

        mock_session.request = AsyncMock(return_value=mock_response)
        mock_session.__aenter__ = AsyncMock(return_value=mock_response)
        mock_session.__aexit__ = AsyncMock()

        requests = [
            ExecutionRequest(
                id=f'race-{i}',
                code=f'sleep {i}',
                mode=ExecutionMode.EPHEMERAL,
                env='alpine:latest'
            )
            for i in range(3)
        ]

        result = await client.race(requests)

        assert result.request_id == 'fast-123'

    @pytest.mark.asyncio
    async def test_error_handling(self, client, mock_session):
        """Test error handling"""
        mock_response = AsyncMock()
        mock_response.status = 500
        mock_response.ok = False
        mock_response.text = AsyncMock(return_value='Internal server error')

        mock_session.request = AsyncMock(return_value=mock_response)
        mock_session.__aenter__ = AsyncMock(return_value=mock_response)
        mock_session.__aexit__ = AsyncMock()

        with pytest.raises(PlatformError, match='Server error'):
            await client.execute_ephemeral('echo "test"')

    @pytest.mark.asyncio
    async def test_retry_logic(self, client, mock_session):
        """Test retry logic on transient failures"""
        # First two attempts fail, third succeeds
        responses = [
            AsyncMock(status=500, ok=False, text=AsyncMock(return_value='Error')),
            AsyncMock(status=500, ok=False, text=AsyncMock(return_value='Error')),
            AsyncMock(
                status=200,
                ok=True,
                json=AsyncMock(return_value={
                    'request_id': 'retry-success',
                    'exit_code': 0,
                    'stdout': [],
                    'stderr': [],
                    'duration': 0.1
                })
            )
        ]

        mock_session.request = AsyncMock(side_effect=responses)
        for resp in responses:
            resp.__aenter__ = AsyncMock(return_value=resp)
            resp.__aexit__ = AsyncMock()

        # Should succeed after retries
        result = await client.execute_ephemeral('echo "retry"')
        assert result.request_id == 'retry-success'


class TestFluentExecutor:
    """Test fluent executor API"""

    @pytest.mark.asyncio
    async def test_fluent_chain(self, client, mock_session):
        """Test fluent execution chain"""
        mock_responses = [
            # from_env response
            AsyncMock(
                status=200,
                ok=True,
                json=AsyncMock(return_value={
                    'request_id': 'env-123',
                    'exit_code': 0,
                    'stdout': [],
                    'stderr': [],
                    'duration': 0.1,
                    'snapshot': 'snap-env'
                })
            ),
            # exec response
            AsyncMock(
                status=200,
                ok=True,
                json=AsyncMock(return_value={
                    'request_id': 'exec-123',
                    'exit_code': 0,
                    'stdout': [],
                    'stderr': [],
                    'duration': 0.2,
                    'snapshot': 'snap-exec'
                })
            )
        ]

        mock_session.request = AsyncMock(side_effect=mock_responses)
        for resp in mock_responses:
            resp.__aenter__ = AsyncMock(return_value=resp)
            resp.__aexit__ = AsyncMock()

        fluent = FluentExecutor(client)
        result = await fluent.from_env('python:3.9').exec('pip install numpy').run()

        # Verify chain executed
        assert fluent.current_snapshot == 'snap-exec'

    @pytest.mark.asyncio
    async def test_fluent_branch(self, client, mock_session):
        """Test fluent branching"""
        mock_responses = [
            # Initial exec
            AsyncMock(
                status=200,
                ok=True,
                json=AsyncMock(return_value={
                    'request_id': 'exec-123',
                    'exit_code': 0,
                    'stdout': [],
                    'stderr': [],
                    'duration': 0.1,
                    'snapshot': 'snap-base'
                })
            ),
            # Branch creation
            AsyncMock(
                status=200,
                ok=True,
                json=AsyncMock(return_value={
                    'id': 'branch-123',
                    'snapshot_id': 'snap-branch',
                    'parent_branch': None,
                    'created_at': 1234567890.0,
                    'divergence_point': 'snap-base'
                })
            )
        ]

        mock_session.request = AsyncMock(side_effect=mock_responses)
        for resp in mock_responses:
            resp.__aenter__ = AsyncMock(return_value=resp)
            resp.__aexit__ = AsyncMock()

        fluent = FluentExecutor(client)
        fluent.current_snapshot = 'snap-base'  # Set initial snapshot
        await fluent.branch().run()

        assert fluent.current_snapshot == 'snap-branch'


class TestTangleJobClient:
    """Test Tangle job client"""

    @pytest.mark.asyncio
    async def test_job_submission(self):
        """Test job submission to Tangle"""
        with patch('substrate.SubstrateInterface') as mock_substrate:
            mock_instance = Mock()
            mock_substrate.return_value = mock_instance

            # Mock successful submission
            mock_receipt = Mock()
            mock_receipt.is_success = True
            mock_receipt.block_hash = '0xabc123'
            mock_receipt.extrinsic_hash = '0xdef456'
            mock_receipt.triggered_events = [
                Mock(value={
                    'event': {
                        'module_id': 'Jobs',
                        'event_id': 'JobSubmitted',
                        'attributes': [1, 123]  # blueprint_id, job_id
                    }
                })
            ]

            mock_instance.submit_extrinsic = Mock(return_value=mock_receipt)
            mock_instance.create_signed_extrinsic = Mock()
            mock_instance.compose_call = Mock()

            client = TangleJobClient()
            client.keypair = Mock()  # Mock keypair

            result = client.submit_job('echo "Hello Tangle"')

            assert result['job_id'] == 123
            assert result['block_hash'] == '0xabc123'

    @pytest.mark.asyncio
    async def test_wait_for_job(self):
        """Test waiting for job completion"""
        with patch('substrate.SubstrateInterface') as mock_substrate:
            mock_instance = Mock()
            mock_substrate.return_value = mock_instance

            # Mock job query result
            mock_result = Mock()
            mock_result.value = {
                'status': 'Completed',
                'result': {
                    'exit_code': 0,
                    'stdout': '48656c6c6f',  # "Hello" in hex
                    'stderr': '',
                    'execution_time_ms': 100
                }
            }

            mock_instance.query = Mock(return_value=mock_result)

            client = TangleJobClient()
            result = await client.wait_for_job(123)

            assert result['job_id'] == 123
            assert result['exit_code'] == 0
            assert result['stdout'] == b'Hello'
            assert result['execution_time_ms'] == 100


if __name__ == '__main__':
    pytest.main([__file__, '-v'])