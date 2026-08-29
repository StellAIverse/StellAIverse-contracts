// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./SecurityEvents.sol";

contract GovernanceManager is SecurityEvents {
    mapping(bytes32 => bool) public executedProposals;

    function executeProposal(
        bytes32 proposalId,
        string calldata action
    ) external {
        require(!executedProposals[proposalId], "Proposal already executed");

        executedProposals[proposalId] = true;

        emit GovernanceActionExecuted(
            msg.sender,
            proposalId,
            action,
            block.timestamp
        );
    }
}