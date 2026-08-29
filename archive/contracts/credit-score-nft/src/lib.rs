#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, BytesN, Env, Map, String, Symbol, Vec,
};
mod test;

use stellai_lib::{
    admin,
    audit::{create_audit_log, OperationType},
    errors::ContractError,
    storage_keys::ADMIN_KEY,
    types::RoyaltyInfo,
    validation, ADMIN_KEY,
};

// ============================================================================
// Event types
// ============================================================================
#[contracttype]
#[derive(Clone)]
pub enum CreditScoreEvent {
    CreditScoreNFTMinted,
    CreditScoreUpdated,
    CreditScoreVerified,
    CreditScoreTransferred,
    MetadataUpdated,
}

// ============================================================================
// Credit Score NFT Data Structure
// ============================================================================
#[contracttype]
#[derive(Clone, Debug)]
pub struct CreditScoreNFT {
    pub token_id: u64,
    pub owner: Address,
    pub credit_score: u32, // 300-850 range
    pub score_type: ScoreType,
    pub verification_status: VerificationStatus,
    pub issued_at: u64,
    pub expires_at: u64,
    pub metadata_cid: String,
    pub verification_data: VerificationData,
    pub royalty_info: Option<RoyaltyInfo>,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ScoreType {
    FICO = 0,
    VantageScore = 1,
    Experian = 2,
    Equifax = 3,
    TransUnion = 4,
    Custom = 5,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum VerificationStatus {
    Pending = 0,
    Verified = 1,
    Rejected = 2,
    Expired = 3,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct VerificationData {
    pub verification_method: String,
    pub verified_by: Address,
    pub verification_timestamp: u64,
    pub verification_hash: BytesN<32>,
    pub external_reference: String,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct CreditScoreMetadata {
    pub name: String,
    pub description: String,
    pub image: String,
    pub external_url: String,
    pub attributes: Vec<ScoreAttribute>,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct ScoreAttribute {
    pub trait_type: String,
    pub value: String,
    pub display_type: Option<String>,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MintRequest {
    pub owner: Address,
    pub credit_score: u32,
    pub score_type: ScoreType,
    pub metadata: CreditScoreMetadata,
    pub verification_data: VerificationData,
    pub expires_at: u64,
    pub royalty_info: Option<RoyaltyInfo>,
}

#[contract]
pub struct CreditScoreNFT;
#[contractimpl]
impl CreditScoreNFT {
    /// Initialize contract with admin (one-time setup)
    pub fn init_contract(env: Env, admin: Address) -> Result<(), ContractError> {
        // Security: Verify this is first initialization
        let admin_data = env
            .storage()
            .instance()
            .get::<_, Address>(&Symbol::new(&env, ADMIN_KEY));
        if admin_data.is_some() {
            return Err(ContractError::AlreadyInitialized);
        }

        admin.require_auth();
        env.storage()
            .instance()
            .set(&Symbol::new(&env, ADMIN_KEY), &admin);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "token_counter"), &0u64);

        // Initialize verification authorities list
        let verification_authorities: Vec<Address> = Vec::new(&env);
        env.storage().instance().set(
            &Symbol::new(&env, "verification_authorities"),
            &verification_authorities,
        );

        Ok(())
    }

    /// Add a verification authority (admin only)
    pub fn add_verification_authority(
        env: Env,
        admin: Address,
        authority: Address,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        Self::verify_admin(&env, &admin)?;

        let mut authorities: Vec<Address> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "verification_authorities"))
            .unwrap_or_else(|| Vec::new(&env));

        // Check if authority already exists
        for existing in authorities.iter() {
            if existing == authority {
                return Err(ContractError::AlreadyExists);
            }
        }

        authorities.push_back(authority);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "verification_authorities"), &authorities);

        // Create audit log
        let before_state = String::from_str(&env, "{}");
        let after_state = String::from_str(&env, "{\"authority_added\":true}");
        let tx_hash = String::from_str(&env, "0x_verification_authority_added");
        let description = Some(String::from_str(&env, "Verification authority added"));

        create_audit_log(
            &env,
            admin,
            OperationType::AdminAddMinter,
            before_state,
            after_state,
            tx_hash,
            description,
        );

        Ok(())
    }

    /// Mint a new credit score NFT
    pub fn mint_credit_score_nft(
        env: Env,
        minter: Address,
        request: MintRequest,
    ) -> Result<u64, ContractError> {
        minter.require_auth();

        // Validate credit score range (300-850)
        if request.credit_score < 300 || request.credit_score > 850 {
            return Err(ContractError::InvalidInput);
        }

        // Validate expiration date
        let current_time = env.ledger().timestamp();
        if request.expires_at <= current_time {
            return Err(ContractError::InvalidInput);
        }

        // Generate new token ID
        let token_id = Self::increment_token_counter(&env);

        // Create NFT
        let nft = CreditScoreNFT {
            token_id,
            owner: request.owner.clone(),
            credit_score: request.credit_score,
            score_type: request.score_type.clone(),
            verification_status: VerificationStatus::Pending,
            issued_at: current_time,
            expires_at: request.expires_at,
            metadata_cid: request.metadata.image.clone(),
            verification_data: request.verification_data.clone(),
            royalty_info: request.royalty_info.clone(),
        };

        // Store NFT
        Self::store_nft(&env, token_id, &nft);

        // Store metadata
        Self::store_metadata(&env, token_id, &request.metadata);

        // Create audit log
        let before_state = String::from_str(&env, "{}");
        let after_state = String::from_str(
            &env,
            &format!(
                "{{\"token_id\":{},\"owner\":\"{:?}\",\"score\":{}}}",
                token_id, request.owner, request.credit_score
            ),
        );
        let tx_hash = String::from_str(&env, "0x_credit_score_minted");
        let description = Some(String::from_str(&env, "Credit score NFT minted"));

        create_audit_log(
            &env,
            minter,
            OperationType::AdminMint,
            before_state,
            after_state,
            tx_hash,
            description,
        );

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "CreditScoreNFTMinted"),),
            (
                token_id,
                request.owner,
                request.credit_score,
                request.score_type,
            ),
        );

        Ok(token_id)
    }

    /// Verify a credit score NFT (verification authority only)
    pub fn verify_credit_score(
        env: Env,
        verifier: Address,
        token_id: u64,
        verification_hash: BytesN<32>,
    ) -> Result<(), ContractError> {
        verifier.require_auth();

        // Verify caller is authorized
        Self::verify_verification_authority(&env, &verifier)?;

        let mut nft = Self::_get_nft(&env, token_id)?;

        // Update verification status
        nft.verification_status = VerificationStatus::Verified;
        nft.verification_data.verification_timestamp = env.ledger().timestamp();
        nft.verification_data.verification_hash = verification_hash;

        // Store updated NFT
        Self::store_nft(&env, token_id, &nft);

        // Create audit log
        let before_state = String::from_str(&env, "{\"verified\":false}");
        let after_state = String::from_str(&env, "{\"verified\":true}");
        let tx_hash = String::from_str(&env, "0x_credit_score_verified");
        let description = Some(String::from_str(&env, "Credit score NFT verified"));

        create_audit_log(
            &env,
            verifier,
            OperationType::AdminApprove,
            before_state,
            after_state,
            tx_hash,
            description,
        );

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "CreditScoreVerified"),),
            (token_id, verifier, verification_hash),
        );

        Ok(())
    }

    /// Update credit score (verification authority only)
    pub fn update_credit_score(
        env: Env,
        verifier: Address,
        token_id: u64,
        new_score: u32,
        new_expires_at: u64,
    ) -> Result<(), ContractError> {
        verifier.require_auth();

        // Verify caller is authorized
        Self::verify_verification_authority(&env, &verifier)?;

        // Validate new score
        if new_score < 300 || new_score > 850 {
            return Err(ContractError::InvalidInput);
        }

        let mut nft = Self::_get_nft(&env, token_id)?;
        let old_score = nft.credit_score;

        // Update score and expiration
        nft.credit_score = new_score;
        nft.expires_at = new_expires_at;
        nft.verification_status = VerificationStatus::Verified;
        nft.verification_data.verification_timestamp = env.ledger().timestamp();

        // Store updated NFT
        Self::store_nft(&env, token_id, &nft);

        // Create audit log
        let before_state = String::from_str(&env, &format!("{{\"score\":{}}}", old_score));
        let after_state = String::from_str(&env, &format!("{{\"score\":{}}}", new_score));
        let tx_hash = String::from_str(&env, "0x_credit_score_updated");
        let description = Some(String::from_str(&env, "Credit score updated"));

        create_audit_log(
            &env,
            verifier,
            OperationType::ParameterUpdate,
            before_state,
            after_state,
            tx_hash,
            description,
        );

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "CreditScoreUpdated"),),
            (token_id, old_score, new_score),
        );

        Ok(())
    }

    /// Transfer NFT ownership
    pub fn transfer_nft(
        env: Env,
        from: Address,
        to: Address,
        token_id: u64,
    ) -> Result<(), ContractError> {
        from.require_auth();

        let mut nft = Self::_get_nft(&env, token_id)?;

        // Verify ownership
        if nft.owner != from {
            return Err(ContractError::Unauthorized);
        }

        // Update ownership
        nft.owner = to.clone();

        // Store updated NFT
        Self::store_nft(&env, token_id, &nft);

        // Create audit log
        let before_state = String::from_str(&env, &format!("{{\"owner\":\"{:?}\"}}", from));
        let after_state = String::from_str(&env, &format!("{{\"owner\":\"{:?}\"}}", to));
        let tx_hash = String::from_str(&env, "0x_nft_transferred");
        let description = Some(String::from_str(&env, "NFT transferred"));

        create_audit_log(
            &env,
            from,
            OperationType::AdminTransfer,
            before_state,
            after_state,
            tx_hash,
            description,
        );

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "CreditScoreTransferred"),),
            (token_id, from, to),
        );

        Ok(())
    }

    /// Get NFT by token ID
    pub fn get_nft(env: Env, token_id: u64) -> Result<CreditScoreNFT, ContractError> {
        Self::_get_nft(&env, token_id)
    }

    /// Get NFT metadata
    pub fn get_metadata(env: Env, token_id: u64) -> Result<CreditScoreMetadata, ContractError> {
        Self::_get_metadata(&env, token_id)
    }

    /// Get NFT owner
    pub fn owner_of(env: Env, token_id: u64) -> Result<Address, ContractError> {
        let nft = Self::_get_nft(&env, token_id)?;
        Ok(nft.owner)
    }

    /// Check if NFT is verified
    pub fn is_verified(env: Env, token_id: u64) -> Result<bool, ContractError> {
        let nft = Self::_get_nft(&env, token_id)?;
        Ok(nft.verification_status == VerificationStatus::Verified)
    }

    /// Get all NFTs owned by an address
    pub fn get_nfts_by_owner(env: Env, owner: Address) -> Result<Vec<u64>, ContractError> {
        let counter = Self::get_token_counter(&env);
        let mut owned_tokens = Vec::new(&env);

        for token_id in 1..=counter {
            if let Ok(nft) = Self::_get_nft(&env, token_id) {
                if nft.owner == owner {
                    owned_tokens.push_back(token_id);
                }
            }
        }

        Ok(owned_tokens)
    }

    /// Get verification authorities
    pub fn get_verification_authorities(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "verification_authorities"))
            .unwrap_or_else(|| Vec::new(&env))
    }

    // ==================== INTERNAL FUNCTIONS ====================

    fn verify_admin(env: &Env, admin: &Address) -> Result<(), ContractError> {
        admin::verify_admin(env, admin).map_err(|_| ContractError::Unauthorized)
    }

    fn verify_verification_authority(env: &Env, authority: &Address) -> Result<(), ContractError> {
        let authorities: Vec<Address> = env
            .storage()
            .instance()
            .get(&Symbol::new(env, "verification_authorities"))
            .unwrap_or_else(|| Vec::new(env));

        for auth in authorities.iter() {
            if &auth == authority {
                return Ok(());
            }
        }

        Err(ContractError::Unauthorized)
    }

    fn increment_token_counter(env: &Env) -> u64 {
        let counter = Self::get_token_counter(env) + 1;
        env.storage()
            .instance()
            .set(&Symbol::new(env, "token_counter"), &counter);
        counter
    }

    fn get_token_counter(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&Symbol::new(env, "token_counter"))
            .unwrap_or(0)
    }

    fn store_nft(env: &Env, token_id: u64, nft: &CreditScoreNFT) {
        env.storage()
            .instance()
            .set(&Symbol::new(env, &format!("nft_{}", token_id)), nft);
    }

    fn _get_nft(env: &Env, token_id: u64) -> Result<CreditScoreNFT, ContractError> {
        env.storage()
            .instance()
            .get(&Symbol::new(env, &format!("nft_{}", token_id)))
            .ok_or(ContractError::NotFound)
    }

    fn store_metadata(env: &Env, token_id: u64, metadata: &CreditScoreMetadata) {
        env.storage().instance().set(
            &Symbol::new(env, &format!("metadata_{}", token_id)),
            metadata,
        );
    }

    fn _get_metadata(env: &Env, token_id: u64) -> Result<CreditScoreMetadata, ContractError> {
        env.storage()
            .instance()
            .get(&Symbol::new(env, &format!("metadata_{}", token_id)))
            .ok_or(ContractError::NotFound)
    }
}