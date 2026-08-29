// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

abstract contract SecurityEvents {
    event KYCUpdated(
        address indexed actor,
        address indexed target,
        bool status,
        uint256 timestamp
    );

    event RoleUpdated(
        address indexed actor,
        address indexed target,
        bytes32 role,
        bool granted,
        uint256 timestamp
    );

    event GovernanceActionExecuted(
        address indexed actor,
        bytes32 indexed proposalId,
        string action,
        uint256 timestamp
    );
}