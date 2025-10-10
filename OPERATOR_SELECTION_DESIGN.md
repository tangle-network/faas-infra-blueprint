# Operator Selection & Load Balancing Design

## Core Insight

**By default**: ALL operators running the blueprint listen to ALL job call events and will ALL try to execute them.

**Solution**: Use smart contract storage to coordinate which operator should execute which job, allowing operators to filter events and only process jobs assigned to them.

---

## Architecture

### Smart Contract Storage (On-Chain Coordination)

```solidity
contract ZkFaasBlueprint is BlueprintServiceManagerBase {
    // ============================================================================
    // OPERATOR SELECTION STORAGE
    // ============================================================================

    /// Operator metadata for load balancing
    struct OperatorInfo {
        address operatorAddress;
        bytes32 publicKeyHash;
        string endpoint;           // e.g., "https://op1.faas.network"
        uint256 totalJobsExecuted;
        uint256 successfulJobs;
        uint256 failedJobs;
        uint256 lastJobTimestamp;
        uint256 currentLoad;       // Number of active jobs
        uint256 maxConcurrentJobs; // Operator capacity
        bool active;
    }

    /// Persistent container → operator assignment (sticky routing)
    struct ContainerAssignment {
        string containerId;
        address operator;
        uint256 assignedAt;
        bool active;
    }

    /// Job call → assigned operator mapping
    struct JobAssignment {
        uint64 jobCallId;
        address assignedOperator;
        uint256 assignedAt;
        bool executed;
    }

    // Storage mappings
    mapping(address => OperatorInfo) public operators;
    mapping(string => ContainerAssignment) public containerAssignments;  // containerId => operator
    mapping(uint64 => JobAssignment) public jobAssignments;              // jobCallId => operator
    mapping(address => uint256) public operatorNonces;                   // For round-robin

    address[] public operatorList;  // All active operators

    // ============================================================================
    // OPERATOR SELECTION LOGIC
    // ============================================================================

    /// Select operator for a new job using round-robin + load balancing
    function selectOperatorForJob(
        uint64 jobCallId,
        uint8 jobType,
        bytes calldata jobInputs
    ) internal returns (address) {
        // Parse inputs to check if this is for an existing container
        (bool isContainerJob, string memory containerId) = parseContainerJob(jobType, jobInputs);

        if (isContainerJob && bytes(containerId).length > 0) {
            // STICKY ROUTING: Use existing operator for this container
            ContainerAssignment memory assignment = containerAssignments[containerId];
            if (assignment.active && operators[assignment.operator].active) {
                jobAssignments[jobCallId] = JobAssignment({
                    jobCallId: jobCallId,
                    assignedOperator: assignment.operator,
                    assignedAt: block.timestamp,
                    executed: false
                });

                emit JobAssigned(jobCallId, assignment.operator, "sticky");
                return assignment.operator;
            }
        }

        // NEW JOB: Select operator using load balancing
        address selectedOperator = selectLeastLoadedOperator();

        jobAssignments[jobCallId] = JobAssignment({
            jobCallId: jobCallId,
            assignedOperator: selectedOperator,
            assignedAt: block.timestamp,
            executed: false
        });

        // Increment operator load
        operators[selectedOperator].currentLoad += 1;

        emit JobAssigned(jobCallId, selectedOperator, "load_balanced");
        return selectedOperator;
    }

    /// Select operator with lowest current load
    function selectLeastLoadedOperator() internal view returns (address) {
        require(operatorList.length > 0, "No operators available");

        address bestOperator = operatorList[0];
        uint256 lowestLoad = operators[bestOperator].currentLoad;

        for (uint256 i = 1; i < operatorList.length; i++) {
            address op = operatorList[i];
            if (!operators[op].active) continue;

            uint256 load = operators[op].currentLoad;

            // Prefer operator with lower load
            if (load < lowestLoad) {
                lowestLoad = load;
                bestOperator = op;
            }
            // If equal load, prefer operator with better success rate
            else if (load == lowestLoad) {
                uint256 bestSuccessRate = calculateSuccessRate(bestOperator);
                uint256 currentSuccessRate = calculateSuccessRate(op);
                if (currentSuccessRate > bestSuccessRate) {
                    bestOperator = op;
                }
            }
        }

        return bestOperator;
    }

    /// Calculate operator success rate (0-10000 = 0%-100%)
    function calculateSuccessRate(address operator) internal view returns (uint256) {
        OperatorInfo memory info = operators[operator];
        if (info.totalJobsExecuted == 0) return 10000; // New operators get 100%
        return (info.successfulJobs * 10000) / info.totalJobsExecuted;
    }

    /// Parse job inputs to determine if it's a container-specific job
    function parseContainerJob(uint8 jobType, bytes calldata inputs)
        internal pure returns (bool, string memory)
    {
        // Job types that operate on existing containers
        if (jobType == EXECUTE_ADVANCED_JOB_ID ||
            jobType == STOP_INSTANCE_JOB_ID ||
            jobType == PAUSE_INSTANCE_JOB_ID ||
            jobType == EXPOSE_PORT_JOB_ID ||
            jobType == UPLOAD_FILES_JOB_ID) {

            // First parameter is usually container/instance ID
            try abi.decode(inputs, (string)) returns (string memory containerId) {
                return (true, containerId);
            } catch {
                return (false, "");
            }
        }

        return (false, "");
    }

    // ============================================================================
    // HOOKS INTEGRATION
    // ============================================================================

    /// Modified onRegister to track operators
    function onRegister(
        ServiceOperators.OperatorPreferences calldata operator,
        bytes calldata registrationInputs
    ) external payable virtual override onlyFromMaster {
        (bytes32 publicKeyHash, string memory endpoint, uint256 maxConcurrentJobs) =
            abi.decode(registrationInputs, (bytes32, string, uint256));

        address operatorAddr = operatorToAddress(operator);

        // Add to operator list if new
        if (!operators[operatorAddr].active) {
            operatorList.push(operatorAddr);
        }

        operators[operatorAddr] = OperatorInfo({
            operatorAddress: operatorAddr,
            publicKeyHash: publicKeyHash,
            endpoint: endpoint,
            totalJobsExecuted: 0,
            successfulJobs: 0,
            failedJobs: 0,
            lastJobTimestamp: 0,
            currentLoad: 0,
            maxConcurrentJobs: maxConcurrentJobs > 0 ? maxConcurrentJobs : 10,
            active: true
        });

        emit OperatorRegistered(operator, publicKeyHash, endpoint);
    }

    /// NEW HOOK: Called when job is submitted (BEFORE operators execute)
    /// This is where we assign the operator!
    function onJobCall(
        uint64 serviceId,
        uint8 job,
        uint64 jobCallId,
        bytes calldata inputs
    ) external payable virtual override onlyFromMaster {
        // Select and assign operator for this job
        address assignedOperator = selectOperatorForJob(jobCallId, job, inputs);

        // Operators will read this assignment and only the assigned one will execute
        emit JobCallDispatched(serviceId, job, jobCallId, assignedOperator);
    }

    /// Modified onJobResult to track operator performance
    function onJobResult(
        uint64 serviceId,
        uint8 job,
        uint64 jobCallId,
        ServiceOperators.OperatorPreferences calldata operator,
        bytes calldata inputs,
        bytes calldata outputs
    ) external payable virtual override onlyFromMaster {
        address operatorAddr = operatorToAddress(operator);

        // Verify this operator was assigned to this job
        JobAssignment storage assignment = jobAssignments[jobCallId];
        require(assignment.assignedOperator == operatorAddr, "Operator not assigned to this job");
        require(!assignment.executed, "Job already executed");

        // Mark as executed
        assignment.executed = true;

        // Update operator stats
        OperatorInfo storage opInfo = operators[operatorAddr];
        opInfo.totalJobsExecuted += 1;
        opInfo.lastJobTimestamp = block.timestamp;
        opInfo.currentLoad -= 1;  // Job completed, reduce load

        // Determine if job succeeded
        bool success = true;
        try this.validateJobResult(job, outputs) {
            opInfo.successfulJobs += 1;
        } catch {
            opInfo.failedJobs += 1;
            success = false;
        }

        // Handle job-specific logic
        if (job == START_INSTANCE_JOB_ID) {
            _handleStartInstance(operatorAddr, inputs, outputs);
        }
        else if (job == STOP_INSTANCE_JOB_ID) {
            _handleStopInstance(inputs);
        }
        // ... other jobs

        emit JobResultProcessed(serviceId, job, jobCallId, operator, success);
    }

    /// Store container → operator mapping for START_INSTANCE
    function _handleStartInstance(
        address operator,
        bytes calldata inputs,
        bytes calldata outputs
    ) internal {
        // Parse outputs to get container ID
        (string memory containerId, string memory endpoint) =
            abi.decode(outputs, (string, string));

        // Assign container to this operator (sticky routing)
        containerAssignments[containerId] = ContainerAssignment({
            containerId: containerId,
            operator: operator,
            assignedAt: block.timestamp,
            active: true
        });

        emit ContainerAssigned(containerId, operator, endpoint);
    }

    /// Remove container assignment for STOP_INSTANCE
    function _handleStopInstance(bytes calldata inputs) internal {
        (string memory containerId) = abi.decode(inputs, (string));

        ContainerAssignment storage assignment = containerAssignments[containerId];
        if (assignment.active) {
            assignment.active = false;
            emit ContainerStopped(containerId, assignment.operator);
        }
    }

    /// Validate job result (can be overridden for custom validation)
    function validateJobResult(uint8 job, bytes calldata outputs) external pure {
        // Basic validation - can be extended
        require(outputs.length > 0, "Empty result");
    }

    // ============================================================================
    // VIEW FUNCTIONS (For Operators to Query)
    // ============================================================================

    /// Check if operator is assigned to a job
    function isOperatorAssignedToJob(uint64 jobCallId, address operator)
        external view returns (bool)
    {
        return jobAssignments[jobCallId].assignedOperator == operator;
    }

    /// Get operator assigned to a container
    function getContainerOperator(string calldata containerId)
        external view returns (address)
    {
        return containerAssignments[containerId].operator;
    }

    /// Get operator stats
    function getOperatorStats(address operator)
        external view returns (
            uint256 totalJobs,
            uint256 successfulJobs,
            uint256 failedJobs,
            uint256 currentLoad,
            uint256 successRate
        )
    {
        OperatorInfo memory info = operators[operator];
        return (
            info.totalJobsExecuted,
            info.successfulJobs,
            info.failedJobs,
            info.currentLoad,
            calculateSuccessRate(operator)
        );
    }

    // ============================================================================
    // EVENTS
    // ============================================================================

    event JobAssigned(uint64 indexed jobCallId, address indexed operator, string strategy);
    event JobCallDispatched(uint64 indexed serviceId, uint8 indexed job, uint64 jobCallId, address assignedOperator);
    event ContainerAssigned(string indexed containerId, address indexed operator, string endpoint);
    event ContainerStopped(string indexed containerId, address indexed operator);
}
```

---

## Operator Implementation (Off-Chain Filtering)

### Blueprint Runner with Custom Event Filtering

```rust
// In faas-bin/src/main.rs

use blueprint_sdk::{
    tangle::{
        consumer::TangleConsumer,
        producer::TangleProducer,
    },
};

#[tokio::main]
async fn main() -> eyre::Result<()> {
    // ... setup ...

    let tangle_client = env.tangle_client().await?;
    let keystore = env.keystore();
    let sr25519_signer = keystore.first_local::<SpSr25519>()?;
    let operator_address = sr25519_signer.to_account_id();

    info!("Operator address: {:?}", operator_address);

    // Create custom producer with event filtering
    let producer = TangleProducer::finalized_blocks(tangle_client.rpc_client.clone())
        .await?
        .with_filter(move |event| {
            // Only process events assigned to this operator
            should_process_event(&event, &operator_address)
        });

    // ... rest of setup ...
}

/// Filter function: Only process events assigned to this operator
fn should_process_event(event: &TangleEvent, operator_address: &AccountId) -> bool {
    match event {
        TangleEvent::JobCall { job_call_id, .. } => {
            // Query smart contract to check if we're assigned
            // This would be done via RPC call to the contract
            is_operator_assigned_to_job(*job_call_id, operator_address)
        }
        _ => true, // Process other events normally
    }
}

/// Query smart contract for job assignment
fn is_operator_assigned_to_job(job_call_id: u64, operator_address: &AccountId) -> bool {
    // Make RPC call to smart contract's isOperatorAssignedToJob function
    // Return true only if this operator is assigned

    // Pseudocode:
    // let contract = ZkFaasBlueprint::at(contract_address);
    // contract.is_operator_assigned_to_job(job_call_id, operator_address).call().await

    // For now, simplified:
    true  // TODO: Implement actual contract query
}
```

---

## Alternative: Job Handler Level Filtering

If we can't filter at the producer level, filter in each job handler:

```rust
// In faas-lib/src/jobs.rs

pub async fn execute_function_job(
    Context(ctx): Context<FaaSContext>,
    CallId(call_id): CallId,
    TangleArg(args): TangleArg<ExecuteFunctionArgs>,
) -> Result<TangleResult<Vec<u8>>, JobError> {
    // STEP 1: Check if this operator is assigned to this job
    let operator_address = ctx.config.keystore()
        .first_local::<SpSr25519>()?
        .to_account_id();

    let is_assigned = query_smart_contract_assignment(call_id, &operator_address).await?;

    if !is_assigned {
        info!("Job {call_id} not assigned to this operator, skipping");
        // Return early WITHOUT submitting result
        // This prevents duplicate execution
        return Err(JobError::NotAssigned);
    }

    info!("Job {call_id} assigned to this operator, executing");

    // STEP 2: Execute job normally
    let request = PlatformRequest {
        id: format!("job_{call_id}"),
        code: args.command.join(" "),
        mode: Mode::Ephemeral,
        env: args.image,
        timeout: Duration::from_secs(60),
        checkpoint: None,
        branch_from: None,
        runtime: None,
    };

    let response = ctx.executor.run(request).await?;

    // STEP 3: Submit result to chain (only assigned operator does this)
    Ok(TangleResult(response.stdout))
}

/// Query smart contract to check job assignment
async fn query_smart_contract_assignment(
    job_call_id: u64,
    operator_address: &AccountId,
) -> Result<bool, JobError> {
    // TODO: Implement actual contract query via RPC
    // contract.isOperatorAssignedToJob(job_call_id, operator_address)
    Ok(true)
}
```

---

## Benefits of This Design

### 1. **On-Chain Coordination**
- Smart contract decides operator assignment deterministically
- All operators can verify assignment independently
- Slashable if operator executes job not assigned to them

### 2. **Load Balancing**
- Least-loaded operator gets new jobs
- Success rate factored into selection
- Sticky routing for persistent containers

### 3. **No Duplicate Work**
- Only assigned operator executes job
- Other operators skip via early return
- Prevents wasted compute

### 4. **Operator Reputation**
- Success/failure tracked on-chain
- Low-performing operators get fewer jobs
- Slashing for malicious behavior

### 5. **Minimal Changes to SDKs**
- Filtering happens in job handlers (Rust code)
- Smart contract handles assignment logic
- No changes to user-facing APIs

---

## Implementation Plan

### Phase 1: Smart Contract Updates (1 week)
1. Add operator tracking storage
2. Implement `onJobCall` hook with operator selection
3. Add job assignment validation in `onJobResult`
4. Implement sticky routing for containers
5. Add slashing for unauthorized execution

### Phase 2: Job Handler Filtering (1 week)
1. Add contract query helper functions
2. Update each job handler with assignment check
3. Add early return for unassigned jobs
4. Test with multiple operators

### Phase 3: Operator Registration (3 days)
1. Update `onRegister` to collect operator metadata
2. Add operator endpoint (gateway URL) to registration
3. Update operator stats on job completion

### Phase 4: Testing & Validation (1 week)
1. Deploy 3+ operators
2. Submit 100+ jobs, verify load distribution
3. Test sticky routing with persistent containers
4. Verify only assigned operators execute jobs
5. Test slashing for unauthorized execution

---

## Key Questions Resolved

**Q: How do operators know which jobs to execute?**
A: Smart contract assigns operator in `onJobCall` hook. Operators query assignment before executing.

**Q: How do we prevent duplicate execution?**
A: Job handlers check assignment and return early if not assigned. Contract rejects results from unassigned operators.

**Q: How do we implement load balancing?**
A: Smart contract selects least-loaded operator using on-chain load tracking.

**Q: How do we handle sticky routing (vibecoding)?**
A: Container → operator mapping stored on-chain. All jobs for that container route to same operator.

**Q: How do we slash malicious operators?**
A: Contract validates operator was assigned before accepting result. Can implement slashing for violations.

**Q: Do we need to modify pallet-services?**
A: NO. Everything done via smart contract hooks and job handler logic.

---

## Next Steps

1. Implement `onJobCall` hook in smart contract
2. Add operator selection logic (load balancing)
3. Add sticky routing for containers
4. Update job handlers to query assignment
5. Test with multi-operator setup
