# Python SDK Implementation

## Package Structure

```
sdk/python/
├── faas_sdk/
│   ├── __init__.py
│   ├── client.py
│   ├── snapshots.py
│   ├── instances.py
│   ├── branches.py
│   ├── execution.py
│   ├── files.py
│   ├── network.py
│   ├── ssh.py
│   ├── errors.py
│   └── utils.py
├── tests/
├── examples/
├── setup.py
└── requirements.txt
```

## Core Client

```python
# faas_sdk/client.py

from typing import Optional, Dict, Any
import os
from .snapshots import SnapshotManager
from .instances import InstanceManager
from .branches import BranchManager

class FaaSClient:
    """Main client for FaaS platform operations."""

    def __init__(
        self,
        api_key: Optional[str] = None,
        endpoint: str = "https://api.faas.io",
        timeout: int = 30,
        max_retries: int = 3
    ):
        self.api_key = api_key or os.environ.get("FAAS_API_KEY")
        self.endpoint = endpoint
        self.timeout = timeout
        self.max_retries = max_retries

        self.snapshots = SnapshotManager(self)
        self.instances = InstanceManager(self)
        self.branches = BranchManager(self)
        self.execution = ExecutionManager(self)
        self.files = FileManager(self)
        self.network = NetworkManager(self)

    def exec(self, command: str, target=None) -> ExecutionResult:
        """Execute command with automatic snapshot creation."""
        return self.execution.exec(command, target)

    def branch(self, snapshot_id: Optional[str] = None) -> Branch:
        """Create execution branch for parallel operations."""
        return self.branches.create(snapshot_id)
```

## Snapshot Operations

```python
# faas_sdk/snapshots.py

from typing import Optional, List, Dict, Any
from dataclasses import dataclass
from datetime import datetime

@dataclass
class Snapshot:
    id: str
    parent_id: Optional[str]
    metadata: Dict[str, Any]
    created_at: datetime
    size: int
    checksum: str

    def exec(self, command: str) -> 'Snapshot':
        """Execute command and create new snapshot."""
        pass

    def upload(self, local_path: str, remote_path: str) -> 'Snapshot':
        """Upload file and create new snapshot."""
        pass

    def download(self, remote_path: str, local_path: str) -> None:
        """Download file from snapshot."""
        pass

class SnapshotManager:
    def __init__(self, client):
        self.client = client

    def create(
        self,
        base_image: Optional[str] = None,
        vcpus: int = 1,
        memory: int = 512,
        disk_size: int = 10240,
        **kwargs
    ) -> Snapshot:
        """Create new snapshot."""
        pass

    def get(self, snapshot_id: str) -> Snapshot:
        """Get snapshot by ID."""
        pass

    def list(self, filter: Optional[Dict] = None) -> List[Snapshot]:
        """List snapshots with optional filter."""
        pass

    def delete(self, snapshot_id: str) -> None:
        """Delete snapshot."""
        pass

    def chain(self, *operations) -> Snapshot:
        """Chain multiple operations on snapshots."""
        snapshot = None
        for op in operations:
            snapshot = op(snapshot)
        return snapshot
```

## Instance Management

```python
# faas_sdk/instances.py

from typing import Optional, List, Dict, Any
from dataclasses import dataclass
from enum import Enum
import asyncio

class InstanceState(Enum):
    PENDING = "pending"
    RUNNING = "running"
    PAUSED = "paused"
    STOPPED = "stopped"
    TERMINATED = "terminated"

@dataclass
class Instance:
    id: str
    snapshot_id: str
    state: InstanceState
    endpoints: Dict[str, str]
    resources: Dict[str, Any]

    def ssh(self) -> 'SSHClient':
        """Get SSH client for instance."""
        pass

    def exec(self, command: str) -> ExecutionResult:
        """Execute command on instance."""
        pass

class InstanceManager:
    def __init__(self, client):
        self.client = client

    def start(
        self,
        snapshot_id: str,
        auto_stop: bool = True,
        ttl: Optional[int] = None
    ) -> Instance:
        """Start new instance from snapshot."""
        pass

    def stop(self, instance_id: str) -> None:
        """Stop instance."""
        pass

    def pause(self, instance_id: str) -> None:
        """Pause instance."""
        pass

    def resume(self, instance_id: str) -> None:
        """Resume paused instance."""
        pass

    def wait_until_ready(self, instance_id: str, timeout: int = 300) -> None:
        """Wait until instance is ready."""
        pass

class ManagedInstance:
    """Context manager for automatic instance lifecycle."""

    def __init__(self, client, snapshot_id: str):
        self.client = client
        self.snapshot_id = snapshot_id
        self.instance = None

    def __enter__(self) -> Instance:
        self.instance = self.client.instances.start(self.snapshot_id)
        self.client.instances.wait_until_ready(self.instance.id)
        return self.instance

    def __exit__(self, *args):
        if self.instance:
            self.client.instances.stop(self.instance.id)
```

## Branching Operations

```python
# faas_sdk/branches.py

from typing import Optional, List, Dict, Any
from dataclasses import dataclass

@dataclass
class Branch:
    id: str
    parent_id: str
    name: Optional[str]
    metadata: Dict[str, Any]
    divergence_point: Snapshot

    def exec(self, command: str) -> 'Branch':
        """Execute command on branch."""
        pass

    def merge(self, other: 'Branch') -> Snapshot:
        """Merge with another branch."""
        pass

class BranchManager:
    def __init__(self, client):
        self.client = client

    def create(self, snapshot_id: str, name: Optional[str] = None) -> Branch:
        """Create new branch from snapshot."""
        pass

    def list(self, parent_id: Optional[str] = None) -> List[Branch]:
        """List branches."""
        pass

    def parallel_map(self, items: List[Any], fn) -> List[Any]:
        """Execute function on multiple branches in parallel."""
        branches = [self.create() for _ in items]
        results = []
        for branch, item in zip(branches, items):
            result = fn(item, branch)
            results.append(result)
        return results
```

## Execution Operations

```python
# faas_sdk/execution.py

from typing import Union, List, Optional
from dataclasses import dataclass

@dataclass
class ExecutionResult:
    exit_code: int
    stdout: str
    stderr: str
    duration: float
    snapshot_id: Optional[str] = None

class ExecutionManager:
    def __init__(self, client):
        self.client = client

    def exec(
        self,
        command: str,
        target: Optional[Union[str, Instance, Snapshot]] = None
    ) -> ExecutionResult:
        """Execute command on target."""
        pass

    def exec_batch(
        self,
        commands: List[str],
        target: Optional[Union[str, Instance, Snapshot]] = None
    ) -> List[ExecutionResult]:
        """Execute multiple commands."""
        pass

    def exec_script(
        self,
        script: str,
        target: Optional[Union[str, Instance, Snapshot]] = None,
        interpreter: str = "bash"
    ) -> ExecutionResult:
        """Execute script on target."""
        pass
```

## File Operations

```python
# faas_sdk/files.py

from typing import Union, Optional, List
from pathlib import Path
from dataclasses import dataclass

@dataclass
class FileInfo:
    path: str
    size: int
    modified: datetime
    is_directory: bool
    permissions: str

class FileManager:
    def __init__(self, client):
        self.client = client

    def upload(
        self,
        target: Union[str, Instance, Snapshot],
        local_path: Union[str, Path],
        remote_path: str
    ) -> None:
        """Upload file or directory."""
        pass

    def download(
        self,
        target: Union[str, Instance, Snapshot],
        remote_path: str,
        local_path: Union[str, Path]
    ) -> None:
        """Download file or directory."""
        pass

    def sync(
        self,
        target: Union[str, Instance, Snapshot],
        local_path: Union[str, Path],
        remote_path: str,
        bidirectional: bool = False
    ) -> None:
        """Sync files between local and remote."""
        pass
```

## SSH Operations

```python
# faas_sdk/ssh.py

from typing import Optional, Dict, Any
import paramiko

class SSHClient:
    def __init__(self, instance: Instance):
        self.instance = instance
        self.client = None

    def connect(self) -> None:
        """Establish SSH connection."""
        pass

    def exec_command(self, command: str) -> ExecutionResult:
        """Execute command via SSH."""
        pass

    def put_file(self, local_path: str, remote_path: str) -> None:
        """Upload file via SSH."""
        pass

    def get_file(self, remote_path: str, local_path: str) -> None:
        """Download file via SSH."""
        pass

    def create_shell(self) -> InteractiveShell:
        """Create interactive shell."""
        pass

    def close(self) -> None:
        """Close SSH connection."""
        pass

    def __enter__(self):
        self.connect()
        return self

    def __exit__(self, *args):
        self.close()
```

## Usage Examples

```python
from faas_sdk import FaaSClient

# Initialize client
client = FaaSClient()

# Simple execution
result = client.exec("echo 'Hello World'")

# Snapshot chaining
snapshot = (client.snapshots
    .create(base_image="ubuntu:22.04")
    .exec("apt-get update")
    .exec("apt-get install -y python3 python3-pip")
    .upload("./requirements.txt", "/tmp/requirements.txt")
    .exec("pip install -r /tmp/requirements.txt"))

# Instance management with context manager
with ManagedInstance(client, snapshot.id) as instance:
    result = instance.exec("python app.py")
    print(result.stdout)

# Parallel execution
def process_data(data_file, branch):
    branch.upload(data_file, f"/data/{data_file}")
    return branch.exec(f"python process.py /data/{data_file}")

data_files = ["data1.csv", "data2.csv", "data3.csv"]
results = client.branches.parallel_map(data_files, process_data)

# SSH operations
instance = client.instances.start(snapshot_id=snapshot.id)
with instance.ssh() as ssh:
    ssh.put_file("./script.py", "/tmp/script.py")
    result = ssh.exec_command("python /tmp/script.py")
    ssh.get_file("/tmp/output.json", "./output.json")
```