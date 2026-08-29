// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/access/Ownable.sol";
import "./SecurityEvents.sol";

contract KYCManager is Ownable, SecurityEvents {
    mapping(address => bool) public verifiedUsers;

    constructor(address initialOwner) Ownable(initialOwner) {}

    function updateKYC(address user, bool status) external onlyOwner {
        verifiedUsers[user] = status;

        emit KYCUpdated(msg.sender, user, status, block.timestamp);
    }
}