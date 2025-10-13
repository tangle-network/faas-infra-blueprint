// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.20;

import "tnt-core/BlueprintServiceManagerBase.sol";

/**
 * @title FaaSBlueprint
 * @author Tangle Network
 * @notice Comprehensive blueprint for Function-as-a-Service with multi-agent orchestration
 *
 * @dev This contract manages all FaaS platform capabilities:
 * - Container execution (ephemeral, cached, checkpointed, branched, persistent)
 * - Snapshot management (CRIU-based checkpointing)
 * - Instance lifecycle (start, stop, pause, resume)
 * - Port exposure and networking
 * - File operations
 * - Optional: ZK proof generation showcase
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
contract FaaSBlueprint is BlueprintServiceManagerBase {
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

    /// @notice Constructor
    constructor() BlueprintServiceManagerBase() {}

    /**
     * @dev Hook for service operator registration
     */
    function onRegister(
        ServiceOperators.OperatorPreferences calldata operator,
        bytes calldata
    )
        external
        payable
        virtual
        override
        onlyFromMaster
    {
        address operatorAddr = operatorToAddress(operator);
        if (!operators[operatorAddr].active) {
            operatorList.push(operatorAddr);
            operators[operatorAddr] = OperatorInfo({
                addr: operatorAddr,
                totalJobs: 0,
                successfulJobs: 0,
                currentLoad: 0,
                active: true
            });
        }

        emit OperatorRegistered(operator, bytes32(0), "");
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
    {
        address operatorAddr = operatorToAddress(operator);

        // Validate operator assignment if job was assigned
        if (jobAssignments[jobCallId].assignedOperator != address(0)) {
            require(
                jobAssignments[jobCallId].assignedOperator == operatorAddr,
                "Operator not assigned to this job"
            );
            require(!jobAssignments[jobCallId].executed, "Job already executed");
            jobAssignments[jobCallId].executed = true;
        }

        // Update operator stats
        if (operators[operatorAddr].active) {
            operators[operatorAddr].totalJobs += 1;
            operators[operatorAddr].successfulJobs += 1;
            if (operators[operatorAddr].currentLoad > 0) {
                operators[operatorAddr].currentLoad -= 1;
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
            // Accept unknown job types for future extensibility
            // Just track in operator stats
            emit JobResultProcessed(serviceId, job, jobCallId, operator);
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
        require(operatorList.length > 0, "No operators available");

        address bestOp = operatorList[0];
        uint256 lowestLoad = operators[bestOp].currentLoad;

        for (uint256 i = 1; i < operatorList.length; i++) {
            address op = operatorList[i];
            if (!operators[op].active) continue;

            if (operators[op].currentLoad < lowestLoad) {
                lowestLoad = operators[op].currentLoad;
                bestOp = op;
            }
        }

        _assignJob(jobCallId, bestOp);
        return bestOp;
    }

    function _assignJob(uint64 jobCallId, address operator) internal {
        jobAssignments[jobCallId] = JobAssignment({
            assignedOperator: operator,
            executed: false
        });
        operators[operator].currentLoad += 1;
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
        bool active
    ) {
        OperatorInfo memory info = operators[operator];
        return (info.totalJobs, info.successfulJobs, info.currentLoad, info.active);
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
}
