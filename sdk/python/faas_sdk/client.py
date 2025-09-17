
import os
import time
import requests
from typing import Optional, Dict, List, Any
from dataclasses import dataclass


@dataclass
class Snapshot:
    id: str
    name: str
    created_at: int
    size_bytes: int
    image: Optional[str] = None


@dataclass
class Instance:
    id: str
    status: str  # 'starting', 'running', 'paused', 'stopped'
    created_at: int
    cpu_cores: int
    memory_mb: int
    disk_gb: int
    ssh_host: Optional[str] = None
    ssh_port: Optional[int] = None
    ssh_username: Optional[str] = None


@dataclass
class ExecResult:
    stdout: str
    stderr: str
    exit_code: int


@dataclass
class Branch:
    id: str
    parent_snapshot_id: str
    name: str
    created_at: int


class FaaSClient:

    def __init__(self, api_key: Optional[str] = None, endpoint: Optional[str] = None):
        self.api_key = api_key or os.getenv('FAAS_API_KEY', 'dev-api-key')
        self.endpoint = endpoint or os.getenv('FAAS_API_URL', 'http://localhost:8080')
        self.headers = {'x-api-key': self.api_key}

        self.snapshots = SnapshotManager(self)
        self.instances = InstanceManager(self)
        self.branches = BranchManager(self)

    def execute(self, command: str, image: str = 'alpine:latest') -> ExecResult:
        response = requests.post(
            f'{self.endpoint}/api/v1/execute',
            headers=self.headers,
            json={
                'image': image,
                'command': command.split(),
                'env_vars': None,
                'payload': [],
            }
        )
        response.raise_for_status()
        data = response.json()

        return ExecResult(
            stdout=bytes(data.get('response', [])).decode() if data.get('response') else '',
            stderr=data.get('logs', ''),
            exit_code=1 if data.get('error') else 0
        )

    def execute_advanced(self,
                        command: str,
                        image: str = 'alpine:latest',
                        mode: str = 'ephemeral',
                        checkpoint_id: Optional[str] = None,
                        branch_from: Optional[str] = None,
                        timeout: Optional[int] = None) -> ExecResult:
        response = requests.post(
            f'{self.endpoint}/api/v1/execute/advanced',
            headers=self.headers,
            json={
                'image': image,
                'command': command.split(),
                'env_vars': None,
                'payload': [],
                'mode': mode,
                'checkpoint_id': checkpoint_id,
                'branch_from': branch_from,
                'timeout_secs': timeout,
            }
        )
        response.raise_for_status()
        data = response.json()

        return ExecResult(
            stdout=bytes(data.get('response', [])).decode() if data.get('response') else '',
            stderr=data.get('logs', ''),
            exit_code=1 if data.get('error') else 0
        )


class SnapshotManager:

    def __init__(self, client: FaaSClient):
        self._client = client

    def create(self,
               name: str,
               image_id: Optional[str] = None,
               container_id: Optional[str] = None,
               vcpus: int = 1,
               memory: int = 1024,
               disk_size: int = 10240) -> Snapshot:
        response = requests.post(
            f'{self._client.endpoint}/api/v1/snapshots',
            headers=self._client.headers,
            json={
                'container_id': container_id or f'new_{int(time.time())}',
                'name': name,
                'description': f'vcpus:{vcpus} memory:{memory}',
            }
        )
        response.raise_for_status()
        data = response.json()

        return Snapshot(
            id=data['snapshot_id'],
            name=name,
            created_at=int(time.time()),
            size_bytes=0,
            image=image_id
        )

    def list(self) -> List[Snapshot]:
        response = requests.get(
            f'{self._client.endpoint}/api/v1/snapshots',
            headers=self._client.headers
        )
        response.raise_for_status()

        return [Snapshot(**snap) for snap in response.json()]

    def restore(self, snapshot_id: str) -> str:
        response = requests.post(
            f'{self._client.endpoint}/api/v1/snapshots/{snapshot_id}/restore',
            headers=self._client.headers
        )
        response.raise_for_status()
        return response.json()['container_id']


class InstanceManager:

    def __init__(self, client: FaaSClient):
        self._client = client

    def start(self,
              snapshot_id: Optional[str] = None,
              image: str = 'alpine:latest',
              cpu_cores: int = 1,
              memory_mb: int = 1024,
              disk_gb: int = 10,
              enable_ssh: bool = False) -> 'InstanceProxy':
        response = requests.post(
            f'{self._client.endpoint}/api/v1/instances',
            headers=self._client.headers,
            json={
                'snapshot_id': snapshot_id,
                'image': image,
                'cpu_cores': cpu_cores,
                'memory_mb': memory_mb,
                'disk_gb': disk_gb,
                'enable_ssh': enable_ssh,
            }
        )
        response.raise_for_status()
        data = response.json()

        instance = Instance(
            id=data['instance_id'],
            status='starting',
            created_at=int(time.time()),
            cpu_cores=cpu_cores,
            memory_mb=memory_mb,
            disk_gb=disk_gb
        )

        return InstanceProxy(instance, self._client)

    def get(self, instance_id: str) -> Instance:
        response = requests.get(
            f'{self._client.endpoint}/api/v1/instances/{instance_id}/info',
            headers=self._client.headers
        )
        response.raise_for_status()
        data = response.json()

        return Instance(
            id=data['id'],
            status=data['status'],
            created_at=data['created_at'],
            cpu_cores=data['resources']['cpu_cores'],
            memory_mb=data['resources']['memory_mb'],
            disk_gb=data['resources']['disk_gb'],
            ssh_host=data.get('ssh_host'),
            ssh_port=data.get('ssh_port'),
            ssh_username=data.get('ssh_username')
        )

    def list(self) -> List[Instance]:
        response = requests.get(
            f'{self._client.endpoint}/api/v1/instances',
            headers=self._client.headers
        )
        response.raise_for_status()

        return [
            Instance(
                id=data['id'],
                status=data['status'],
                created_at=data['created_at'],
                cpu_cores=data['resources']['cpu_cores'],
                memory_mb=data['resources']['memory_mb'],
                disk_gb=data['resources']['disk_gb']
            )
            for data in response.json()
        ]


class BranchManager:

    def __init__(self, client: FaaSClient):
        self._client = client

    def create(self, parent_snapshot_id: str, name: str) -> Branch:
        response = requests.post(
            f'{self._client.endpoint}/api/v1/branches',
            headers=self._client.headers,
            json={
                'parent_snapshot_id': parent_snapshot_id,
                'branch_name': name,
            }
        )
        response.raise_for_status()
        data = response.json()

        return Branch(
            id=data['branch_id'],
            parent_snapshot_id=parent_snapshot_id,
            name=name,
            created_at=int(time.time())
        )

    def merge(self, branch_ids: List[str], strategy: str = 'latest') -> str:
        response = requests.post(
            f'{self._client.endpoint}/api/v1/branches/merge',
            headers=self._client.headers,
            json={
                'branch_ids': branch_ids,
                'merge_strategy': strategy,
            }
        )
        response.raise_for_status()
        return response.json()['merged_id']


class InstanceProxy:

    def __init__(self, instance: Instance, client: FaaSClient):
        self._instance = instance
        self._client = client

    @property
    def id(self) -> str:
        return self._instance.id

    @property
    def status(self) -> str:
        return self._instance.status

    def exec(self, command: str) -> ExecResult:
        response = requests.post(
            f'{self._client.endpoint}/api/v1/execute/advanced',
            headers=self._client.headers,
            json={
                'image': 'use-instance',
                'command': command.split(),
                'env_vars': None,
                'payload': [],
                'mode': 'persistent',
                'checkpoint_id': self._instance.id,
            }
        )
        response.raise_for_status()
        data = response.json()

        return ExecResult(
            stdout=bytes(data.get('response', [])).decode() if data.get('response') else '',
            stderr=data.get('logs', ''),
            exit_code=1 if data.get('error') else 0
        )

    def snapshot(self, name: Optional[str] = None) -> Snapshot:
        snapshot_name = name or f'snapshot-{self._instance.id}-{int(time.time())}'

        response = requests.post(
            f'{self._client.endpoint}/api/v1/snapshots',
            headers=self._client.headers,
            json={
                'container_id': self._instance.id,
                'name': snapshot_name,
                'description': 'Instance snapshot',
            }
        )
        response.raise_for_status()
        data = response.json()

        return Snapshot(
            id=data['snapshot_id'],
            name=snapshot_name,
            created_at=int(time.time()),
            size_bytes=0
        )

    def stop(self) -> None:
        response = requests.post(
            f'{self._client.endpoint}/api/v1/instances/{self._instance.id}/stop',
            headers=self._client.headers
        )
        response.raise_for_status()
        self._instance.status = 'stopped'

    def pause(self) -> str:
        response = requests.post(
            f'{self._client.endpoint}/api/v1/instances/{self._instance.id}/pause',
            headers=self._client.headers
        )
        response.raise_for_status()
        self._instance.status = 'paused'
        return response.json()['checkpoint_id']

    def expose_port(self, port: int, protocol: str = 'http', subdomain: Optional[str] = None) -> str:
        response = requests.post(
            f'{self._client.endpoint}/api/v1/ports/expose',
            headers=self._client.headers,
            json={
                'instance_id': self._instance.id,
                'internal_port': port,
                'protocol': protocol,
                'subdomain': subdomain,
            }
        )
        response.raise_for_status()
        return response.json()['public_url']

    def upload_files(self, target_path: str, files_data: bytes) -> None:
        response = requests.post(
            f'{self._client.endpoint}/api/v1/files/upload',
            headers=self._client.headers,
            json={
                'instance_id': self._instance.id,
                'target_path': target_path,
                'files_data': list(files_data),
            }
        )
        response.raise_for_status()