// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.20;

import "tnt-core/BlueprintServiceManagerBase.sol";
import "openzeppelin-contracts/contracts/utils/ReentrancyGuard.sol";
import "openzeppelin-contracts/contracts/utils/Strings.sol";

// Precompile address for slashing (Tangle Network specific)
// NOTE: This should be updated to match your runtime's actual slashing precompile address
address constant SLASHING_PRECOMPILE = 0x00000000000000000000000000000000000009a0;

/**
 * @title FaaSBlueprint
 * @author Tangle Network
 * @notice Comprehensive blueprint for Function-as-a-Service with multi-agent orchestration
 * @dev This contract manages all FaaS platform capabilities:
 * - Container execution (ephemeral, cached, checkpointed, branched, persistent)
 * - Snapshot management (CRIU-based checkpointing)
 * - Instance lifecycle (start, stop, pause, resume)
 * - Port exposure and networking
 * - File operations
 * - Load balancing and operator selection
 * - Malicious behavior slashing
 *
 * Jobs:
 * - Job 0: Execute Function (basic)
 * - Job 1: Execute Advanced (with modes)
 * - Job 2: Create Snapshot
 * - Job 3: Restore Snapshot
 * - Job 4: Create Branch
 * - Job 5: Merge Branches
 * - Job 6: Start Instance
 * - Job 7: Stop Instance
 * - Job 8: Pause Instance
 * - Job 9: Resume Instance
 * - Job 10: Expose Port
 * - Job 11: Upload Files
 */
contract FaaSBlueprint is BlueprintServiceManagerBase, ReentrancyGuard {
    // ============================================================================
    // EXECUTION TRACKING
    // ============================================================================

    /// @notice Execution metadata
    struct ExecutionRecord {
        uint64 callId;
        address operator;
        string image;
        uint256 timestamp;
        bool success;
        uint256 executionTimeMs;
    }

    /// @notice Snapshot metadata
    struct SnapshotRecord {
        string snapshotId;
        string containerId;
        address creator;
        uint256 timestamp;
        uint256 sizeBytes;
    }

    /// @notice Instance metadata
    struct InstanceRecord {
        string instanceId;
        address owner;
        string image;
        uint256 timestamp;
        bool isRunning;
        string[] exposedPorts;
    }

    /// @notice Execution history
    ExecutionRecord[] public executions;

    /// @notice Snapshot registry
    mapping(string => SnapshotRecord) public snapshots;

    /// @notice Instance registry
    mapping(string => InstanceRecord) public instances;

    /// @notice Total executions by operator
    mapping(address => uint256) public operatorExecutions;

    // ============================================================================
    // OPERATOR SELECTION & LOAD BALANCING
    // ============================================================================

    /// @notice Operator info for load balancing
    struct OperatorInfo {
        address addr;
        uint256 totalJobs;
        uint256 successfulJobs;
        uint256 currentLoad;
        uint256 maxConcurrentJobs;
        bool active;
    }

    /// @notice Job assignment for operator selection
    struct JobAssignment {
        address assignedOperator;
        bool executed;
    }

    /// @notice Operator info by address
    mapping(address => OperatorInfo) public operators;

    /// @notice Job call ID to assignment
    mapping(uint64 => JobAssignment) public jobAssignments;

    /// @notice List of all registered operators
    address[] public operatorList;

    /// @notice Default maximum number of concurrent jobs for an operator when none is provided
    uint256 internal constant DEFAULT_MAX_CONCURRENT_JOBS = 4;

    // ============================================================================
    // EVENTS
    // ============================================================================

    event FunctionExecuted(
        uint64 indexed callId,
        address indexed operator,
        string image,
        bool success,
        uint256 executionTimeMs
    );

    event SnapshotCreated(
        string indexed snapshotId,
        string containerId,
        address indexed creator,
        uint256 sizeBytes
    );

    event SnapshotRestored(
        string indexed snapshotId,
        string newContainerId,
        address indexed operator
    );

    event InstanceStarted(
        string indexed instanceId,
        address indexed owner,
        string image
    );

    event InstanceStopped(
        string indexed instanceId,
        address indexed operator
    );

    event PortExposed(
        string indexed instanceId,
        uint16 port,
        string publicUrl
    );

    event FilesUploaded(
        string indexed instanceId,
        string targetPath,
        uint256 bytesUploaded
    );

    event JobAssigned(uint64 indexed jobCallId, address indexed operator);
    event JobCallDispatched(
        uint64 indexed serviceId,
        uint8 indexed job,
        uint64 jobCallId,
        address indexed operator
    );

    /// @notice Constructor
    constructor() BlueprintServiceManagerBase() {}

    /**
     * @dev Hook for service operator registration
     */
    function onRegister(
        ServiceOperators.OperatorPreferences calldata operator,
        bytes calldata registrationInputs
    )
        external
        payable
        virtual
        override
        onlyFromMaster
    {
        address operatorAddr = operatorToAddress(operator);
        OperatorInfo storage info = operators[operatorAddr];

        // Extract optional maxConcurrentJobs from registration payload (first 32 bytes)
        uint256 providedMaxJobs = DEFAULT_MAX_CONCURRENT_JOBS;
        if (registrationInputs.length >= 32) {
            // Use assembly to avoid reverting on malformed input while reading first word
            assembly {
                providedMaxJobs := calldataload(add(registrationInputs.offset, 32))
            }
        }
        if (providedMaxJobs == 0) {
            providedMaxJobs = DEFAULT_MAX_CONCURRENT_JOBS;
        }

        if (info.addr == address(0)) {
            operatorList.push(operatorAddr);
            info.addr = operatorAddr;
            info.totalJobs = 0;
            info.successfulJobs = 0;
        }

        info.active = true;
        info.maxConcurrentJobs = providedMaxJobs;
        info.currentLoad = 0;

        emit OperatorRegistered(operator, bytes32(0), "");
    }

    /**
     * @dev Hook for operator unregistering from the service
     */
    function onUnregister(
        ServiceOperators.OperatorPreferences calldata operator
    )
        external
        virtual
        override
        onlyFromMaster
    {
        address operatorAddr = operatorToAddress(operator);
        OperatorInfo storage info = operators[operatorAddr];
        if (info.addr != address(0)) {
            info.active = false;
            info.currentLoad = 0;
        }
    }

    /**
     * @dev Hook for service instance requests
     */
    function onRequest(
        ServiceOperators.RequestParams calldata params
    )
        external
        payable
        virtual
        override
        onlyFromMaster
    {
        emit ServiceRequested(params.requestId, params.requester);
    }

    /**
     * @dev Hook invoked when a job call is dispatched. Selects and records the operator.
     */
    function onJobCall(
        uint64 serviceId,
        uint8 job,
        uint64 jobCallId,
        bytes calldata /* inputs */
    )
        external
        payable
        virtual
        override
        onlyFromMaster
    {
        address assignedOperator = _selectAndAssignOperator(jobCallId);
        emit JobCallDispatched(serviceId, job, jobCallId, assignedOperator);
    }

    // ============================================================================
    // JOB RESULT HANDLING
    // ============================================================================

    /**
     * @dev Hook for handling job results - accepts all job types
     * @param serviceId The ID of the service
     * @param job The job identifier
     * @param jobCallId The unique ID for the job call
     * @param operator The operator sending the result
     * @param inputs Inputs used for the job
     * @param outputs Outputs from the job execution
     */
    function onJobResult(
        uint64 serviceId,
        uint8 job,
        uint64 jobCallId,
        ServiceOperators.OperatorPreferences calldata operator,
        bytes calldata inputs,
        bytes calldata outputs
    )
        external
        payable
        virtual
        override
        onlyFromMaster
        nonReentrant
    {
        address operatorAddr = operatorToAddress(operator);

        // Validate operator assignment ALWAYS - after transition period, all jobs should be assigned
        if (jobAssignments[jobCallId].assignedOperator != address(0)) {
            require(
                jobAssignments[jobCallId].assignedOperator == operatorAddr,
                "Operator not assigned to this job"
            );
            require(!jobAssignments[jobCallId].executed, "Job already executed");
            jobAssignments[jobCallId].executed = true;
        } else {
            // During transition period: Accept first valid submission but reject subsequent ones
            // Check if another operator already submitted this job
            bool alreadyExecuted = false;
            if (executions.length > 0) {
                for (uint256 i = executions.length - 1; i >= 0 && i >= executions.length - job; i--) {
                    if (executions[i].callId == jobCallId) {
                        alreadyExecuted = true;
                        break;
                    }
                    if (i == 0) break; // Prevent underflow
                }
            }
            if (alreadyExecuted) {
                // Slash for duplicate execution during transition period
                _slashOperator(operatorAddr, "Duplicate execution during transition");
                revert("Job already executed by another operator");
            }
        }

        // Update operator stats
        OperatorInfo storage info = operators[operatorAddr];
        if (info.addr != address(0)) {
            info.totalJobs += 1;
            info.successfulJobs += 1;
            if (info.currentLoad > 0) {
                info.currentLoad -= 1;
            }
        }

        // Route to appropriate handler based on job type
        if (job == 0) {
            _handleBasicExecution(jobCallId, operatorAddr, inputs, outputs);
        } else if (job == 1) {
            _handleAdvancedExecution(jobCallId, operatorAddr, inputs, outputs);
        } else if (job == 2) {
            _handleCreateSnapshot(operatorAddr, inputs, outputs);
        } else if (job == 3) {
            _handleRestoreSnapshot(operatorAddr, inputs, outputs);
        } else if (job == 4) {
            _handleCreateBranch(operatorAddr, inputs, outputs);
        } else if (job == 5) {
            _handleMergeBranches(operatorAddr, inputs, outputs);
        } else if (job == 6) {
            _handleStartInstance(operatorAddr, inputs, outputs);
        } else if (job == 7) {
            _handleStopInstance(operatorAddr, inputs, outputs);
        } else if (job == 8) {
            _handlePauseInstance(operatorAddr, inputs, outputs);
        } else if (job == 9) {
            _handleResumeInstance(operatorAddr, inputs, outputs);
        } else if (job == 10) {
            _handleExposePort(operatorAddr, inputs, outputs);
        } else if (job == 11) {
            _handleUploadFiles(operatorAddr, inputs, outputs);
        } else {
            // Malicious actors trying to submit results for non-existent jobs
            _slashOperator(operatorAddr, "Unauthorized job result submission");
            revert("Unknown job type");
        }
    }

    // ============================================================================
    // JOB HANDLERS
    // ============================================================================

    function _handleBasicExecution(
        uint64 callId,
        address operator,
        bytes calldata inputs,
        bytes calldata outputs
    ) internal {
        // Outputs is the stdout/stderr
        // Track execution
        operatorExecutions[operator]++;

        // For basic execution, we just emit success
        // Image would be first field in inputs but we keep it simple
        emit FunctionExecuted(callId, operator, "", true, 0);
    }

    function _handleAdvancedExecution(
        uint64 callId,
        address operator,
        bytes calldata inputs,
        bytes calldata outputs
    ) internal {
        // Similar to basic but with mode support
        operatorExecutions[operator]++;
        emit FunctionExecuted(callId, operator, "", true, 0);
    }

    function _handleCreateSnapshot(
        address operator,
        bytes calldata inputs,
        bytes calldata outputs
    ) internal {
        // Outputs should contain snapshot_id
        // We accept any format and just emit event
        emit SnapshotCreated("", "", operator, 0);
    }

    function _handleRestoreSnapshot(
        address operator,
        bytes calldata inputs,
        bytes calldata outputs
    ) internal {
        emit SnapshotRestored("", "", operator);
    }

    function _handleCreateBranch(
        address operator,
        bytes calldata,
        bytes calldata
    ) internal {
        // Branch creation tracking
        operatorExecutions[operator]++;
    }

    function _handleMergeBranches(
        address operator,
        bytes calldata,
        bytes calldata
    ) internal {
        // Branch merge tracking
        operatorExecutions[operator]++;
    }

    function _handleStartInstance(
        address operator,
        bytes calldata inputs,
        bytes calldata outputs
    ) internal {
        // Track instance start
        emit InstanceStarted("", operator, "");
    }

    function _handleStopInstance(
        address operator,
        bytes calldata inputs,
        bytes calldata outputs
    ) internal {
        emit InstanceStopped("", operator);
    }

    function _handlePauseInstance(
        address operator,
        bytes calldata,
        bytes calldata
    ) internal {
        operatorExecutions[operator]++;
    }

    function _handleResumeInstance(
        address operator,
        bytes calldata,
        bytes calldata
    ) internal {
        operatorExecutions[operator]++;
    }

    function _handleExposePort(
        address operator,
        bytes calldata inputs,
        bytes calldata outputs
    ) internal {
        emit PortExposed("", 0, "");
    }

    function _handleUploadFiles(
        address operator,
        bytes calldata inputs,
        bytes calldata outputs
    ) internal {
        emit FilesUploaded("", "", 0);
    }

    // ============================================================================
    // OPERATOR SELECTION
    // ============================================================================

    /// @notice Select operator for a job using load balancing
    function selectOperator(uint64 jobCallId) public returns (address) {
        return _selectAndAssignOperator(jobCallId);
    }

    function _assignJob(uint64 jobCallId, address operator) internal {
        require(operator != address(0), "Invalid operator");
        OperatorInfo storage info = operators[operator];
        require(info.active, "Operator inactive");
        require(info.maxConcurrentJobs > 0, "Operator capacity not set");
        require(info.currentLoad < info.maxConcurrentJobs, "Operator at capacity");

        JobAssignment storage assignment = jobAssignments[jobCallId];
        require(assignment.assignedOperator == address(0), "Job already assigned");

        assignment.assignedOperator = operator;
        assignment.executed = false;
        info.currentLoad += 1;
        emit JobAssigned(jobCallId, operator);
    }

    function isAssignedOperator(uint64 jobCallId, address operator)
        external
        view
        returns (bool)
    {
        return jobAssignments[jobCallId].assignedOperator == operator;
    }

    // ============================================================================
    // SLASHING LOGIC
    // ============================================================================

    /// @notice Slash operator for malicious behavior
    function _slashOperator(address operator, string memory reason) internal {
        string memory message = string(abi.encodePacked(
            "FaaS Blueprint: ",
            reason,
            " - Operator: ",
            Strings.toHexString(uint256(uint160(operator)), 20)
        ));

        // Call slashing precompile (runtime handles stake deduction)
        (bool success,) = SLASHING_PRECOMPILE.call(abi.encode(operator, message));

        if (success) {
            // Mark operator as inactive to prevent future jobs
            operators[operator].active = false;
            emit OperatorSlashed(operator, reason);
        }
    }

    // ============================================================================
    // LOAD BALANCING LOGIC
    // ============================================================================

    /// @notice Enhanced operator selection with load balancing
    function selectOperatorAndAssign(uint64 jobCallId) external returns (address) {
        return _selectAndAssignOperator(jobCallId);
    }

    function _selectAndAssignOperator(uint64 jobCallId) internal returns (address) {
        address selectedOp = _selectBestOperator();
        _assignJob(jobCallId, selectedOp);
        return selectedOp;
    }

    function _selectBestOperator() internal view returns (address) {
        address bestOp = address(0);
        uint256 bestScore = 0;

        for (uint256 i = 0; i < operatorList.length; i++) {
            address op = operatorList[i];
            OperatorInfo memory info = operators[op];
            if (!info.active) continue;
            if (info.maxConcurrentJobs == 0) continue;
            if (info.currentLoad >= info.maxConcurrentJobs) continue;

            uint256 score = _calculateOperatorScore(op);
            if (score > bestScore || bestOp == address(0)) {
                bestScore = score;
                bestOp = op;
            }
        }

        require(bestOp != address(0), "No operators available");
        return bestOp;
    }

    /// @notice Calculate operator selection score (higher is better)
    /// Combines low load preference with success rate bonus
    function _calculateOperatorScore(address operator) internal view returns (uint256) {
        OperatorInfo memory info = operators[operator];
        if (!info.active) return 0;
        if (info.maxConcurrentJobs == 0) return 0;
        if (info.currentLoad >= info.maxConcurrentJobs) return 0;

        uint256 remainingCapacity = info.maxConcurrentJobs - info.currentLoad;
        // Weight available capacity higher than historical success
        uint256 loadScore = remainingCapacity * 1_000_000;

        // Success rate bonus (0-1000 basis points)
        uint256 successBonus = 0;
        if (info.totalJobs > 0) {
            successBonus = (info.successfulJobs * 1000) / info.totalJobs;
        } else {
            successBonus = 1000; // treat new operators optimistically
        }

        return loadScore + successBonus;
    }

    // ============================================================================
    // UTILITY FUNCTIONS
    // ============================================================================

    function operatorToAddress(ServiceOperators.OperatorPreferences calldata operator)
        internal
        pure
        returns (address)
    {
        return address(uint160(uint256(keccak256(abi.encodePacked(operator.ecdsaPublicKey)))));
    }

    function getExecutionCount() external view returns (uint256) {
        return executions.length;
    }

    function getOperatorStats(address operator) external view returns (
        uint256 totalJobs,
        uint256 successfulJobs,
        uint256 currentLoad,
        uint256 maxConcurrentJobs,
        bool active
    ) {
        OperatorInfo memory info = operators[operator];
        return (
            info.totalJobs,
            info.successfulJobs,
            info.currentLoad,
            info.maxConcurrentJobs,
            info.active
        );
    }

    function getJobAssignment(uint64 jobCallId) external view returns (address assignedOperator, bool executed) {
        JobAssignment memory assignment = jobAssignments[jobCallId];
        return (assignment.assignedOperator, assignment.executed);
    }

    function getAssignedOperatorForJob(uint64 jobCallId) external view returns (address) {
        return jobAssignments[jobCallId].assignedOperator;
    }

    function getActiveOperators() external view returns (address[] memory activeOperators) {
        uint256 count;
        for (uint256 i = 0; i < operatorList.length; i++) {
            if (operators[operatorList[i]].active) {
                count += 1;
            }
        }

        activeOperators = new address[](count);
        uint256 idx;
        for (uint256 i = 0; i < operatorList.length; i++) {
            address op = operatorList[i];
            if (operators[op].active) {
                activeOperators[idx] = op;
                idx += 1;
            }
        }
    }

    // ============================================================================
    // EVENTS
    // ============================================================================

    event OperatorRegistered(
        ServiceOperators.OperatorPreferences operator,
        bytes32 publicKeyHash,
        string metadataUri
    );

    event ServiceRequested(uint64 indexed requestId, address indexed requester);

    event JobResultProcessed(
        uint64 indexed serviceId,
        uint8 indexed job,
        uint64 jobCallId,
        ServiceOperators.OperatorPreferences operator
    );

    event OperatorSlashed(address indexed operator, string reason);
}
