// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import "@openzeppelin/contracts/token/ERC721/extensions/ERC721URIStorage.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

/**
 * @title SyntheticAgent
 * @dev Synthetic representation of a StellAIverse agent on non-Stellar chains.
 * Mints and burns are controlled by the cross-chain bridge manager.
 */
contract SyntheticAgent is ERC721, ERC721URIStorage, Ownable {
    
    // Mapping from synthetic token ID to original Stellar Agent ID
    mapping(uint256 => uint256) public stellarAgentIds;
    
    // Bridge relayer address (can mint/burn)
    address public bridgeRelayer;

    event AgentMinted(uint256 indexed syntheticId, uint256 indexed stellarId, address indexed owner);
    event AgentBurned(uint256 indexed syntheticId, uint256 indexed stellarId);

    constructor(string memory name, string memory symbol, address _bridgeRelayer) 
        ERC721(name, symbol) 
        Ownable(msg.sender)
    {
        bridgeRelayer = _bridgeRelayer;
    }

    modifier onlyBridge() {
        require(msg.sender == bridgeRelayer || msg.sender == owner(), "Not authorized bridge");
        _;
    }

    function setBridgeRelayer(address _newRelayer) external onlyOwner {
        bridgeRelayer = _newRelayer;
    }

    /**
     * @dev Mint a synthetic agent when it is bridged from Stellar.
     */
    function mint(address to, uint256 syntheticId, uint256 stellarId, string memory metadataUri) 
        external 
        onlyBridge 
    {
        _safeMint(to, syntheticId);
        _setTokenURI(syntheticId, metadataUri);
        stellarAgentIds[syntheticId] = stellarId;
        
        emit AgentMinted(syntheticId, stellarId, to);
    }

    /**
     * @dev Burn a synthetic agent to bridge it back to Stellar.
     */
    function burn(uint256 syntheticId) external {
        require(ownerOf(syntheticId) == msg.sender || msg.sender == bridgeRelayer, "Not owner nor bridge");
        
        uint256 stellarId = stellarAgentIds[syntheticId];
        _burn(syntheticId);
        delete stellarAgentIds[syntheticId];
        
        emit AgentBurned(syntheticId, stellarId);
    }

    // Overrides required by Solidity
    function tokenURI(uint256 tokenId)
        public
        view
        override(ERC721, ERC721URIStorage)
        returns (string memory)
    {
        return super.tokenURI(tokenId);
    }

    function supportsInterface(bytes4 interfaceId)
        public
        view
        override(ERC721, ERC721URIStorage)
        returns (bool)
    {
        return super.supportsInterface(interfaceId);
    }
}
