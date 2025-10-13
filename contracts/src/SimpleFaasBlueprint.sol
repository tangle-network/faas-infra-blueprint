// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.20;

import "tnt-core/BlueprintServiceManagerBase.sol";

/**
 * @title SimpleFaasBlueprint
 * @notice Minimal blueprint that accepts all job results without validation
 */
contract SimpleFaasBlueprint is BlueprintServiceManagerBase {
    constructor() BlueprintServiceManagerBase() {}

    /// @notice Accept all registrations
    function onRegister(
        ServiceOperators.OperatorPreferences calldata,
        bytes calldata
    ) external payable virtual override onlyFromMaster {}

    /// @notice Accept all service requests
    function onRequest(
        ServiceOperators.RequestParams calldata
    ) external payable virtual override onlyFromMaster {}

    /// @notice Accept ALL job results without validation
    function onJobResult(
        uint64,
        uint8,
        uint64,
        ServiceOperators.OperatorPreferences calldata,
        bytes calldata,
        bytes calldata
    ) external payable virtual override onlyFromMaster {
        // No validation - accept everything
    }
}
