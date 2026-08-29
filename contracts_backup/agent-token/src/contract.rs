// LEGACY CONTRACT (DEPRECATED)
// This file is retained for historical reference only.
// Do not use `contracts_backup/*` for active development or deployment.
#![no_std]
extern crate alloc;
use alloc::format;
use soroban_sdk::{contract, contractimpl, contracterror, contracttype, Symbol, Address, String, Env, Vec};

const ADMIN_KEY: &str = "admin";
const AGENT_COUNTER_KEY: &str = "agent_counter";
const AGENT_KEY_PREFIX: &str = "agent_";
const AGENT_LEASE_STATUS_PREFIX: &str = "agent_lease_";
const APPROVED_MINTERS_KEY: &str = "approved_minters";

// Maximum lengths for validation
const MAX_STRING_LENGTH: usize = 256;
const MAX_CAPABILITIES: usize = 10;

// ============================================================================
// Contract Error Enum
// ============================================================================
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    AlreadyInitialized = 1,
    Unauthorized = 2,
    DuplicateAgentId = 3,
    AgentNotFound = 4,
    InvalidAgentId = 5,
    InvalidInput = 6,
    AgentLeased = 7,
    OverflowError = 8,
    SameAddressTransfer = 9,
    NotOwner = 10,
}

// ============================================================================
// Agent data structure
// ============================================================================
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Agent {
    pub id: u64,
    pub owner: Address,
    pub name: String,
    pub model_hash: String,
    pub metadata_cid: String,
    pub capabilities: Vec<String>,
    pub evolution_level: u32,
    pub created_at: u64,
    pub updated_at: u64,
    pub nonce: u64,
}

// ============================================================================
// Event types
// ============================================================================
#[contracttype]
#[derive(Clone)]
pub enum AgentEvent {
    AgentMinted,
    AgentUpdated,
    AgentTransferred,
    LeaseStarted,
    LeaseEnded,
}

#[contract]
pub struct AgentNFT;

#[contractimpl]
impl AgentNFT {
    /// Initialize contract with admin (one-time setup)
    pub fn init_contract(env: Env, admin: Address) -> Result<(), ContractError> {
        // Security: Verify this is first initialization
        let admin_data = env.storage().instance().get::<_, Address>(&Symbol::new(&env, ADMIN_KEY));
        if admin_data.is_some() {
            return Err(ContractError::AlreadyInitialized);
        }

        admin.require_auth();
        env.storage().instance().set(&Symbol::new(&env, ADMIN_KEY), &admin);
        env.storage().instance().set(&Symbol::new(&env, AGENT_COUNTER_KEY), &0u64);
        
        // Initialize approved minters list (empty by default)
        let approved_minters: Vec<Address> = Vec::new(&env);
        env.storage().instance().set(&Symbol::new(&env, APPROVED_MINTERS_KEY), &approved_minters);
        
        Ok(())
    }

    /// Add an approved minter (admin only)
    pub fn add_approved_minter(env: Env, admin: Address, minter: Address) -> Result<(), ContractError> {
        admin.require_auth();
        Self::verify_admin(&env, &admin)?;

        let mut approved_minters: Vec<Address> = env.storage()
            .instance()
            .get(&Symbol::new(&env, APPROVED_MINTERS_KEY))
            .unwrap_or_else(|| Vec::new(&env));

        approved_minters.push_back(minter);
        env.storage().instance().set(&Symbol::new(&env, APPROVED_MINTERS_KEY), &approved_minters);

        Ok(())
    }

    /// Helper to get storage key for an agent
    fn get_agent_key(env: &Env, agent_id: u64) -> String {
        String::from_str(env, &format!("{}{}", AGENT_KEY_PREFIX, agent_id))
    }

    /// Helper to get storage key for agent lease status
    fn get_agent_lease_key(env: &Env, agent_id: u64) -> String {
        String::from_str(env, &format!("{}{}", AGENT_LEASE_STATUS_PREFIX, agent_id))
    }

    /// Verify caller is admin
    fn verify_admin(env: &Env, caller: &Address) -> Result<(), ContractError> {
        let admin: Address = env.storage()
            .instance()
            .get(&Symbol::new(env, ADMIN_KEY))
            .ok_or(ContractError::Unauthorized)?;
        
        if caller != &admin {
            return Err(ContractError::Unauthorized);
        }
        Ok(())
    }

    /// Verify caller is admin or approved minter
    fn verify_minter(env: &Env, caller: &Address) -> Result<(), ContractError> {
        // Check if admin
        if let Some(admin) = env.storage().instance().get::<_, Address>(&Symbol::new(env, ADMIN_KEY)) {
            if caller == &admin {
                return Ok(());
            }
        }

        // Check if approved minter
        let approved_minters: Vec<Address> = env.storage()
            .instance()
            .get(&Symbol::new(env, APPROVED_MINTERS_KEY))
            .unwrap_or_else(|| Vec::new(env));

        for i in 0..approved_minters.len() {
            if let Some(minter) = approved_minters.get(i) {
                if &minter == caller {
                    return Ok(());
                }
            }
        }

        Err(ContractError::Unauthorized)
    }

    /// Safe addition with overflow checks
    fn safe_add(a: u64, b: u64) -> Result<u64, ContractError> {
        a.checked_add(b).ok_or(ContractError::OverflowError)
    }

    /// Check if agent is currently leased
    fn is_agent_leased(env: &Env, agent_id: u64) -> bool {
        let lease_key = Self::get_agent_lease_key(env, agent_id);
        env.storage()
            .instance()
            .get::<_, bool>(&lease_key)
            .unwrap_or(false)
    }

    /// Set agent lease status
    fn set_agent_lease_status(env: &Env, agent_id: u64, is_leased: bool) {
        let lease_key = Self::get_agent_lease_key(env, agent_id);
        env.storage().instance().set(&lease_key, &is_leased);
    }

    /// Check if agent ID already exists
    fn agent_exists(env: &Env, agent_id: u64) -> bool {
        let key = Self::get_agent_key(env, agent_id);
        env.storage().instance().has(&key)
    }

    /// Mint a new agent NFT - Implements requirement from issue
    /// 
    /// # Arguments
    /// * `agent_id` - Unique identifier for the agent (u128 in spec, using u64 for storage efficiency)
    /// * `owner` - Address of the agent owner
    /// * `metadata_cid` - IPFS CID for agent metadata
    /// * `initial_evolution_level` - Starting evolution level
    ///
    /// # Returns
    /// Result<(), ContractError>
    ///
    /// # Errors
    /// - ContractError::Unauthorized if caller is not admin or approved minter
    /// - ContractError::DuplicateAgentId if agent_id already exists
    /// - ContractError::InvalidInput if validation fails
    pub fn mint_agent(
        env: Env,
        agent_id: u128,
        owner: Address,
        metadata_cid: String,
        initial_evolution_level: u32,
    ) -> Result<(), ContractError> {
        owner.require_auth();
        
        // Validate caller authorization (admin or approved minter)
        Self::verify_minter(&env, &owner)?;

        // Convert u128 to u64 for storage (validate it fits)
        let agent_id_u64 = agent_id.try_into()
            .map_err(|_| ContractError::InvalidInput)?;

        // Enforce uniqueness of agent_id
        if Self::agent_exists(&env, agent_id_u64) {
            return Err(ContractError::DuplicateAgentId);
        }

        // Input validation
        if metadata_cid.len() > MAX_STRING_LENGTH.try_into().unwrap() {
            return Err(ContractError::InvalidInput);
        }

        // Create agent with metadata CID and evolution level
        let agent = Agent {
            id: agent_id_u64,
            owner: owner.clone(),
            name: String::from_str(&env, ""),  // Can be set via update_agent
            model_hash: String::from_str(&env, ""),  // Can be set via update_agent
            metadata_cid,
            capabilities: Vec::new(&env),
            evolution_level: initial_evolution_level,
            created_at: env.ledger().timestamp(),
            updated_at: env.ledger().timestamp(),
            nonce: 0,
        };

        // Persist agent data
        let key = Self::get_agent_key(&env, agent_id_u64);
        env.storage().instance().set(&key, &agent);
        
        // Initialize lease status to false (not leased)
        Self::set_agent_lease_status(&env, agent_id_u64, false);

        // Emit AgentMinted event
        env.events().publish(
            (Symbol::new(&env, "agent_nft"), AgentEvent::AgentMinted),
            (agent_id_u64, owner.clone(), initial_evolution_level)
        );

        Ok(())
    }

    /// Legacy mint function for backward compatibility
    pub fn mint_agent_legacy(
        env: Env,
        owner: Address,
        name: String,
        model_hash: String,
        capabilities: Vec<String>,
    ) -> Result<u64, ContractError> {
        owner.require_auth();
        
        // Validate caller authorization
        Self::verify_minter(&env, &owner)?;

        // Input validation
        if name.len() > MAX_STRING_LENGTH.try_into().unwrap() {
            return Err(ContractError::InvalidInput);
        }
        if model_hash.len() > MAX_STRING_LENGTH.try_into().unwrap() {
            return Err(ContractError::InvalidInput);
        }
        if capabilities.len() > MAX_CAPABILITIES.try_into().unwrap() {
            return Err(ContractError::InvalidInput);
        }

        // Validate each capability string
        for i in 0..capabilities.len() {
            if let Some(cap) = capabilities.get(i) {
                if cap.len() > MAX_STRING_LENGTH.try_into().unwrap() {
                    return Err(ContractError::InvalidInput);
                }
            }
        }

        // Increment agent counter safely
        let counter: u64 = env.storage()
            .instance()
            .get(&Symbol::new(&env, AGENT_COUNTER_KEY))
            .unwrap_or(0);
        
        let agent_id = Self::safe_add(counter, 1)?;
        
        // Create agent
        let agent = Agent {
            id: agent_id,
            owner: owner.clone(),
            name,
            model_hash,
            metadata_cid: String::from_str(&env, ""),
            capabilities,
            evolution_level: 0,
            created_at: env.ledger().timestamp(),
            updated_at: env.ledger().timestamp(),
            nonce: 0,
            escrow_locked: false,
            escrow_holder: None,
        };

        // Store agent
        let key = Self::get_agent_key(&env, agent_id);
        env.storage().instance().set(&key, &agent);
        
        // Initialize lease status
        Self::set_agent_lease_status(&env, agent_id, false);
        
        // Update counter
        env.storage().instance().set(&Symbol::new(&env, AGENT_COUNTER_KEY), &agent_id);

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "agent_nft"), AgentEvent::AgentMinted),
            (agent_id, owner.clone())
        );

        Ok(agent_id)
    }

    /// Get agent metadata with bounds checking
    pub fn get_agent(env: Env, agent_id: u64) -> Result<Agent, ContractError> {
        if agent_id == 0 {
            return Err(ContractError::InvalidAgentId);
        }

        let key = Self::get_agent_key(&env, agent_id);
        env.storage()
            .instance()
            .get::<_, Agent>(&key)
            .ok_or(ContractError::AgentNotFound)
    }

    /// Update agent metadata with authorization check
    pub fn update_agent(
        env: Env,
        agent_id: u64,
        owner: Address,
        name: Option<String>,
        capabilities: Option<Vec<String>>,
    ) -> Result<(), ContractError> {
        owner.require_auth();

        if agent_id == 0 {
            return Err(ContractError::InvalidAgentId);
        }

        let key = Self::get_agent_key(&env, agent_id);
        let mut agent: Agent = env.storage()
            .instance()
            .get(&key)
            .ok_or(ContractError::AgentNotFound)?;

        // Authorization check: only owner can update
        if agent.owner != owner {
            return Err(ContractError::NotOwner);
        }

        // Check if agent is leased
        if Self::is_agent_leased(&env, agent_id) {
            return Err(ContractError::AgentLeased);
        }

        // Update fields with validation
        if let Some(new_name) = name {
            if new_name.len() > MAX_STRING_LENGTH.try_into().unwrap() {
                return Err(ContractError::InvalidInput);
            }
            agent.name = new_name;
        }

        if let Some(new_capabilities) = capabilities {
            if new_capabilities.len() > MAX_CAPABILITIES.try_into().unwrap() {
                return Err(ContractError::InvalidInput);
            }
            for i in 0..new_capabilities.len() {
                if let Some(cap) = new_capabilities.get(i) {
                    if cap.len() > MAX_STRING_LENGTH.try_into().unwrap() {
                        return Err(ContractError::InvalidInput);
                    }
                }
            }
            agent.capabilities = new_capabilities;
        }

        // Increment nonce for replay protection
        agent.nonce = agent.nonce.checked_add(1).ok_or(ContractError::OverflowError)?;
        agent.updated_at = env.ledger().timestamp();

        env.storage().instance().set(&key, &agent);
        
        env.events().publish(
            (Symbol::new(&env, "agent_nft"), AgentEvent::AgentUpdated),
            (agent_id, owner)
        );

        Ok(())
    }

    /// Get total agents minted
    pub fn total_agents(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, AGENT_COUNTER_KEY))
            .unwrap_or(0)
    }

    /// Get nonce for replay protection
    pub fn get_nonce(env: Env, agent_id: u64) -> Result<u64, ContractError> {
        if agent_id == 0 {
            return Err(ContractError::InvalidAgentId);
        }

        let key = Self::get_agent_key(&env, agent_id);
        env.storage()
            .instance()
            .get::<_, Agent>(&key)
            .map(|agent| agent.nonce)
            .ok_or(ContractError::AgentNotFound)
    }

    /// Transfer ownership of an Agent NFT
    pub fn transfer_agent(
        env: Env,
        agent_id: u64,
        from: Address,
        to: Address,
    ) -> Result<(), ContractError> {
        if agent_id == 0 {
            return Err(ContractError::InvalidAgentId);
        }

        from.require_auth();

        if from == to {
            return Err(ContractError::SameAddressTransfer);
        }

        let key = Self::get_agent_key(&env, agent_id);
        let mut agent: Agent = env.storage()
            .instance()
            .get(&key)
            .ok_or(ContractError::AgentNotFound)?;

        if agent.owner != from {
            return Err(ContractError::NotOwner);
        }

        if Self::is_agent_leased(&env, agent_id) {
            return Err(ContractError::AgentLeased);
        }

        let previous_owner = agent.owner.clone();
        agent.owner = to.clone();
        agent.nonce = agent.nonce.checked_add(1).ok_or(ContractError::OverflowError)?;
        agent.updated_at = env.ledger().timestamp();

        env.storage().instance().set(&key, &agent);

        env.events().publish(
            (Symbol::new(&env, "agent_nft"), AgentEvent::AgentTransferred),
            (agent_id, previous_owner, to.clone())
        );

        Ok(())
    }

    /// Get current owner of an agent
    pub fn get_agent_owner(env: Env, agent_id: u64) -> Result<Address, ContractError> {
        if agent_id == 0 {
            return Err(ContractError::InvalidAgentId);
        }

        let key = Self::get_agent_key(&env, agent_id);
        env.storage()
            .instance()
            .get::<_, Agent>(&key)
            .map(|agent| agent.owner)
            .ok_or(ContractError::AgentNotFound)
    }

    /// Check if agent can be transferred
    pub fn can_transfer_agent(env: Env, agent_id: u64, caller: Address) -> bool {
        if agent_id == 0 {
            return false;
        }

        let key = Self::get_agent_key(&env, agent_id);
        let agent = match env.storage().instance().get::<_, Agent>(&key) {
            Some(agent) => agent,
            None => return false,
        };

        if agent.owner != caller {
            return false;
        }

        !Self::is_agent_leased(&env, agent_id)
    }

    /// Start leasing an agent
    pub fn start_lease(env: Env, agent_id: u64) -> Result<(), ContractError> {
        if agent_id == 0 {
            return Err(ContractError::InvalidAgentId);
        }

        Self::set_agent_lease_status(&env, agent_id, true);
        
        env.events().publish(
            (Symbol::new(&env, "agent_nft"), AgentEvent::LeaseStarted),
            (agent_id, env.ledger().timestamp())
        );

        Ok(())
    }

    /// End leasing an agent
    pub fn end_lease(env: Env, agent_id: u64) -> Result<(), ContractError> {
        if agent_id == 0 {
            return Err(ContractError::InvalidAgentId);
        }

        Self::set_agent_lease_status(&env, agent_id, false);
        
        env.events().publish(
            (Symbol::new(&env, "agent_nft"), AgentEvent::LeaseEnded),
            (agent_id, env.ledger().timestamp())
        );

        Ok(())
    }

    /// Check if agent is leased
    pub fn is_leased(env: Env, agent_id: u64) -> Result<bool, ContractError> {
        if agent_id == 0 {
            return Err(ContractError::InvalidAgentId);
        }
        Ok(Self::is_agent_leased(&env, agent_id))
    }
}
