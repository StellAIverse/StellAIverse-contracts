// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/access/AccessControl.sol";
import "./SecurityEvents.sol";

contract RoleManager is AccessControl, SecurityEvents {
    bytes32 public constant ADMIN_ROLE = keccak256("ADMIN_ROLE");

    constructor(address admin) {
        _grantRole(DEFAULT_ADMIN_ROLE, admin);
    }

    function grantAdminRole(address user) external onlyRole(DEFAULT_ADMIN_ROLE) {
        grantRole(ADMIN_ROLE, user);

        emit RoleUpdated(
            msg.sender,
            user,
            ADMIN_ROLE,
            true,
            block.timestamp
        );
    }

    function revokeAdminRole(address user) external onlyRole(DEFAULT_ADMIN_ROLE) {
        revokeRole(ADMIN_ROLE, user);

        emit RoleUpdated(
            msg.sender,
            user,
            ADMIN_ROLE,
            false,
            block.timestamp
        );
    }
}