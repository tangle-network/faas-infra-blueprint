# FaaS SDK Architecture Design

## Overview

Based on analysis of Morph Cloud's SDK design, we'll create multi-language SDKs for our FaaS system that provide:
- Simple, intuitive APIs for container/VM management
- Snapshot and branching capabilities (future)
- Type-safe interfaces (TypeScript) and Pythonic APIs (Python)
- Consistent experience across languages

## Core SDK Features

### 1. Python SDK (`faas-python`)

```python
from faas import FaaSClient
from faas.types import Environment, ExecutionResult

class FaaSClient:
    """Main client for FaaS operations"""

    def __init__(self, api_key: str = None, endpoint: str = None):
        self.api_key = api_key or os.getenv("FAAS_API_KEY")
        self.endpoint = endpoint or os.getenv("FAAS_ENDPOINT", "http://localhost:8080")
        self.environments = EnvironmentManager(self)
        self.executions = ExecutionManager(self)
        self.snapshots = SnapshotManager(self)  # Future

    async def execute(
        self,
        code: str,
        environment: str = "alpine-fast",
        timeout: int = 30,
        resources: dict = None
    ) -> ExecutionResult:
        """Execute code in specified environment"""
        pass

    async def execute_file(self, file_path: str, **kwargs) -> ExecutionResult:
        """Execute a file in the FaaS environment"""
        pass

class EnvironmentManager:
    """Manage execution environments"""

    async def list(self) -> List[Environment]:
        """List available environments"""
        pass

    async def create(
        self,
        name: str,
        base_image: str,
        layers: List[Layer] = None,
        resources: ResourceRequirements = None
    ) -> Environment:
        """Create a custom environment"""
        pass

    async def get(self, name: str) -> Environment:
        """Get environment details"""
        pass

    async def update(self, name: str, **updates) -> Environment:
        """Update environment configuration"""
        pass

    async def delete(self, name: str) -> bool:
        """Delete an environment"""
        pass

class ExecutionManager:
    """Manage code executions"""

    async def run(
        self,
        command: List[str],
        environment: str,
        stdin: bytes = None,
        env_vars: dict = None,
        timeout: int = 30
    ) -> ExecutionResult:
        """Run a command in an environment"""
        pass

    async def run_batch(
        self,
        executions: List[ExecutionRequest]
    ) -> List[ExecutionResult]:
        """Run multiple executions in parallel"""
        pass

    async def stream(
        self,
        command: List[str],
        environment: str,
        **kwargs
    ) -> AsyncIterator[str]:
        """Stream execution output"""
        pass

class SnapshotManager:
    """Manage environment snapshots (future feature)"""

    async def create(
        self,
        execution_id: str,
        name: str = None
    ) -> Snapshot:
        """Create a snapshot of a running execution"""
        pass

    async def restore(
        self,
        snapshot_id: str,
        branch_count: int = 1
    ) -> Union[Execution, List[Execution]]:
        """Restore from snapshot, optionally branching"""
        pass

    async def list(self) -> List[Snapshot]:
        """List available snapshots"""
        pass

    async def delete(self, snapshot_id: str) -> bool:
        """Delete a snapshot"""
        pass
```

### 2. TypeScript SDK (`@faas/sdk`)

```typescript
// Core client
export class FaaSClient {
    constructor(config?: FaaSConfig) {
        this.apiKey = config?.apiKey || process.env.FAAS_API_KEY;
        this.endpoint = config?.endpoint || process.env.FAAS_ENDPOINT || "http://localhost:8080";

        this.environments = new EnvironmentManager(this);
        this.executions = new ExecutionManager(this);
        this.snapshots = new SnapshotManager(this); // Future
    }

    async execute(
        code: string,
        options?: ExecutionOptions
    ): Promise<ExecutionResult> {
        // Execute code with specified options
    }

    async executeFile(
        filePath: string,
        options?: ExecutionOptions
    ): Promise<ExecutionResult> {
        // Execute file contents
    }
}

// Environment management
export class EnvironmentManager {
    async list(): Promise<Environment[]> { }

    async create(config: EnvironmentConfig): Promise<Environment> { }

    async get(name: string): Promise<Environment> { }

    async update(
        name: string,
        updates: Partial<EnvironmentConfig>
    ): Promise<Environment> { }

    async delete(name: string): Promise<boolean> { }
}

// Execution management
export class ExecutionManager {
    async run(
        command: string[],
        environment: string,
        options?: RunOptions
    ): Promise<ExecutionResult> { }

    async runBatch(
        executions: ExecutionRequest[]
    ): Promise<ExecutionResult[]> { }

    async *stream(
        command: string[],
        environment: string,
        options?: StreamOptions
    ): AsyncGenerator<string> { }
}

// Snapshot management (future)
export class SnapshotManager {
    async create(
        executionId: string,
        name?: string
    ): Promise<Snapshot> { }

    async restore(
        snapshotId: string,
        options?: RestoreOptions
    ): Promise<Execution | Execution[]> { }

    async list(): Promise<Snapshot[]> { }

    async delete(snapshotId: string): Promise<boolean> { }
}

// Types
export interface ExecutionResult {
    requestId: string;
    status: 'success' | 'error' | 'timeout';
    stdout?: string;
    stderr?: string;
    exitCode?: number;
    duration: number;
    cached: boolean;
}

export interface Environment {
    id: string;
    name: string;
    baseImage: string;
    layers?: Layer[];
    resources?: ResourceRequirements;
    cacheStrategy?: CacheStrategy;
    features?: Record<string, boolean>;
}
```

### 3. Rust SDK (`faas-rust-sdk`)

```rust
use faas_sdk::{FaaSClient, Environment, ExecutionResult};

pub struct FaaSClient {
    api_key: String,
    endpoint: String,
    client: reqwest::Client,
}

impl FaaSClient {
    pub fn new(api_key: Option<String>, endpoint: Option<String>) -> Self {
        // Initialize client
    }

    pub async fn execute(
        &self,
        code: &str,
        environment: Option<&str>,
        timeout: Option<Duration>,
    ) -> Result<ExecutionResult> {
        // Execute code
    }

    pub fn environments(&self) -> EnvironmentManager {
        EnvironmentManager::new(self)
    }

    pub fn executions(&self) -> ExecutionManager {
        ExecutionManager::new(self)
    }

    pub fn snapshots(&self) -> SnapshotManager {
        SnapshotManager::new(self)
    }
}
```

### 4. CLI Tool (`faas-cli`)

```bash
# Installation
cargo install faas-cli
# or
npm install -g @faas/cli
# or
pip install faas-cli

# Usage examples
faas execute "echo 'Hello World'" --env alpine-fast
faas execute script.py --env python-ai --timeout 60
faas env list
faas env create my-env --base rust:latest --cache aggressive
faas snapshot create --execution-id abc123 --name "checkpoint-1"
faas snapshot restore checkpoint-1 --branch 5  # Create 5 branches
```

## SDK Design Principles

### 1. Consistency Across Languages
- Same method names and patterns
- Similar object hierarchies
- Consistent error handling

### 2. Progressive Disclosure
- Simple operations are simple
- Complex features are available but not required
- Sensible defaults

### 3. Type Safety
- Strong typing in TypeScript and Rust
- Type hints in Python
- Runtime validation

### 4. Async-First
- All I/O operations are async
- Support for streaming responses
- Proper cancellation handling

### 5. Developer Experience
- Excellent IDE support
- Comprehensive documentation
- Rich error messages
- Debug logging

## Implementation Phases

### Phase 1: Core SDK (Week 1-2)
- [ ] Basic client setup
- [ ] Authentication
- [ ] Simple execute API
- [ ] Environment listing

### Phase 2: Advanced Execution (Week 3-4)
- [ ] Batch execution
- [ ] Streaming output
- [ ] File uploads
- [ ] Environment variables

### Phase 3: Environment Management (Week 5-6)
- [ ] Custom environment creation
- [ ] Layer management
- [ ] Resource configuration
- [ ] Cache strategies

### Phase 4: Snapshot Support (Week 7-8)
- [ ] Snapshot creation
- [ ] Restore functionality
- [ ] Branching support
- [ ] State management

## API Endpoint Design

```yaml
/api/v1:
  /execute:
    POST: Execute code
  /executions:
    GET: List executions
    /{id}:
      GET: Get execution details
      DELETE: Cancel execution
      /logs:
        GET: Stream logs
      /snapshot:
        POST: Create snapshot
  /environments:
    GET: List environments
    POST: Create environment
    /{name}:
      GET: Get environment
      PUT: Update environment
      DELETE: Delete environment
  /snapshots:
    GET: List snapshots
    POST: Create snapshot
    /{id}:
      GET: Get snapshot
      DELETE: Delete snapshot
      /restore:
        POST: Restore snapshot
      /branch:
        POST: Branch snapshot
```

## Example Usage Scenarios

### 1. Simple Execution
```python
from faas import FaaSClient

client = FaaSClient()
result = await client.execute("print('Hello World')", environment="python-ai")
print(result.stdout)
```

### 2. Blockchain Development
```typescript
const client = new FaaSClient();

const result = await client.execute(`
    cargo init my-contract
    cd my-contract
    echo '[dependencies]' >> Cargo.toml
    echo 'anchor-lang = "0.29"' >> Cargo.toml
    cargo build --release
`, {
    environment: "rust-blockchain-v1",
    timeout: 120
});
```

### 3. Parallel Testing (Future with Snapshots)
```python
# Create a base snapshot with test environment
base = await client.snapshots.create_from_script("""
    pip install pytest
    git clone https://github.com/myrepo/project
    cd project
    pip install -r requirements.txt
""", name="test-base")

# Branch into parallel test runs
branches = await client.snapshots.restore(
    base.id,
    branch_count=10
)

# Run different test suites in parallel
results = await asyncio.gather(*[
    branch.execute(f"pytest tests/test_module_{i}.py")
    for i, branch in enumerate(branches)
])
```

## Security Considerations

1. **API Key Management**
   - Secure storage in environment variables
   - Key rotation support
   - Scoped permissions

2. **Resource Limits**
   - Per-user quotas
   - Rate limiting
   - Timeout enforcement

3. **Code Isolation**
   - Sandboxed execution
   - Network isolation options
   - File system restrictions

## Next Steps

1. **Prototype Python SDK** - Start with Python as it's most flexible
2. **Define OpenAPI Spec** - Create formal API specification
3. **Build TypeScript SDK** - Leverage strong typing
4. **Create CLI Tool** - Use Rust for performance
5. **Write Documentation** - Comprehensive guides and examples
6. **Implement Snapshot API** - Add branching capabilities