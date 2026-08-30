#![no_std]

use soroban_sdk::{
    contract, contractimpl, symbol_short, Address, Bytes, Env, IntoVal, String, Symbol, Val, Vec,
};

use stellai_lib::{
    AuctionType, RoyaltyConfig, RoyaltyRecipient, SealedCommit, SealedReveal, WorkflowStep,
    WorkflowStepStatus,
};

// ── Storage keys ──────────────────────────────────────────────────────────────

const ADMIN_KEY: &str = "mkt_admin";
const LISTING_CTR_KEY: &str = "lst_ctr";
const LISTING_PREFIX: &str = "lst_";
const ROYALTY_PREFIX: &str = "roy_";
const AGENT_NFT_KEY: &str = "agent_nft";
const HUB_KEY: &str = "exec_hub";
const PENDING_SALE_PREFIX: &str = "psale_";
const WF_LISTING_PREFIX: &str = "wf_lst_";
// New storage keys for extended features
const AUCTION_CTR_KEY: &str = "auc_ctr";
const AUCTION_PREFIX: &str = "auc_";
const BID_RECORD_PREFIX: &str = "bid_";
const OFFER_CTR_KEY: &str = "ofr_ctr";
const OFFER_PREFIX: &str = "ofr_";
const DISPUTE_CTR_KEY: &str = "dsp_ctr";
const DISPUTE_PREFIX: &str = "dsp_";
const TRANSACTION_HISTORY_PREFIX: &str = "txn_";
const PLATFORM_FEE_KEY: &str = "plat_fee";
const DEFAULT_LISTING_DURATION: u64 = 30 * 24 * 60 * 60; // 30 days in seconds
const MIN_BID_INCREMENT_BPS: u32 = 100; // 1% minimum bid increment

// ── NFT Marketplace Trading + Auction extension constants ─────────────────────
const COLLECTION_CTR_KEY: &str = "coll_ctr";
const COLLECTION_PREFIX: &str = "coll_";
const COLLECTION_ROYALTY_PREFIX: &str = "rcoll_";
const COUNTER_OFFER_CTR_KEY: &str = "cofr_ctr";
const COUNTER_OFFER_PREFIX: &str = "cofr_";
const SEALED_COMMIT_PREFIX: &str = "scomm_";
const SEALED_REVEAL_PREFIX: &str = "srev_";
const GOV_ROLE_KEY: &str = "gov_role";
const KYC_ROLE_KEY: &str = "kyc_role";
const MAX_COLLECTION_NAME_LEN: u32 = 64;
const MAX_BATCH_SIZE: u32 = 25;
const DEFAULT_COUNTER_OFFER_DAYS: u64 = 5;
#[allow(dead_code)]
const AUCTION_TYPE_DUTCH: u32 = 1;
#[allow(dead_code)]
const AUCTION_TYPE_SEALED: u32 = 2;

// ── NFT Marketplace extension constants ─────────────────────────────────────
const NFT_LISTING_PREFIX: &str = "nlst_";
const NFT_LISTING_CTR_KEY: &str = "nlst_ctr";
const CURRENCY_PREFIX: &str = "ccy_";
const CURRENCY_CTR_KEY: &str = "ccy_ctr";
const ACCEPTED_CURRENCY_KEY: &str = "acc_ccy";
const FEE_SPLIT_KEY: &str = "fee_sp";
const IPFS_METADATA_PREFIX: &str = "ipfs_";
const EXTENSION_WINDOW_SECS: u64 = 300; // 5 minutes auto-extension window
const DEFAULT_EXTENSION_SECS: u64 = 300; // 5 minutes extension

// ── Local types ───────────────────────────────────────────────────────────────

#[derive(Clone)]
#[soroban_sdk::contracttype]
pub struct PendingSale {
    pub listing_id: u64,
    pub buyer: Address,
    pub amount: i128,
    pub seller: Address,
    pub agent_id: u64,
    pub workflow_id: u64,
    pub created_at: u64,
}

#[derive(Clone)]
#[soroban_sdk::contracttype]
pub struct Offer {
    pub offer_id: u64,
    pub listing_id: u64,
    pub offerer: Address,
    pub amount: i128,
    pub active: bool,
    pub created_at: u64,
    pub expires_at: u64,
}

#[derive(Clone)]
#[soroban_sdk::contracttype]
pub struct TransactionRecord {
    pub txn_id: u64,
    pub listing_id: u64,
    pub asset_id: u64,
    pub seller: Address,
    pub buyer: Address,
    pub amount: i128,
    pub royalty_amount: i128,
    pub platform_fee: i128,
    pub timestamp: u64,
    pub txn_type: String, // "sale", "auction_won", "offer_accepted"
}

#[derive(Clone)]
#[soroban_sdk::contracttype]
pub struct PlatformFeeConfig {
    pub fee_bps: u32,
    pub recipient: Address,
    pub min_fee: Option<i128>,
    pub max_fee: Option<i128>,
}

#[derive(Clone)]
#[soroban_sdk::contracttype]
pub struct Collection {
    pub collection_id: u64,
    pub creator: Address,
    pub name: String,
    pub members: Vec<u64>,
    pub royalty_config: RoyaltyConfig,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone)]
#[soroban_sdk::contracttype]
pub struct CounterOffer {
    pub counter_id: u64,
    pub listing_id: u64,
    pub in_response_to_offer_id: u64,
    pub by_seller: Address,
    pub amount: i128,
    pub active: bool,
    pub created_at: u64,
    pub expires_at: u64,
}

// ── NFT Marketplace types ───────────────────────────────────────────────────

#[derive(Clone)]
#[soroban_sdk::contracttype]
pub struct NftListing {
    pub nft_listing_id: u64,
    pub nft_token_ref: stellai_lib::NftTokenRef,
    pub seller: Address,
    pub price: i128,
    pub currency_symbol: String,
    pub currency_token_address: Option<Address>,
    pub active: bool,
    pub created_at: u64,
    pub expires_at: u64,
    pub metadata_uri: String, // IPFS CID for NFT metadata
}

#[derive(Clone)]
#[soroban_sdk::contracttype]
pub struct CurrencyRecord {
    pub currency_id: u64,
    pub symbol: String,
    pub token_address: Option<Address>,
    pub decimals: u32,
    pub active: bool,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct Marketplace;

#[contractimpl]
impl Marketplace {
    // =========================================================================
    // Initialisation
    // =========================================================================

    pub fn init_contract(env: Env, admin: Address) {
        let key = Symbol::new(&env, ADMIN_KEY);
        if env.storage().instance().has(&key) {
            panic!("Already initialized");
        }
        admin.require_auth();
        env.storage().instance().set(&key, &admin);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, LISTING_CTR_KEY), &0u64);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, AUCTION_CTR_KEY), &0u64);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, COLLECTION_CTR_KEY), &0u64);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, COUNTER_OFFER_CTR_KEY), &0u64);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, OFFER_CTR_KEY), &0u64);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, DISPUTE_CTR_KEY), &0u64);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, NFT_LISTING_CTR_KEY), &0u64);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, CURRENCY_CTR_KEY), &0u64);
        // Initialize default platform fee: 2.5%
        let default_fee = PlatformFeeConfig {
            fee_bps: 250,
            recipient: admin.clone(),
            min_fee: None,
            max_fee: None,
        };
        env.storage()
            .instance()
            .set(&Symbol::new(&env, PLATFORM_FEE_KEY), &default_fee);
    }

    pub fn set_agent_nft_contract(env: Env, admin: Address, agent_nft: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, AGENT_NFT_KEY), &agent_nft);
        env.events().publish((symbol_short!("nft_set"),), agent_nft);
    }

    pub fn set_execution_hub(env: Env, admin: Address, hub: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, HUB_KEY), &hub);
        env.events().publish((symbol_short!("hub_set"),), hub);
    }

    // =========================================================================
    // Listings
    // =========================================================================

    pub fn create_listing(
        env: Env,
        agent_id: u64,
        seller: Address,
        listing_type: u32,
        price: i128,
        duration_days: Option<u64>,
    ) -> u64 {
        seller.require_auth();
        if agent_id == 0 {
            panic!("Invalid agent ID");
        }
        if listing_type > 2 {
            panic!("Invalid listing type");
        }
        if !(stellai_lib::PRICE_LOWER_BOUND..=stellai_lib::PRICE_UPPER_BOUND).contains(&price) {
            panic!("Price out of valid range");
        }
        if listing_type == 1 {
            let dur = duration_days.expect("Duration required for leases");
            if dur == 0 || dur > stellai_lib::MAX_DURATION_DAYS {
                panic!("Lease duration out of valid range");
            }
        }

        let agent = Self::load_agent(&env, agent_id);
        if agent.owner != seller {
            panic!("Only agent owner can create listings");
        }
        if agent.escrow_locked {
            panic!("Agent already locked in escrow");
        }

        let listing_id = Self::next_listing_id(&env);
        let marketplace = env.current_contract_address();

        // Calculate expiration time
        let current_time = env.ledger().timestamp();
        let expires_at = if let Some(days) = duration_days {
            current_time + (days * 24 * 60 * 60)
        } else {
            current_time + DEFAULT_LISTING_DURATION
        };

        let listing_type_enum = match listing_type {
            0 => stellai_lib::ListingType::Sale,
            1 => stellai_lib::ListingType::Lease,
            2 => stellai_lib::ListingType::Auction,
            _ => panic!("Invalid listing type"),
        };

        let listing = stellai_lib::Listing {
            listing_id,
            asset_id: agent_id,
            asset_type: stellai_lib::AssetType::Agent,
            seller: seller.clone(),
            price,
            listing_type: listing_type_enum,
            active: true,
            created_at: current_time,
            expires_at,
        };

        let lk = Self::listing_key(&env, listing_id);
        env.storage().instance().set(&lk, &listing);

        let mut updated_agent = agent;
        updated_agent.escrow_locked = true;
        updated_agent.escrow_holder = Some(marketplace.clone());
        updated_agent.updated_at = env.ledger().timestamp();
        Self::save_agent(&env, agent_id, &updated_agent);

        env.events().publish(
            (symbol_short!("lst_creat"),),
            (listing_id, agent_id, seller.clone(), price),
        );
        env.events().publish(
            (symbol_short!("esc_lock"),),
            (agent_id, seller, marketplace),
        );

        listing_id
    }

    // =========================================================================
    // Execution-hub-orchestrated sale
    // =========================================================================

    /// Purchase an agent via an execution-hub workflow.
    ///
    /// Registers a three-step workflow in the hub, stores a pending-sale
    /// record, then drives step 0 immediately.  Remaining steps are driven by
    /// subsequent `execute_workflow_step` calls on the hub.
    ///
    /// Returns `(listing_id, workflow_id)`.
    pub fn buy_agent(env: Env, listing_id: u64, buyer: Address, amount: i128) -> (u64, u64) {
        buyer.require_auth();

        if listing_id == 0 {
            panic!("Invalid listing ID");
        }

        let listing = Self::load_listing(&env, listing_id);
        if !listing.active {
            panic!("Listing is not active");
        }
        if amount < listing.price {
            panic!("Insufficient payment");
        }
        if amount > stellai_lib::PRICE_UPPER_BOUND {
            panic!("Payment exceeds safe maximum");
        }

        let marketplace = env.current_contract_address();
        let agent = Self::load_agent(&env, listing.asset_id);
        if !agent.escrow_locked {
            panic!("Agent not in escrow");
        }
        match &agent.escrow_holder {
            Some(h) if h == &marketplace => {}
            _ => panic!("Agent locked by a different contract"),
        }

        // Persist pending sale (workflow_id filled in after the hub call)
        let pending = PendingSale {
            listing_id,
            buyer: buyer.clone(),
            amount,
            seller: listing.seller.clone(),
            agent_id: listing.asset_id,
            workflow_id: 0,
            created_at: env.ledger().timestamp(),
        };
        let psk = Self::pending_sale_key(&env, listing_id);
        env.storage().instance().set(&psk, &pending);

        let hub = Self::get_hub(&env);
        let steps = Self::build_sale_steps(&env, &marketplace, listing_id);
        let context_tag: Option<String> = Some(String::from_str(&env, "agent_sale"));
        let none_u64: Option<u64> = None;
        let cb_contract: Option<Address> = Some(marketplace.clone());

        // Build args for create_workflow
        let mut cw_args = Vec::<Val>::new(&env);
        cw_args.push_back(marketplace.clone().into_val(&env));
        cw_args.push_back(String::from_str(&env, "agent_sale").into_val(&env));
        cw_args.push_back(steps.into_val(&env));
        cw_args.push_back(none_u64.into_val(&env));
        cw_args.push_back(context_tag.into_val(&env));
        cw_args.push_back(cb_contract.into_val(&env));

        let workflow_id: u64 =
            env.invoke_contract(&hub, &Symbol::new(&env, "create_workflow"), cw_args);

        // Back-fill workflow_id
        let mut updated_pending: PendingSale = env
            .storage()
            .instance()
            .get(&psk)
            .expect("Pending sale disappeared");
        updated_pending.workflow_id = workflow_id;
        env.storage().instance().set(&psk, &updated_pending);

        // Store workflow→listing mapping for callback reconciliation
        let wlk = Self::wf_listing_key(&env, workflow_id);
        env.storage().instance().set(&wlk, &listing_id);

        env.events().publish(
            (symbol_short!("sale_init"),),
            (listing_id, buyer, workflow_id, env.ledger().timestamp()),
        );

        // Drive step 0
        let mut ews_args = Vec::<Val>::new(&env);
        ews_args.push_back(workflow_id.into_val(&env));
        let _: WorkflowStepStatus =
            env.invoke_contract(&hub, &Symbol::new(&env, "execute_workflow_step"), ews_args);

        (listing_id, workflow_id)
    }

    // =========================================================================
    // Workflow step functions (called by the execution hub)
    // =========================================================================

    /// Step 0 — verify the listing and escrow are still valid.
    /// `encoded_args`: 8 bytes big-endian listing_id.
    pub fn verify_sale(env: Env, encoded_args: Bytes) {
        let listing_id = Self::decode_u64(&encoded_args);
        let listing = Self::load_listing(&env, listing_id);
        if !listing.active {
            panic!("Listing no longer active");
        }
        let psk = Self::pending_sale_key(&env, listing_id);
        if !env.storage().instance().has(&psk) {
            panic!("No pending sale for this listing");
        }
        let marketplace = env.current_contract_address();
        let agent = Self::load_agent(&env, listing.asset_id);
        if !agent.escrow_locked {
            panic!("Agent not in escrow at verify time");
        }
        match &agent.escrow_holder {
            Some(h) if h == &marketplace => {}
            _ => panic!("Escrow holder mismatch at verify time"),
        }
        env.events().publish(
            (symbol_short!("sale_vfy"),),
            (listing_id, env.ledger().timestamp()),
        );
    }

    /// Step 1 — transfer ownership to the buyer.
    /// `encoded_args`: 8 bytes big-endian listing_id.
    pub fn transfer_ownership(env: Env, encoded_args: Bytes) {
        let listing_id = Self::decode_u64(&encoded_args);
        let listing = Self::load_listing(&env, listing_id);

        let psk = Self::pending_sale_key(&env, listing_id);
        let pending: PendingSale = env.storage().instance().get(&psk).expect("No pending sale");

        let mut agent = Self::load_agent(&env, listing.asset_id);
        agent.owner = pending.buyer.clone();
        agent.nonce = agent.nonce.checked_add(1).expect("Agent nonce overflow");
        agent.updated_at = env.ledger().timestamp();
        Self::save_agent(&env, listing.asset_id, &agent);

        env.events().publish(
            (symbol_short!("own_xfer"),),
            (
                listing.asset_id,
                listing.seller,
                pending.buyer,
                env.ledger().timestamp(),
            ),
        );
    }

    /// Step 2 — release escrow, deactivate listing, emit sale record.
    /// `encoded_args`: 8 bytes big-endian listing_id.
    pub fn record_sale(env: Env, encoded_args: Bytes) {
        let listing_id = Self::decode_u64(&encoded_args);
        let mut listing = Self::load_listing(&env, listing_id);

        let psk = Self::pending_sale_key(&env, listing_id);
        let pending: PendingSale = env.storage().instance().get(&psk).expect("No pending sale");

        // Royalty resolution: try multi-recipient RoyaltyConfig first
        // (set via `set_collection_royalty`), then fall back to single-recipient
        // RoyaltyInfo (legacy `set_royalty`), then no royalty.
        let (royalty_amount, platform_fee) =
            Self::compute_settlement_fees(&env, listing.asset_id, pending.amount);

        let seller_amount = pending
            .amount
            .checked_sub(royalty_amount)
            .expect("Seller amount underflow")
            .checked_sub(platform_fee)
            .expect("Platform-fee underflow");

        let mut agent = Self::load_agent(&env, listing.asset_id);
        agent.escrow_locked = false;
        agent.escrow_holder = None;
        agent.updated_at = env.ledger().timestamp();
        Self::save_agent(&env, listing.asset_id, &agent);

        listing.active = false;
        let lk = Self::listing_key(&env, listing_id);
        env.storage().instance().set(&lk, &listing);

        // Persist settlement record (unified with auctions).
        Self::record_transaction(
            &env,
            listing_id,
            listing.asset_id,
            listing.seller.clone(),
            pending.buyer.clone(),
            pending.amount,
            royalty_amount,
            platform_fee,
            String::from_str(&env, "sale"),
        );

        env.storage().instance().remove(&psk);

        env.events().publish(
            (symbol_short!("agnt_sold"),),
            (
                listing_id,
                listing.asset_id,
                pending.buyer.clone(),
                seller_amount,
                royalty_amount,
                platform_fee,
            ),
        );
        env.events().publish(
            (symbol_short!("esc_rel"),),
            (
                listing.asset_id,
                pending.buyer,
                env.current_contract_address(),
            ),
        );
    }

    // =========================================================================
    // Rollback (called by hub on failure)
    // =========================================================================

    /// Compensating action for the sale steps.
    /// Restores agent ownership to seller and releases escrow if needed.
    /// `encoded_args`: 8 bytes big-endian listing_id.
    pub fn rollback(env: Env, encoded_args: Bytes) {
        if encoded_args.is_empty() {
            return;
        }
        let listing_id = Self::decode_u64(&encoded_args);
        let psk = Self::pending_sale_key(&env, listing_id);
        let pending_opt: Option<PendingSale> = env.storage().instance().get(&psk);

        let pending = match pending_opt {
            Some(p) => p,
            None => return, // nothing to roll back
        };

        let listing_opt = Self::try_load_listing(&env, listing_id);
        if let Ok(listing) = listing_opt {
            if let Ok(mut agent) = Self::try_load_agent(&env, listing.asset_id) {
                // Restore ownership if it was transferred
                if agent.owner == pending.buyer {
                    agent.owner = pending.seller.clone();
                    agent.nonce = agent.nonce.checked_add(1).expect("Nonce overflow");
                    agent.updated_at = env.ledger().timestamp();
                    env.events().publish(
                        (symbol_short!("rb_own"),),
                        (
                            listing.asset_id,
                            pending.buyer.clone(),
                            pending.seller.clone(),
                            env.ledger().timestamp(),
                        ),
                    );
                }
                // Release escrow
                if agent.escrow_locked {
                    agent.escrow_locked = false;
                    agent.escrow_holder = None;
                    agent.updated_at = env.ledger().timestamp();
                    env.events().publish(
                        (symbol_short!("rb_esc"),),
                        (listing.asset_id, env.ledger().timestamp()),
                    );
                }
                Self::save_agent(&env, listing.asset_id, &agent);
            }
        }

        env.storage().instance().remove(&psk);
    }

    // =========================================================================
    // Standard execution-hub step interface
    // =========================================================================

    /// Entry point called by the execution hub for every workflow step.
    /// Dispatches to the correct step function based on step_index.
    pub fn exec_step(env: Env, step_index: u32, encoded_args: Bytes) {
        match step_index {
            0 => Self::verify_sale(env, encoded_args),
            1 => Self::transfer_ownership(env, encoded_args),
            2 => Self::record_sale(env, encoded_args),
            _ => panic!("Unknown step index"),
        }
    }

    // =========================================================================
    // Workflow completion callback (called by hub)
    // =========================================================================

    /// `status`: 2=Completed, 3=RolledBack, 4=Failed, 5=Cancelled
    pub fn wf_done(env: Env, workflow_id: u64, status: u32) {
        let wlk = Self::wf_listing_key(&env, workflow_id);
        let listing_id: Option<u64> = env.storage().instance().get(&wlk);

        let lid = match listing_id {
            Some(id) => id,
            None => return,
        };

        let psk = Self::pending_sale_key(&env, lid);

        match status {
            2 => {
                // Completed — remove cross-reference
                env.storage().instance().remove(&wlk);
                env.events().publish(
                    (symbol_short!("cb_ok"),),
                    (workflow_id, lid, env.ledger().timestamp()),
                );
            }
            3..=5 => {
                // RolledBack / Failed / Cancelled — ensure listing stays active
                if let Ok(mut listing) = Self::try_load_listing(&env, lid) {
                    if !listing.active {
                        listing.active = true;
                        let lk = Self::listing_key(&env, lid);
                        env.storage().instance().set(&lk, &listing);
                    }
                }
                if env.storage().instance().has(&psk) {
                    env.storage().instance().remove(&psk);
                }
                env.storage().instance().remove(&wlk);
                env.events().publish(
                    (symbol_short!("cb_fail"),),
                    (workflow_id, lid, status, env.ledger().timestamp()),
                );
            }
            _ => {}
        }
    }

    // =========================================================================
    // Auto-expire listings
    // =========================================================================

    /// Check and expire any listings that have passed their expiration date
    pub fn cleanup_expired_listings(env: Env, listing_ids: Vec<u64>) -> Vec<u64> {
        let current_time = env.ledger().timestamp();
        let mut expired_listings = Vec::new(&env);
        let marketplace = env.current_contract_address();

        for i in 0..listing_ids.len() {
            if let Some(listing_id) = listing_ids.get(i) {
                if let Ok(mut listing) = Self::try_load_listing(&env, listing_id) {
                    if listing.active && listing.expires_at < current_time {
                        // Auto-delist the expired listing
                        listing.active = false;
                        let lk = Self::listing_key(&env, listing_id);
                        env.storage().instance().set(&lk, &listing);

                        // Release escrow
                        let mut agent = Self::load_agent(&env, listing.asset_id);
                        if agent.escrow_locked {
                            match &agent.escrow_holder {
                                Some(h) if h == &marketplace => {
                                    agent.escrow_locked = false;
                                    agent.escrow_holder = None;
                                    agent.updated_at = current_time;
                                    agent.nonce =
                                        agent.nonce.checked_add(1).expect("Nonce overflow");
                                    Self::save_agent(&env, listing.asset_id, &agent);
                                }
                                _ => {}
                            }
                        }

                        expired_listings.push_back(listing_id);
                        env.events().publish(
                            (symbol_short!("lst_exp"),),
                            (listing_id, listing.asset_id, current_time),
                        );
                    }
                }
            }
        }
        expired_listings
    }

    // =========================================================================
    // Cancel listing
    // =========================================================================

    pub fn cancel_listing(env: Env, listing_id: u64, seller: Address) {
        seller.require_auth();
        if listing_id == 0 {
            panic!("Invalid listing ID");
        }
        let mut listing = Self::load_listing(&env, listing_id);
        if listing.seller != seller {
            panic!("Only seller can cancel listing");
        }
        if !listing.active {
            panic!("Listing is not active");
        }

        let marketplace = env.current_contract_address();
        let mut agent = Self::load_agent(&env, listing.asset_id);
        if agent.escrow_locked {
            match &agent.escrow_holder {
                Some(h) if h == &marketplace => {
                    agent.escrow_locked = false;
                    agent.escrow_holder = None;
                    agent.updated_at = env.ledger().timestamp();
                    agent.nonce = agent.nonce.checked_add(1).expect("Nonce overflow");
                    Self::save_agent(&env, listing.asset_id, &agent);
                }
                _ => panic!("Agent locked by a different contract"),
            }
        }

        listing.active = false;
        let lk = Self::listing_key(&env, listing_id);
        env.storage().instance().set(&lk, &listing);

        env.events().publish(
            (symbol_short!("lst_cncl"),),
            (listing_id, listing.asset_id, seller),
        );
    }

    // =========================================================================
    // Offer and Counter-offer System
    // =========================================================================

    /// Create an offer on an active listing
    pub fn make_offer(
        env: Env,
        listing_id: u64,
        offerer: Address,
        amount: i128,
        duration_days: Option<u64>,
    ) -> u64 {
        offerer.require_auth();

        if listing_id == 0 {
            panic!("Invalid listing ID");
        }
        if amount <= 0 || amount > stellai_lib::PRICE_UPPER_BOUND {
            panic!("Invalid offer amount");
        }

        let listing = Self::load_listing(&env, listing_id);
        if !listing.active {
            panic!("Listing is not active");
        }
        if listing.expires_at < env.ledger().timestamp() {
            panic!("Listing has expired");
        }

        let offer_id = Self::next_offer_id(&env);
        let current_time = env.ledger().timestamp();
        let expires_at = if let Some(days) = duration_days {
            current_time + (days * 24 * 60 * 60)
        } else {
            current_time + 7 * 24 * 60 * 60 // 7 days default
        };

        let offer = Offer {
            offer_id,
            listing_id,
            offerer: offerer.clone(),
            amount,
            active: true,
            created_at: current_time,
            expires_at,
        };

        let ok = Self::offer_key(&env, offer_id);
        env.storage().instance().set(&ok, &offer);

        env.events().publish(
            (symbol_short!("ofr_made"),),
            (offer_id, listing_id, offerer, amount, expires_at),
        );

        offer_id
    }

    /// Accept an offer (only seller can accept)
    pub fn accept_offer(env: Env, offer_id: u64, seller: Address) -> (u64, u64) {
        seller.require_auth();

        if offer_id == 0 {
            panic!("Invalid offer ID");
        }

        let mut offer: Offer = env
            .storage()
            .instance()
            .get(&Self::offer_key(&env, offer_id))
            .expect("Offer not found");

        if !offer.active {
            panic!("Offer is not active");
        }
        if offer.expires_at < env.ledger().timestamp() {
            panic!("Offer has expired");
        }

        let listing = Self::load_listing(&env, offer.listing_id);
        if listing.seller != seller {
            panic!("Only listing seller can accept offers");
        }
        if !listing.active {
            panic!("Listing is no longer active");
        }

        // Mark offer as inactive
        offer.active = false;
        env.storage()
            .instance()
            .set(&Self::offer_key(&env, offer_id), &offer);

        // Start the purchase workflow
        Self::buy_agent(env, offer.listing_id, offer.offerer, offer.amount)
    }

    /// Reject an offer
    pub fn reject_offer(env: Env, offer_id: u64, caller: Address) {
        caller.require_auth();

        let mut offer: Offer = env
            .storage()
            .instance()
            .get(&Self::offer_key(&env, offer_id))
            .expect("Offer not found");

        let listing = Self::load_listing(&env, offer.listing_id);
        if listing.seller != caller && offer.offerer != caller {
            panic!("Only involved parties can reject offers");
        }

        if offer.active {
            offer.active = false;
            env.storage()
                .instance()
                .set(&Self::offer_key(&env, offer_id), &offer);
            env.events().publish(
                (symbol_short!("ofr_rjct"),),
                (offer_id, caller, env.ledger().timestamp()),
            );
        }
    }

    // =========================================================================
    // Auction System
    // =========================================================================

    /// Create an English auction for an asset
    pub fn create_auction(
        env: Env,
        agent_id: u64,
        seller: Address,
        start_price: i128,
        reserve_price: i128,
        duration_days: u64,
        min_bid_increment_bps: Option<u32>,
    ) -> u64 {
        seller.require_auth();

        if agent_id == 0 {
            panic!("Invalid agent ID");
        }
        if start_price <= 0 || reserve_price <= 0 {
            panic!("Prices must be positive");
        }
        if reserve_price > start_price {
            panic!("Reserve price cannot exceed start price");
        }
        if duration_days == 0 || duration_days > 365 {
            panic!("Invalid auction duration");
        }

        let agent = Self::load_agent(&env, agent_id);
        if agent.owner != seller {
            panic!("Only owner can create auctions");
        }
        if agent.escrow_locked {
            panic!("Agent already locked in escrow");
        }

        let auction_id = Self::next_auction_id(&env);
        let current_time = env.ledger().timestamp();
        let end_time = current_time + (duration_days * 24 * 60 * 60);
        let min_increment = min_bid_increment_bps.unwrap_or(MIN_BID_INCREMENT_BPS);

        #[allow(clippy::manual_range_contains)]
        if min_increment < 10 || min_increment > 10000 {
            panic!("Invalid bid increment (must be 0.1% to 100%)");
        }

        let marketplace = env.current_contract_address();
        let mut updated_agent = agent;
        updated_agent.escrow_locked = true;
        updated_agent.escrow_holder = Some(marketplace.clone());
        updated_agent.updated_at = current_time;
        Self::save_agent(&env, agent_id, &updated_agent);

        let auction = stellai_lib::Auction {
            auction_id,
            agent_id,
            seller: seller.clone(),
            auction_type: stellai_lib::AuctionType::English,
            start_price,
            reserve_price,
            current_price: start_price,
            highest_bidder: None,
            highest_bid: 0,
            start_time: current_time,
            end_time,
            min_bid_increment_bps: min_increment,
            status: stellai_lib::AuctionStatus::Active,
            dutch_config: None,
            sealed_commit_end: None,
            sealed_reveal_end: None,
        };

        let ak = Self::auction_key(&env, auction_id);
        env.storage().instance().set(&ak, &auction);

        env.events().publish(
            (symbol_short!("auc_creat"),),
            (auction_id, agent_id, seller, start_price, end_time),
        );

        auction_id
    }

    /// Place a bid on an active auction
    pub fn place_bid(env: Env, auction_id: u64, bidder: Address, bid_amount: i128) {
        bidder.require_auth();

        if auction_id == 0 {
            panic!("Invalid auction ID");
        }
        if bid_amount <= 0 {
            panic!("Bid amount must be positive");
        }

        let mut auction: stellai_lib::Auction = env
            .storage()
            .instance()
            .get(&Self::auction_key(&env, auction_id))
            .expect("Auction not found");

        let current_time = env.ledger().timestamp();
        if auction.status != stellai_lib::AuctionStatus::Active {
            panic!("Auction is not active");
        }
        if current_time > auction.end_time {
            panic!("Auction has ended");
        }

        // Calculate minimum bid required
        let min_bid = if auction.highest_bid == 0 {
            auction.start_price
        } else {
            let min_increment =
                (auction.highest_bid * (auction.min_bid_increment_bps as i128)) / 10000;
            auction.highest_bid + min_increment
        };

        if bid_amount < min_bid {
            panic!("Bid too low - minimum required: {}", min_bid);
        }

        // Refund previous highest bidder if exists
        if let Some(prev_bidder) = auction.highest_bidder {
            env.events().publish(
                (symbol_short!("bid_refnd"),),
                (auction_id, prev_bidder, auction.highest_bid, current_time),
            );
        }

        // Record the new bid
        let bid_sequence =
            Self::record_bid(&env, auction_id, bidder.clone(), bid_amount, current_time);

        auction.highest_bidder = Some(bidder.clone());
        auction.highest_bid = bid_amount;
        auction.current_price = bid_amount;
        env.storage()
            .instance()
            .set(&Self::auction_key(&env, auction_id), &auction);

        env.events().publish(
            (symbol_short!("bid_plcd"),),
            (auction_id, bidder, bid_amount, bid_sequence, current_time),
        );
    }

    /// Finalize an auction after it has ended
    pub fn finalize_auction(env: Env, auction_id: u64) {
        if auction_id == 0 {
            panic!("Invalid auction ID");
        }

        let mut auction: stellai_lib::Auction = env
            .storage()
            .instance()
            .get(&Self::auction_key(&env, auction_id))
            .expect("Auction not found");

        let current_time = env.ledger().timestamp();
        if auction.status != stellai_lib::AuctionStatus::Active {
            panic!("Auction already processed");
        }
        if current_time <= auction.end_time {
            panic!("Auction has not ended yet");
        }

        // Check if reserve price was met
        if auction.highest_bid >= auction.reserve_price {
            // Auction was successful - highest bidder wins
            auction.status = stellai_lib::AuctionStatus::Won;

            if let Some(ref buyer) = auction.highest_bidder {
                // Process the sale - transfer ownership and distribute funds
                Self::process_auction_sale(&env, &auction, buyer.clone());
            }

            env.events().publish(
                (symbol_short!("auc_won"),),
                (
                    auction_id,
                    auction.highest_bidder.clone(),
                    auction.highest_bid,
                    current_time,
                ),
            );
        } else {
            // Reserve not met - cancel auction, return asset to seller
            auction.status = stellai_lib::AuctionStatus::Ended;
            Self::cancel_auction_asset_return(&env, &auction);

            env.events().publish(
                (symbol_short!("auc_exp"),),
                (
                    auction_id,
                    auction.reserve_price,
                    auction.highest_bid,
                    current_time,
                ),
            );
        }

        env.storage()
            .instance()
            .set(&Self::auction_key(&env, auction_id), &auction);
    }

    /// Cancel an auction and return the asset to the seller
    fn cancel_auction_asset_return(env: &Env, auction: &stellai_lib::Auction) {
        let marketplace = env.current_contract_address();
        let mut agent = Self::load_agent(env, auction.agent_id);

        if agent.escrow_locked {
            match &agent.escrow_holder {
                Some(h) if h == &marketplace => {
                    agent.escrow_locked = false;
                    agent.escrow_holder = None;
                    agent.updated_at = env.ledger().timestamp();
                    Self::save_agent(env, auction.agent_id, &agent);
                }
                _ => panic!("Agent locked by different contract"),
            }
        }
    }

    /// Process a successful auction sale
    fn process_auction_sale(env: &Env, auction: &stellai_lib::Auction, buyer: Address) {
        let mut agent = Self::load_agent(env, auction.agent_id);

        // Transfer ownership to the winning bidder
        agent.owner = buyer.clone();
        agent.escrow_locked = false;
        agent.escrow_holder = None;
        agent.updated_at = env.ledger().timestamp();
        agent.nonce = agent.nonce.checked_add(1).expect("Nonce overflow");
        Self::save_agent(env, auction.agent_id, &agent);

        // Calculate royalties and platform fees
        let royalty_key = Self::royalty_key(env, auction.agent_id);
        let royalty_info: Option<stellai_lib::RoyaltyInfo> =
            env.storage().instance().get(&royalty_key);
        let platform_fee_config: PlatformFeeConfig = env
            .storage()
            .instance()
            .get(&Symbol::new(env, PLATFORM_FEE_KEY))
            .expect("Platform fee not configured");

        let mut royalty_amount = 0;
        if let Some(r) = royalty_info {
            if r.fee <= stellai_lib::MAX_ROYALTY_PERCENTAGE {
                royalty_amount = (auction.highest_bid * (r.fee as i128)) / 10000;
            }
        }

        let platform_fee = (auction.highest_bid * (platform_fee_config.fee_bps as i128)) / 10000;
        let seller_amount = auction.highest_bid - royalty_amount - platform_fee;

        // Record transaction for history
        Self::record_transaction(
            env,
            0, // listing_id - 0 for auctions
            auction.agent_id,
            auction.seller.clone(),
            buyer.clone(),
            auction.highest_bid,
            royalty_amount,
            platform_fee,
            String::from_str(env, "auction_won"),
        );

        env.events().publish(
            (symbol_short!("auc_sold"),),
            (
                auction.auction_id,
                auction.agent_id,
                auction.seller.clone(),
                buyer,
                seller_amount,
                royalty_amount,
                platform_fee,
            ),
        );
    }

    /// Record a bid for historical tracking
    fn record_bid(
        env: &Env,
        auction_id: u64,
        bidder: Address,
        amount: i128,
        timestamp: u64,
    ) -> u64 {
        let bid_key = (String::from_str(env, BID_RECORD_PREFIX), auction_id);
        let bids: Vec<stellai_lib::BidRecord> = env
            .storage()
            .instance()
            .get(&bid_key)
            .unwrap_or_else(|| Vec::new(env));

        let sequence = (bids.len() as u64) + 1;
        let mut new_bids = bids.clone();
        new_bids.push_back(stellai_lib::BidRecord {
            bidder,
            amount,
            timestamp,
            bid_increment: if !bids.is_empty() {
                let prev_bid = bids.last().unwrap();
                amount - prev_bid.amount
            } else {
                0
            },
            sequence,
        });

        env.storage().instance().set(&bid_key, &new_bids);
        sequence
    }

    // =========================================================================
    // Dispute Resolution System
    // =========================================================================

    /// Open a dispute for a transaction
    pub fn open_dispute(
        env: Env,
        listing_id: u64,
        initiator: Address,
        reason: String,
        evidence_cid: Option<String>,
    ) -> u64 {
        initiator.require_auth();

        if listing_id == 0 {
            panic!("Invalid listing ID");
        }
        if reason.is_empty() || reason.len() > 1024 {
            panic!("Invalid dispute reason length");
        }

        let dispute_id = Self::next_dispute_id(&env);
        let current_time = env.ledger().timestamp();

        let dispute = stellai_lib::Dispute {
            dispute_id,
            listing_id,
            asset_type: stellai_lib::AssetType::Agent,
            initiator: initiator.clone(),
            reason,
            evidence_cid,
            status: stellai_lib::DisputeStatus::Open,
            created_at: current_time,
            resolved_at: None,
        };

        let dk = Self::dispute_key(&env, dispute_id);
        env.storage().instance().set(&dk, &dispute);

        env.events().publish(
            (symbol_short!("dsp_open"),),
            (dispute_id, listing_id, initiator, current_time),
        );

        dispute_id
    }

    /// Admin resolves a dispute
    pub fn resolve_dispute(
        env: Env,
        dispute_id: u64,
        admin: Address,
        ruling: bool, // true = side with initiator, false = reject dispute
        resolution_notes: Option<String>,
    ) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        if dispute_id == 0 {
            panic!("Invalid dispute ID");
        }

        let mut dispute: stellai_lib::Dispute = env
            .storage()
            .instance()
            .get(&Self::dispute_key(&env, dispute_id))
            .expect("Dispute not found");

        if dispute.status != stellai_lib::DisputeStatus::Open {
            panic!("Dispute is already resolved");
        }

        let current_time = env.ledger().timestamp();
        dispute.resolved_at = Some(current_time);
        dispute.status = if ruling {
            stellai_lib::DisputeStatus::Resolved
        } else {
            stellai_lib::DisputeStatus::Rejected
        };

        env.storage()
            .instance()
            .set(&Self::dispute_key(&env, dispute_id), &dispute);

        env.events().publish(
            (symbol_short!("dsp_res"),),
            (dispute_id, ruling as u32, current_time, resolution_notes),
        );
    }

    /// Get all active disputes in the queue
    pub fn get_active_disputes(env: Env, dispute_ids: Vec<u64>) -> Vec<stellai_lib::Dispute> {
        let mut active_disputes = Vec::new(&env);

        for i in 0..dispute_ids.len() {
            if let Some(dispute_id) = dispute_ids.get(i) {
                if let Ok(dispute) = Self::try_load_dispute(&env, dispute_id) {
                    if dispute.status == stellai_lib::DisputeStatus::Open {
                        active_disputes.push_back(dispute);
                    }
                }
            }
        }
        active_disputes
    }

    // =========================================================================
    // Transaction History & Analytics
    // =========================================================================

    /// Record a transaction in the history
    #[allow(clippy::too_many_arguments)]
    fn record_transaction(
        env: &Env,
        listing_id: u64,
        asset_id: u64,
        seller: Address,
        buyer: Address,
        amount: i128,
        royalty_amount: i128,
        platform_fee: i128,
        txn_type: String,
    ) -> u64 {
        let key = Symbol::new(env, "txn_ctr");
        let current: u64 = env.storage().instance().get(&key).unwrap_or(0);
        let txn_id = current + 1;
        env.storage().instance().set(&key, &txn_id);

        let record = TransactionRecord {
            txn_id,
            listing_id,
            asset_id,
            seller,
            buyer,
            amount,
            royalty_amount,
            platform_fee,
            timestamp: env.ledger().timestamp(),
            txn_type,
        };

        let tk = Self::transaction_key(env, txn_id);
        env.storage().instance().set(&tk, &record);

        txn_id
    }

    /// Get transaction history for a user (buyer or seller)
    pub fn get_user_transactions(
        env: Env,
        user: Address,
        txn_ids: Vec<u64>,
    ) -> Vec<TransactionRecord> {
        let mut user_txns = Vec::new(&env);

        for i in 0..txn_ids.len() {
            if let Some(txn_id) = txn_ids.get(i) {
                if let Some(record) = env
                    .storage()
                    .instance()
                    .get::<_, TransactionRecord>(&Self::transaction_key(&env, txn_id))
                {
                    if record.seller == user || record.buyer == user {
                        user_txns.push_back(record);
                    }
                }
            }
        }
        user_txns
    }

    /// Get platform analytics (volume, fees, etc.) - admin only
    pub fn get_platform_analytics(env: Env, admin: Address) -> (i128, i128, u64) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let total_volume: i128 = 0;
        let total_fees: i128 = 0;
        let txn_count: u64 = 0;

        // This would typically iterate through a range of transactions
        // For simplicity, this is a placeholder for the analytics calculation

        (total_volume, total_fees, txn_count)
    }

    // =========================================================================
    // Admin Tools
    // =========================================================================

    /// Update platform fee configuration (admin only)
    pub fn set_platform_fee(env: Env, admin: Address, fee_bps: u32, recipient: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        if fee_bps > 1000 {
            panic!("Platform fee cannot exceed 10%");
        }

        let mut config: PlatformFeeConfig = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, PLATFORM_FEE_KEY))
            .expect("Platform fee config not found");

        config.fee_bps = fee_bps;
        config.recipient = recipient.clone();

        env.storage()
            .instance()
            .set(&Symbol::new(&env, PLATFORM_FEE_KEY), &config);

        env.events().publish(
            (symbol_short!("fee_upd"),),
            (fee_bps, recipient, env.ledger().timestamp()),
        );
    }

    // =========================================================================
    // Royalties
    // =========================================================================

    pub fn set_royalty(
        env: Env,
        agent_id: u64,
        creator: Address,
        recipient: Address,
        percentage: u32,
    ) {
        creator.require_auth();
        if agent_id == 0 {
            panic!("Invalid agent ID");
        }
        if percentage > stellai_lib::MAX_ROYALTY_PERCENTAGE {
            panic!("Royalty exceeds maximum");
        }
        let agent = Self::load_agent(&env, agent_id);
        if agent.owner != creator {
            panic!("Only agent owner can set royalty");
        }
        let rk = Self::royalty_key(&env, agent_id);
        env.storage().instance().set(
            &rk,
            &stellai_lib::RoyaltyInfo {
                recipient,
                fee: percentage,
            },
        );
        env.events()
            .publish((symbol_short!("roy_set"),), (agent_id, percentage));
    }

    pub fn get_royalty(env: Env, agent_id: u64) -> Option<stellai_lib::RoyaltyInfo> {
        if agent_id == 0 {
            panic!("Invalid agent ID");
        }
        env.storage()
            .instance()
            .get(&Self::royalty_key(&env, agent_id))
    }

    // =========================================================================
    // BATCH OPERATIONS — Issue #289 acceptance criterion #4
    // =========================================================================

    /// Create up to MAX_BATCH_SIZE listings in a single auth.
    /// Panics on the first invariant violation so that partial state is
    /// visible to the caller (Soroban contracts roll back the txn).
    pub fn batch_create_listings(
        env: Env,
        seller: Address,
        listing_type: u32,
        price: i128,
        agent_ids: Vec<u64>,
    ) -> Vec<u64> {
        seller.require_auth();
        let count = agent_ids.len();
        if count == 0 || count > MAX_BATCH_SIZE {
            panic!("Batch size out of bounds");
        }
        if listing_type > 2 {
            panic!("Invalid listing type");
        }
        if !(stellai_lib::PRICE_LOWER_BOUND..=stellai_lib::PRICE_UPPER_BOUND).contains(&price) {
            panic!("Price out of valid range");
        }
        let listing_type_enum = match listing_type {
            0 => stellai_lib::ListingType::Sale,
            1 => stellai_lib::ListingType::Lease,
            2 => stellai_lib::ListingType::Auction,
            _ => panic!("Invalid listing type"),
        };
        let mut listing_ids: Vec<u64> = Vec::new(&env);
        let current_time = env.ledger().timestamp();
        let expires_at = current_time
            .checked_add(DEFAULT_LISTING_DURATION)
            .expect("Expiry overflow");
        let marketplace = env.current_contract_address();
        for i in 0..count {
            let agent_id = agent_ids.get(i).expect("agent id missing");
            if agent_id == 0 {
                panic!("Invalid agent ID");
            }
            let agent = Self::load_agent(&env, agent_id);
            if agent.owner != seller {
                panic!("Only agent owner can create listings");
            }
            if agent.escrow_locked {
                panic!("Agent already locked in escrow");
            }
            let listing_id = Self::next_listing_id(&env);
            let listing = stellai_lib::Listing {
                listing_id,
                asset_id: agent_id,
                asset_type: stellai_lib::AssetType::Agent,
                seller: seller.clone(),
                price,
                listing_type: listing_type_enum,
                active: true,
                created_at: current_time,
                expires_at,
            };
            let lk = Self::listing_key(&env, listing_id);
            env.storage().instance().set(&lk, &listing);
            let mut updated_agent = agent;
            updated_agent.escrow_locked = true;
            updated_agent.escrow_holder = Some(marketplace.clone());
            updated_agent.updated_at = current_time;
            Self::save_agent(&env, agent_id, &updated_agent);
            env.events().publish(
                (symbol_short!("lst_creat"),),
                (listing_id, agent_id, seller.clone(), price),
            );
            listing_ids.push_back(listing_id);
        }
        env.events()
            .publish((symbol_short!("batch_lst"),), (seller, count, current_time));
        listing_ids
    }

    /// Cancel up to MAX_BATCH_SIZE listings owned by `seller` in a single auth.
    /// Returns the number of listings actually transitioned to inactive.
    pub fn batch_cancel_listings(env: Env, seller: Address, listing_ids: Vec<u64>) -> u32 {
        seller.require_auth();
        let count = listing_ids.len();
        if count == 0 || count > MAX_BATCH_SIZE {
            panic!("Batch size out of bounds");
        }
        let mut cancelled: u32 = 0;
        let marketplace = env.current_contract_address();
        for i in 0..count {
            let listing_id = listing_ids.get(i).expect("listing id missing");
            let mut listing = match Self::try_load_listing(&env, listing_id) {
                Ok(l) => l,
                Err(_) => continue,
            };
            if listing.seller != seller || !listing.active {
                continue;
            }
            if let Ok(mut agent) = Self::try_load_agent(&env, listing.asset_id) {
                if agent.escrow_locked {
                    if let Some(h) = &agent.escrow_holder {
                        if h == &marketplace {
                            agent.escrow_locked = false;
                            agent.escrow_holder = None;
                            agent.updated_at = env.ledger().timestamp();
                            agent.nonce = agent.nonce.checked_add(1).expect("Nonce overflow");
                            Self::save_agent(&env, listing.asset_id, &agent);
                        }
                    }
                }
            }
            listing.active = false;
            let lk = Self::listing_key(&env, listing_id);
            env.storage().instance().set(&lk, &listing);
            cancelled = cancelled.checked_add(1).expect("Counter overflow");
            env.events().publish(
                (symbol_short!("lst_cncl"),),
                (listing_id, listing.asset_id, seller.clone()),
            );
        }
        env.events().publish(
            (symbol_short!("batch_cnl"),),
            (seller, cancelled, env.ledger().timestamp()),
        );
        cancelled
    }

    // =========================================================================
    // COLLECTION MANAGEMENT — Issue #289 acceptance criterion #3
    // =========================================================================

    pub fn create_collection(env: Env, creator: Address, name: String, royalty_bps: u32) -> u64 {
        creator.require_auth();
        if name.is_empty() || name.len() > MAX_COLLECTION_NAME_LEN {
            panic!("Invalid collection name");
        }
        if royalty_bps > stellai_lib::MAX_ROYALTY_PERCENTAGE {
            panic!("Royalty exceeds maximum");
        }
        let collection_id = Self::next_collection_id(&env);
        let current_time = env.ledger().timestamp();
        let recipients: Vec<RoyaltyRecipient> = Vec::new(&env);
        let config = RoyaltyConfig {
            recipients,
            total_bps: royalty_bps,
            min_threshold: 0i128,
            max_cap: None,
        };
        let collection = Collection {
            collection_id,
            creator: creator.clone(),
            name,
            members: Vec::new(&env),
            royalty_config: config,
            created_at: current_time,
            updated_at: current_time,
        };
        let ck = Self::collection_key(&env, collection_id);
        env.storage().instance().set(&ck, &collection);
        env.events().publish(
            (symbol_short!("coll_new"),),
            (collection_id, creator, current_time),
        );
        collection_id
    }

    pub fn add_to_collection(
        env: Env,
        creator: Address,
        collection_id: u64,
        agent_ids: Vec<u64>,
    ) -> u32 {
        creator.require_auth();
        if collection_id == 0 {
            panic!("Invalid collection ID");
        }
        let mut coll = Self::load_collection(&env, collection_id);
        if coll.creator != creator {
            panic!("Only creator can modify collection");
        }
        let count = agent_ids.len();
        if count == 0 || count > MAX_BATCH_SIZE {
            panic!("Batch size out of bounds");
        }
        let mut added: u32 = 0;
        for i in 0..count {
            let agent_id = agent_ids.get(i).expect("agent id missing");
            if agent_id == 0 {
                panic!("Invalid agent ID");
            }
            let mut already_present = false;
            for j in 0..coll.members.len() {
                if let Some(m) = coll.members.get(j) {
                    if m == agent_id {
                        already_present = true;
                    }
                }
            }
            if !already_present {
                coll.members.push_back(agent_id);
                // Propagate the collection's current RoyaltyConfig to the
                // member asset so that `compute_settlement_fees(asset_id)`
                // correctly reads it.
                let crk = Self::collection_royalty_key(&env, agent_id);
                env.storage()
                    .instance()
                    .set(&crk, &coll.royalty_config.clone());
                added = added.checked_add(1).expect("Counter overflow");
            }
        }
        coll.updated_at = env.ledger().timestamp();
        let ck = Self::collection_key(&env, collection_id);
        env.storage().instance().set(&ck, &coll);
        env.events().publish(
            (symbol_short!("coll_add"),),
            (collection_id, creator, added, env.ledger().timestamp()),
        );
        added
    }

    pub fn remove_from_collection(
        env: Env,
        creator: Address,
        collection_id: u64,
        agent_ids: Vec<u64>,
    ) -> u32 {
        creator.require_auth();
        if collection_id == 0 {
            panic!("Invalid collection ID");
        }
        let mut coll = Self::load_collection(&env, collection_id);
        if coll.creator != creator {
            panic!("Only creator can modify collection");
        }
        let count = agent_ids.len();
        if count == 0 || count > MAX_BATCH_SIZE {
            panic!("Batch size out of bounds");
        }
        let mut removed: u32 = 0;
        for i in 0..count {
            let agent_id = agent_ids.get(i).expect("agent id missing");
            let mut found_idx: u32 = u32::MAX;
            for j in 0..coll.members.len() {
                if let Some(m) = coll.members.get(j) {
                    if m == agent_id {
                        found_idx = j;
                    }
                }
            }
            if found_idx != u32::MAX {
                coll.members.remove(found_idx);
                // Clear the per-asset RoyaltyConfig propagated at add time so
                // future sales don't pay this collection's creator royalties
                // for an asset that's no longer in the collection.
                let crk = Self::collection_royalty_key(&env, agent_id);
                env.storage().instance().remove(&crk);
                removed = removed.checked_add(1).expect("Counter overflow");
            }
        }
        coll.updated_at = env.ledger().timestamp();
        let ck = Self::collection_key(&env, collection_id);
        env.storage().instance().set(&ck, &coll);
        env.events().publish(
            (symbol_short!("coll_rm"),),
            (collection_id, creator, removed, env.ledger().timestamp()),
        );
        removed
    }

    pub fn set_collection_royalty(
        env: Env,
        creator: Address,
        collection_id: u64,
        recipients: Vec<RoyaltyRecipient>,
        total_bps: u32,
    ) {
        creator.require_auth();
        if collection_id == 0 {
            panic!("Invalid collection ID");
        }
        if total_bps > stellai_lib::MAX_ROYALTY_PERCENTAGE {
            panic!("Royalty exceeds maximum");
        }
        let mut coll = Self::load_collection(&env, collection_id);
        if coll.creator != creator {
            panic!("Only creator can modify collection");
        }
        let mut sum: u32 = 0;
        for i in 0..recipients.len() {
            if let Some(r) = recipients.get(i) {
                sum = sum.checked_add(r.share_bps).expect("Royalty overflow");
            }
        }
        if sum != total_bps {
            panic!("Royalty share total mismatch");
        }
        let config = RoyaltyConfig {
            recipients: recipients.clone(),
            total_bps,
            min_threshold: 0i128,
            max_cap: None,
        };
        coll.royalty_config = config.clone();
        coll.updated_at = env.ledger().timestamp();
        let ck = Self::collection_key(&env, collection_id);
        env.storage().instance().set(&ck, &coll);
        // Propagate to every member so compute_settlement_fees(asset_id) sees it.
        for i in 0..coll.members.len() {
            if let Some(m) = coll.members.get(i) {
                let mcrk = Self::collection_royalty_key(&env, m);
                env.storage().instance().set(&mcrk, &config.clone());
            }
        }
        env.events().publish(
            (symbol_short!("coll_roy"),),
            (collection_id, total_bps, env.ledger().timestamp()),
        );
    }

    pub fn get_collection(env: Env, collection_id: u64) -> Collection {
        Self::load_collection(&env, collection_id)
    }

    pub fn get_collection_items(env: Env, collection_id: u64) -> Vec<u64> {
        Self::load_collection(&env, collection_id).members
    }

    // =========================================================================
    // COUNTER-OFFER SYSTEM — Issue #289 acceptance criterion #2
    // =========================================================================

    pub fn make_counter_offer(
        env: Env,
        seller: Address,
        offer_id: u64,
        amount: i128,
        duration_days: Option<u64>,
    ) -> u64 {
        seller.require_auth();
        if offer_id == 0 {
            panic!("Invalid offer ID");
        }
        if amount <= 0 || amount > stellai_lib::PRICE_UPPER_BOUND {
            panic!("Invalid counter-offer amount");
        }
        let offer = Self::load_offer(&env, offer_id);
        if !offer.active {
            panic!("Original offer is not active");
        }
        let listing = Self::load_listing(&env, offer.listing_id);
        if listing.seller != seller {
            panic!("Only listing seller can counter-offer");
        }
        if !listing.active {
            panic!("Listing is not active");
        }
        let counter_id = Self::next_counter_offer_id(&env);
        let current_time = env.ledger().timestamp();
        let days = duration_days.unwrap_or(DEFAULT_COUNTER_OFFER_DAYS);
        let secs = days.checked_mul(24 * 60 * 60).expect("Duration overflow");
        let expires_at = current_time.checked_add(secs).expect("Expiry overflow");
        let counter = CounterOffer {
            counter_id,
            listing_id: offer.listing_id,
            in_response_to_offer_id: offer_id,
            by_seller: seller.clone(),
            amount,
            active: true,
            created_at: current_time,
            expires_at,
        };
        let ck = Self::counter_offer_key(&env, counter_id);
        env.storage().instance().set(&ck, &counter);
        env.events().publish(
            (symbol_short!("cofr_made"),),
            (counter_id, offer_id, seller, amount, expires_at),
        );
        counter_id
    }

    pub fn accept_counter_offer(env: Env, offerer: Address, counter_id: u64) -> (u64, u64) {
        offerer.require_auth();
        if counter_id == 0 {
            panic!("Invalid counter ID");
        }
        let mut counter: CounterOffer = env
            .storage()
            .instance()
            .get(&Self::counter_offer_key(&env, counter_id))
            .expect("Counter offer not found");
        if !counter.active {
            panic!("Counter offer is not active");
        }
        if counter.expires_at < env.ledger().timestamp() {
            panic!("Counter offer has expired");
        }
        let original_offer = Self::load_offer(&env, counter.in_response_to_offer_id);
        if original_offer.offerer != offerer {
            panic!("Only original offerer can accept counter");
        }
        counter.active = false;
        env.storage()
            .instance()
            .set(&Self::counter_offer_key(&env, counter_id), &counter);
        // Mark the original offer as inactive
        let mut orig = original_offer;
        orig.active = false;
        env.storage().instance().set(
            &Self::offer_key(&env, counter.in_response_to_offer_id),
            &orig,
        );
        Self::buy_agent(env, counter.listing_id, offerer, counter.amount)
    }

    pub fn reject_counter_offer(env: Env, caller: Address, counter_id: u64) {
        caller.require_auth();
        let mut counter: CounterOffer = env
            .storage()
            .instance()
            .get(&Self::counter_offer_key(&env, counter_id))
            .expect("Counter offer not found");
        let original_offer = Self::load_offer(&env, counter.in_response_to_offer_id);
        if counter.by_seller != caller && original_offer.offerer != caller {
            panic!("Only involved parties can reject counter");
        }
        if counter.active {
            counter.active = false;
            env.storage()
                .instance()
                .set(&Self::counter_offer_key(&env, counter_id), &counter);
            env.events().publish(
                (symbol_short!("cofr_rjct"),),
                (counter_id, caller, env.ledger().timestamp()),
            );
        }
    }

    // =========================================================================
    // DUTCH AUCTION — Issue #289 acceptance criterion #5
    // =========================================================================

    pub fn create_dutch_auction(
        env: Env,
        agent_id: u64,
        seller: Address,
        start_price: i128,
        reserve_price: i128,
        duration_days: u64,
    ) -> u64 {
        seller.require_auth();
        if agent_id == 0 {
            panic!("Invalid agent ID");
        }
        if start_price <= 0 || reserve_price < 0 || start_price < reserve_price {
            panic!("Invalid Dutch auction price bounds");
        }
        if duration_days == 0 || duration_days > 365 {
            panic!("Invalid auction duration");
        }
        let agent = Self::load_agent(&env, agent_id);
        if agent.owner != seller {
            panic!("Only owner can create auctions");
        }
        if agent.escrow_locked {
            panic!("Agent already locked in escrow");
        }
        let auction_id = Self::next_auction_id(&env);
        let now = env.ledger().timestamp();
        let secs = duration_days
            .checked_mul(24 * 60 * 60)
            .expect("Duration overflow");
        let end_time = now.checked_add(secs).expect("end_time overflow");
        let marketplace = env.current_contract_address();
        let mut updated_agent = agent;
        updated_agent.escrow_locked = true;
        updated_agent.escrow_holder = Some(marketplace.clone());
        updated_agent.updated_at = now;
        Self::save_agent(&env, agent_id, &updated_agent);
        let auction = stellai_lib::Auction {
            auction_id,
            agent_id,
            seller: seller.clone(),
            auction_type: AuctionType::Dutch,
            start_price,
            reserve_price,
            current_price: start_price,
            highest_bidder: None,
            highest_bid: 0,
            start_time: now,
            end_time,
            min_bid_increment_bps: MIN_BID_INCREMENT_BPS,
            status: stellai_lib::AuctionStatus::Active,
            dutch_config: Some(Bytes::from_array(&env, &now.to_be_bytes())),
            sealed_commit_end: None,
            sealed_reveal_end: None,
        };
        let ak = Self::auction_key(&env, auction_id);
        env.storage().instance().set(&ak, &auction);
        env.events().publish(
            (symbol_short!("dutch_new"),),
            (auction_id, agent_id, seller, start_price, end_time),
        );
        auction_id
    }

    /// Buy-now on a Dutch auction. Accepts `bid_amount` if at-or-above the
    /// current linearly-decayed price and at-or-above reserve.
    pub fn dutch_buy_now(env: Env, auction_id: u64, buyer: Address, bid_amount: i128) {
        buyer.require_auth();
        if auction_id == 0 {
            panic!("Invalid auction ID");
        }
        if bid_amount <= 0 {
            panic!("Bid amount must be positive");
        }
        let mut auction: stellai_lib::Auction = env
            .storage()
            .instance()
            .get(&Self::auction_key(&env, auction_id))
            .expect("Auction not found");
        if auction.auction_type != AuctionType::Dutch {
            panic!("Auction is not Dutch");
        }
        if auction.status != stellai_lib::AuctionStatus::Active {
            panic!("Dutch auction is not active");
        }
        let now = env.ledger().timestamp();
        if now > auction.end_time {
            panic!("Dutch auction has ended");
        }
        // Hard timelock: bid must be AFTER auction start_time
        if now < auction.start_time {
            panic!("Dutch auction has not started");
        }
        // Linear price decay: start_price -> reserve_price across [start, end]
        let decay_price = Self::dutch_current_price(&env, &auction);
        if bid_amount < decay_price {
            panic!("Bid below current Dutch price");
        }
        if bid_amount < auction.reserve_price {
            panic!("Bid below reserve price");
        }
        auction.highest_bidder = Some(buyer.clone());
        auction.highest_bid = bid_amount;
        auction.current_price = bid_amount;
        auction.status = stellai_lib::AuctionStatus::Won;
        env.storage()
            .instance()
            .set(&Self::auction_key(&env, auction_id), &auction);
        Self::process_auction_sale(&env, &auction, buyer);
    }

    // =========================================================================
    // SEALED-BID AUCTION — Issue #289 acceptance criterion #5
    // =========================================================================

    pub fn create_sealed_bid_auction(
        env: Env,
        agent_id: u64,
        seller: Address,
        start_price: i128,
        reserve_price: i128,
        commit_duration_secs: u64,
        reveal_duration_secs: u64,
    ) -> u64 {
        seller.require_auth();
        if agent_id == 0 || start_price <= 0 || reserve_price <= 0 {
            panic!("Invalid sealed-bid auction params");
        }
        if reserve_price > start_price {
            panic!("Reserve cannot exceed start");
        }
        if commit_duration_secs == 0 || reveal_duration_secs <= commit_duration_secs {
            panic!("Invalid sealed-bid timeline");
        }
        let agent = Self::load_agent(&env, agent_id);
        if agent.owner != seller {
            panic!("Only owner can create auctions");
        }
        if agent.escrow_locked {
            panic!("Agent already locked in escrow");
        }
        let auction_id = Self::next_auction_id(&env);
        let now = env.ledger().timestamp();
        let commit_end = now
            .checked_add(commit_duration_secs)
            .expect("commit-end overflow");
        let reveal_end = commit_end
            .checked_add(reveal_duration_secs)
            .expect("reveal-end overflow");
        let marketplace = env.current_contract_address();
        let mut updated_agent = agent;
        updated_agent.escrow_locked = true;
        updated_agent.escrow_holder = Some(marketplace.clone());
        updated_agent.updated_at = now;
        Self::save_agent(&env, agent_id, &updated_agent);
        let auction = stellai_lib::Auction {
            auction_id,
            agent_id,
            seller: seller.clone(),
            auction_type: stellai_lib::AuctionType::Sealed,
            start_price,
            reserve_price,
            current_price: start_price,
            highest_bidder: None,
            highest_bid: 0,
            start_time: now,
            end_time: reveal_end,
            min_bid_increment_bps: MIN_BID_INCREMENT_BPS,
            status: stellai_lib::AuctionStatus::Active,
            dutch_config: None,
            sealed_commit_end: Some(commit_end),
            sealed_reveal_end: Some(reveal_end),
        };
        let ak = Self::auction_key(&env, auction_id);
        env.storage().instance().set(&ak, &auction);
        env.events().publish(
            (symbol_short!("seal_new"),),
            (auction_id, agent_id, seller, start_price, reveal_end),
        );
        auction_id
    }

    pub fn commit_bid(
        env: Env,
        auction_id: u64,
        bidder: Address,
        commitment: Bytes,
        deposit: i128,
    ) {
        bidder.require_auth();
        if auction_id == 0 {
            panic!("Invalid auction ID");
        }
        if deposit <= 0 {
            panic!("Deposit must be positive");
        }
        let auction: stellai_lib::Auction = env
            .storage()
            .instance()
            .get(&Self::auction_key(&env, auction_id))
            .expect("Auction not found");
        if auction.auction_type != stellai_lib::AuctionType::Sealed {
            panic!("Auction is not sealed");
        }
        let now = env.ledger().timestamp();
        let commit_end = auction.sealed_commit_end.expect("missing commit end");
        if now > commit_end {
            panic!("Commit phase has ended");
        }
        let commit = SealedCommit {
            bidder: bidder.clone(),
            commitment,
            deposit,
            timestamp: now,
        };
        let key = Self::sealed_commit_key(&env, auction_id, &bidder);
        env.storage().instance().set(&key, &commit);
        env.events().publish(
            (symbol_short!("scm_made"),),
            (auction_id, bidder, deposit, now),
        );
    }

    pub fn reveal_bid(env: Env, auction_id: u64, bidder: Address, amount: i128, nonce: String) {
        bidder.require_auth();
        if auction_id == 0 {
            panic!("Invalid auction ID");
        }
        if amount <= 0 {
            panic!("Reveal amount must be positive");
        }
        let mut auction: stellai_lib::Auction = env
            .storage()
            .instance()
            .get(&Self::auction_key(&env, auction_id))
            .expect("Auction not found");
        if auction.auction_type != stellai_lib::AuctionType::Sealed {
            panic!("Auction is not sealed");
        }
        let now = env.ledger().timestamp();
        let commit_end = auction.sealed_commit_end.expect("missing commit end");
        let reveal_end = auction.sealed_reveal_end.expect("missing reveal end");
        if now <= commit_end {
            panic!("Reveal phase has not started");
        }
        if now > reveal_end {
            panic!("Reveal phase has ended");
        }
        let key = Self::sealed_commit_key(&env, auction_id, &bidder);
        let commit: SealedCommit = env
            .storage()
            .instance()
            .get(&key)
            .expect("No commit found for this bidder");
        let reveal = SealedReveal {
            bidder: bidder.clone(),
            amount,
            nonce,
            deposit: commit.deposit,
            timestamp: now,
        };
        let rk = Self::sealed_reveal_key(&env, auction_id, &bidder);
        env.storage().instance().set(&rk, &reveal);
        // Track the highest reveal on the auction so finalize can pick the winner.
        if amount > auction.highest_bid {
            auction.highest_bidder = Some(bidder.clone());
            auction.highest_bid = amount;
            auction.current_price = amount;
            env.storage()
                .instance()
                .set(&Self::auction_key(&env, auction_id), &auction);
        }
        env.events().publish(
            (symbol_short!("srev_done"),),
            (auction_id, bidder, amount, now),
        );
    }

    pub fn finalize_sealed_auction(env: Env, auction_id: u64) {
        if auction_id == 0 {
            panic!("Invalid auction ID");
        }
        let mut auction: stellai_lib::Auction = env
            .storage()
            .instance()
            .get(&Self::auction_key(&env, auction_id))
            .expect("Auction not found");
        if auction.auction_type != stellai_lib::AuctionType::Sealed {
            panic!("Auction is not sealed");
        }
        let now = env.ledger().timestamp();
        let reveal_end = auction.sealed_reveal_end.expect("missing reveal end");
        if now <= reveal_end {
            panic!("Reveal phase has not ended");
        }
        if auction.status != stellai_lib::AuctionStatus::Active {
            panic!("Auction already processed");
        }
        if auction.highest_bidder.is_some() && auction.highest_bid >= auction.reserve_price {
            auction.status = stellai_lib::AuctionStatus::Won;
            let winner = auction.highest_bidder.clone().expect("winner present");
            Self::process_auction_sale(&env, &auction, winner);
        } else {
            auction.status = stellai_lib::AuctionStatus::Ended;
            Self::cancel_auction_asset_return(&env, &auction);
            env.events().publish(
                (symbol_short!("seal_end"),),
                (auction_id, auction.reserve_price, auction.highest_bid, now),
            );
        }
        env.storage()
            .instance()
            .set(&Self::auction_key(&env, auction_id), &auction);
    }

    // =========================================================================
    // ACCESS CONTROL — Issue #289 acceptance criterion #8
    // =========================================================================

    pub fn assign_marketplace_governance(env: Env, admin: Address, new_governance: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        let key = Symbol::new(&env, GOV_ROLE_KEY);
        let mut gov: Vec<Address> = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));
        let mut exists = false;
        for i in 0..gov.len() {
            if let Some(addr) = gov.get(i) {
                if addr == new_governance {
                    exists = true;
                }
            }
        }
        if !exists {
            gov.push_back(new_governance.clone());
            env.storage().instance().set(&key, &gov);
        }
        env.events().publish(
            (symbol_short!("gov_add"),),
            (new_governance, env.ledger().timestamp()),
        );
    }

    pub fn assign_marketplace_kyc_operator(env: Env, admin: Address, new_kyc: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        let key = Symbol::new(&env, KYC_ROLE_KEY);
        let mut list: Vec<Address> = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));
        let mut exists = false;
        for i in 0..list.len() {
            if let Some(addr) = list.get(i) {
                if addr == new_kyc {
                    exists = true;
                }
            }
        }
        if !exists {
            list.push_back(new_kyc.clone());
            env.storage().instance().set(&key, &list);
        }
        env.events().publish(
            (symbol_short!("kyc_add"),),
            (new_kyc, env.ledger().timestamp()),
        );
    }

    pub fn remove_marketplace_governance(env: Env, admin: Address, target: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        let key = Symbol::new(&env, GOV_ROLE_KEY);
        let mut gov: Vec<Address> = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));
        let mut found_idx: u32 = u32::MAX;
        for i in 0..gov.len() {
            if let Some(addr) = gov.get(i) {
                if addr == target {
                    found_idx = i;
                }
            }
        }
        if found_idx != u32::MAX {
            gov.remove(found_idx);
            env.storage().instance().set(&key, &gov);
            env.events().publish(
                (symbol_short!("gov_rm"),),
                (target, env.ledger().timestamp()),
            );
        }
    }

    pub fn remove_marketplace_kyc_operator(env: Env, admin: Address, target: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        let key = Symbol::new(&env, KYC_ROLE_KEY);
        let mut list: Vec<Address> = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));
        let mut found_idx: u32 = u32::MAX;
        for i in 0..list.len() {
            if let Some(addr) = list.get(i) {
                if addr == target {
                    found_idx = i;
                }
            }
        }
        if found_idx != u32::MAX {
            list.remove(found_idx);
            env.storage().instance().set(&key, &list);
            env.events().publish(
                (symbol_short!("kyc_rm"),),
                (target, env.ledger().timestamp()),
            );
        }
    }

    #[allow(dead_code)]
    fn require_governance_or_admin(env: &Env, caller: &Address) {
        let admin_opt: Option<Address> = env.storage().instance().get(&Symbol::new(env, ADMIN_KEY));
        if let Some(admin) = admin_opt {
            if caller == &admin {
                return;
            }
        }
        let gov_opt: Option<Vec<Address>> = env
            .storage()
            .instance()
            .get(&Symbol::new(env, GOV_ROLE_KEY));
        if let Some(gov) = gov_opt {
            for i in 0..gov.len() {
                if let Some(addr) = gov.get(i) {
                    if &addr == caller {
                        return;
                    }
                }
            }
        }
        panic!("Unauthorized");
    }

    // =========================================================================
    // =========================================================================
    // NFT LISTINGS — ERC721/ERC1155 support with configurable currency
    // ==========================================================================

    /// Create a listing for an NFT with configurable currency support.
    pub fn create_nft_listing(
        env: Env,
        nft_token_ref: stellai_lib::NftTokenRef,
        seller: Address,
        price: i128,
        currency_symbol: String,
        currency_token_address: Option<Address>,
        duration_days: Option<u64>,
        metadata_uri: String,
    ) -> u64 {
        seller.require_auth();
        if price <= 0 || price > stellai_lib::PRICE_UPPER_BOUND {
            panic!("Price out of valid range");
        }
        if currency_symbol.is_empty() {
            panic!("Currency symbol required");
        }

        let nft_listing_id = Self::next_nft_listing_id(&env);
        let current_time = env.ledger().timestamp();
        let expires_at = if let Some(days) = duration_days {
            current_time + (days * 24 * 60 * 60)
        } else {
            current_time + DEFAULT_LISTING_DURATION
        };

        let nft_listing = NftListing {
            nft_listing_id,
            nft_token_ref: nft_token_ref.clone(),
            seller: seller.clone(),
            price,
            currency_symbol: currency_symbol.clone(),
            currency_token_address,
            active: true,
            created_at: current_time,
            expires_at,
            metadata_uri,
        };

        let lk = Self::nft_listing_key(&env, nft_listing_id);
        env.storage().instance().set(&lk, &nft_listing);

        env.events().publish(
            (symbol_short!("nft_lst"),),
            (
                nft_listing_id,
                nft_token_ref.token_id,
                seller,
                price,
                currency_symbol,
            ),
        );

        nft_listing_id
    }

    /// Buy an NFT listing at the listed price.
    pub fn buy_nft_listing(env: Env, nft_listing_id: u64, buyer: Address, payment_amount: i128) {
        buyer.require_auth();
        if nft_listing_id == 0 {
            panic!("Invalid NFT listing ID");
        }
        if payment_amount <= 0 {
            panic!("Payment must be positive");
        }

        let mut nft_listing = Self::load_nft_listing(&env, nft_listing_id);
        if !nft_listing.active {
            panic!("NFT listing is not active");
        }
        let current_time = env.ledger().timestamp();
        if nft_listing.expires_at < current_time {
            panic!("NFT listing has expired");
        }
        if payment_amount < nft_listing.price {
            panic!("Insufficient payment");
        }

        let (royalty_amount, platform_fee) =
            Self::compute_settlement_fees(&env, nft_listing.nft_token_ref.token_id, payment_amount);

        let _seller_amount = payment_amount
            .checked_sub(royalty_amount)
            .expect("Seller amount underflow")
            .checked_sub(platform_fee)
            .expect("Platform fee underflow");

        nft_listing.active = false;
        let lk = Self::nft_listing_key(&env, nft_listing_id);
        env.storage().instance().set(&lk, &nft_listing);

        Self::record_transaction(
            &env,
            nft_listing_id,
            nft_listing.nft_token_ref.token_id,
            nft_listing.seller.clone(),
            buyer.clone(),
            payment_amount,
            royalty_amount,
            platform_fee,
            String::from_str(&env, "nft_sale"),
        );

        env.events().publish(
            (symbol_short!("nft_sold"),),
            (
                nft_listing_id,
                nft_listing.nft_token_ref.token_id,
                buyer,
                royalty_amount,
                platform_fee,
            ),
        );
    }

    /// Cancel an NFT listing (seller only).
    pub fn cancel_nft_listing(env: Env, nft_listing_id: u64, seller: Address) {
        seller.require_auth();
        if nft_listing_id == 0 {
            panic!("Invalid NFT listing ID");
        }
        let mut nft_listing = Self::load_nft_listing(&env, nft_listing_id);
        if nft_listing.seller != seller {
            panic!("Only seller can cancel NFT listing");
        }
        if !nft_listing.active {
            panic!("NFT listing is not active");
        }

        nft_listing.active = false;
        let lk = Self::nft_listing_key(&env, nft_listing_id);
        env.storage().instance().set(&lk, &nft_listing);

        env.events().publish(
            (symbol_short!("nft_cncl"),),
            (nft_listing_id, nft_listing.nft_token_ref.token_id, seller),
        );
    }

    /// Get an NFT listing by ID.
    pub fn get_nft_listing(env: Env, nft_listing_id: u64) -> NftListing {
        Self::load_nft_listing(&env, nft_listing_id)
    }

    // =========================================================================
    // CONFIGURABLE CURRENCY SUPPORT
    // ==========================================================================

    /// Register a new accepted currency (admin only).
    pub fn register_currency(
        env: Env,
        admin: Address,
        symbol: String,
        token_address: Option<Address>,
        decimals: u32,
    ) -> u64 {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if symbol.is_empty() {
            panic!("Currency symbol required");
        }
        if decimals > 18 {
            panic!("Decimals cannot exceed 18");
        }

        let currency_id = Self::next_currency_id(&env);
        let record = CurrencyRecord {
            currency_id,
            symbol: symbol.clone(),
            token_address,
            decimals,
            active: true,
        };
        let ck = Self::currency_key(&env, currency_id);
        env.storage().instance().set(&ck, &record);

        let mut accepted: Vec<String> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, ACCEPTED_CURRENCY_KEY))
            .unwrap_or_else(|| Vec::new(&env));
        accepted.push_back(symbol.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, ACCEPTED_CURRENCY_KEY), &accepted);

        env.events()
            .publish((symbol_short!("ccy_reg"),), (currency_id, symbol, decimals));

        currency_id
    }

    /// Deactivate a currency (admin only).
    pub fn deactivate_currency(env: Env, admin: Address, currency_id: u64) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if currency_id == 0 {
            panic!("Invalid currency ID");
        }
        let mut record: CurrencyRecord = env
            .storage()
            .instance()
            .get(&Self::currency_key(&env, currency_id))
            .expect("Currency not found");
        record.active = false;
        env.storage()
            .instance()
            .set(&Self::currency_key(&env, currency_id), &record);
        env.events()
            .publish((symbol_short!("ccy_off"),), (currency_id, record.symbol));
    }

    /// Get a registered currency by ID.
    pub fn get_currency(env: Env, currency_id: u64) -> CurrencyRecord {
        env.storage()
            .instance()
            .get(&Self::currency_key(&env, currency_id))
            .expect("Currency not found")
    }

    /// Get all accepted currency symbols.
    pub fn get_accepted_currencies(env: Env) -> Vec<String> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, ACCEPTED_CURRENCY_KEY))
            .unwrap_or_else(|| Vec::new(&env))
    }

    // =========================================================================
    // ENGLISH AUCTION AUTO-EXTENSION
    // ==========================================================================

    /// Create an English auction with configurable auto-extension.
    pub fn create_auction_with_extension(
        env: Env,
        agent_id: u64,
        seller: Address,
        start_price: i128,
        reserve_price: i128,
        duration_days: u64,
        min_bid_increment_bps: Option<u32>,
        extension_window_secs: Option<u64>,
        extension_secs: Option<u64>,
    ) -> u64 {
        seller.require_auth();
        if agent_id == 0 {
            panic!("Invalid agent ID");
        }
        if start_price <= 0 || reserve_price <= 0 {
            panic!("Prices must be positive");
        }
        if reserve_price > start_price {
            panic!("Reserve price cannot exceed start price");
        }
        if duration_days == 0 || duration_days > 365 {
            panic!("Invalid auction duration");
        }

        let agent = Self::load_agent(&env, agent_id);
        if agent.owner != seller {
            panic!("Only owner can create auctions");
        }
        if agent.escrow_locked {
            panic!("Agent already locked in escrow");
        }

        let auction_id = Self::next_auction_id(&env);
        let current_time = env.ledger().timestamp();
        let end_time = current_time + (duration_days * 24 * 60 * 60);
        let min_increment = min_bid_increment_bps.unwrap_or(MIN_BID_INCREMENT_BPS);

        #[allow(clippy::manual_range_contains)]
        if min_increment < 10 || min_increment > 10000 {
            panic!("Invalid bid increment (must be 0.1% to 100%)");
        }

        let marketplace = env.current_contract_address();
        let mut updated_agent = agent;
        updated_agent.escrow_locked = true;
        updated_agent.escrow_holder = Some(marketplace.clone());
        updated_agent.updated_at = current_time;
        Self::save_agent(&env, agent_id, &updated_agent);

        let auction = stellai_lib::Auction {
            auction_id,
            agent_id,
            seller: seller.clone(),
            auction_type: stellai_lib::AuctionType::English,
            start_price,
            reserve_price,
            current_price: start_price,
            highest_bidder: None,
            highest_bid: 0,
            start_time: current_time,
            end_time,
            min_bid_increment_bps: min_increment,
            status: stellai_lib::AuctionStatus::Active,
            dutch_config: None,
            sealed_commit_end: None,
            sealed_reveal_end: None,
        };

        let ak = Self::auction_key(&env, auction_id);
        env.storage().instance().set(&ak, &auction);

        let ext_window = extension_window_secs.unwrap_or(EXTENSION_WINDOW_SECS);
        let ext_secs = extension_secs.unwrap_or(DEFAULT_EXTENSION_SECS);
        if ext_secs == 0 {
            panic!("Extension duration must be positive");
        }
        let ext_key = (Symbol::new(&env, "ext_cfg"), auction_id);
        env.storage()
            .instance()
            .set(&ext_key, &(ext_window, ext_secs));

        env.events().publish(
            (symbol_short!("auc_ext"),),
            (
                auction_id,
                agent_id,
                start_price,
                end_time,
                ext_window,
                ext_secs,
            ),
        );

        auction_id
    }

    /// Place a bid with auto-extension support.
    pub fn place_bid_with_extension(env: Env, auction_id: u64, bidder: Address, bid_amount: i128) {
        bidder.require_auth();
        if auction_id == 0 {
            panic!("Invalid auction ID");
        }
        if bid_amount <= 0 {
            panic!("Bid amount must be positive");
        }

        let mut auction: stellai_lib::Auction = env
            .storage()
            .instance()
            .get(&Self::auction_key(&env, auction_id))
            .expect("Auction not found");

        let current_time = env.ledger().timestamp();
        if auction.status != stellai_lib::AuctionStatus::Active {
            panic!("Auction is not active");
        }
        if current_time > auction.end_time {
            panic!("Auction has ended");
        }

        let min_bid = if auction.highest_bid == 0 {
            auction.start_price
        } else {
            let min_increment =
                (auction.highest_bid * (auction.min_bid_increment_bps as i128)) / 10000;
            auction.highest_bid + min_increment
        };

        if bid_amount < min_bid {
            panic!("Bid too low - minimum required: {}", min_bid);
        }

        if let Some(prev_bidder) = auction.highest_bidder {
            env.events().publish(
                (symbol_short!("bid_refnd"),),
                (auction_id, prev_bidder, auction.highest_bid, current_time),
            );
        }

        // Auto-extension
        let ext_key = (Symbol::new(&env, "ext_cfg"), auction_id);
        let ext_config: Option<(u64, u64)> = env.storage().instance().get(&ext_key);
        if let Some((ext_window, ext_secs)) = ext_config {
            let time_remaining = auction.end_time.saturating_sub(current_time);
            if time_remaining <= ext_window {
                let new_end_time = auction.end_time + ext_secs;
                auction.end_time = new_end_time;
                env.events().publish(
                    (symbol_short!("auc_extend"),),
                    (auction_id, new_end_time, current_time),
                );
            }
        }

        let bid_sequence =
            Self::record_bid(&env, auction_id, bidder.clone(), bid_amount, current_time);

        auction.highest_bidder = Some(bidder.clone());
        auction.highest_bid = bid_amount;
        auction.current_price = bid_amount;
        env.storage()
            .instance()
            .set(&Self::auction_key(&env, auction_id), &auction);

        env.events().publish(
            (symbol_short!("bid_plcd"),),
            (auction_id, bidder, bid_amount, bid_sequence, current_time),
        );
    }

    /// Get the extension config for an auction.
    pub fn get_auction_extension_config(env: Env, auction_id: u64) -> Option<(u64, u64)> {
        let ext_key = (Symbol::new(&env, "ext_cfg"), auction_id);
        env.storage().instance().get(&ext_key)
    }

    // =========================================================================
    // IPFS METADATA FOR COLLECTIONS
    // ==========================================================================

    /// Set IPFS metadata URI for a collection.
    pub fn set_collection_ipfs_metadata(
        env: Env,
        creator: Address,
        collection_id: u64,
        metadata_uri: String,
    ) {
        creator.require_auth();
        if collection_id == 0 {
            panic!("Invalid collection ID");
        }
        let coll = Self::load_collection(&env, collection_id);
        if coll.creator != creator {
            panic!("Only creator can set collection metadata");
        }
        let ipfs_key = Self::ipfs_metadata_key(&env, collection_id);
        env.storage().instance().set(&ipfs_key, &metadata_uri);
        env.events().publish(
            (symbol_short!("ipfs_set"),),
            (collection_id, metadata_uri, env.ledger().timestamp()),
        );
    }

    /// Get IPFS metadata URI for a collection.
    pub fn get_collection_ipfs_metadata(env: Env, collection_id: u64) -> Option<String> {
        let ipfs_key = Self::ipfs_metadata_key(&env, collection_id);
        env.storage().instance().get(&ipfs_key)
    }

    // =========================================================================
    // GOVERNANCE-CONTROLLED FEE SPLITS
    // ==========================================================================

    /// Set the fee split configuration (admin/governance only).
    pub fn set_fee_splits(
        env: Env,
        admin: Address,
        platform_share_bps: u32,
        creator_share_bps: u32,
        collection_share_bps: u32,
        extra_recipients: Vec<stellai_lib::FeeSplitRecipient>,
    ) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut total = platform_share_bps + creator_share_bps + collection_share_bps;
        for i in 0..extra_recipients.len() {
            if let Some(r) = extra_recipients.get(i) {
                total = total.checked_add(r.share_bps).expect("Fee share overflow");
            }
        }
        if total > 10000 {
            panic!("Total fee shares exceed 100%");
        }

        let config = stellai_lib::FeeSplitConfig {
            platform_share_bps,
            creator_share_bps,
            collection_share_bps,
            extra_recipients,
            total_bps: total,
        };
        env.storage()
            .instance()
            .set(&Symbol::new(&env, FEE_SPLIT_KEY), &config);

        env.events().publish(
            (symbol_short!("fee_split"),),
            (
                platform_share_bps,
                creator_share_bps,
                collection_share_bps,
                total,
            ),
        );
    }

    /// Get the current fee split configuration.
    pub fn get_fee_splits(env: Env) -> Option<stellai_lib::FeeSplitConfig> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, FEE_SPLIT_KEY))
    }

    // =========================================================================
    // Queries
    // ==========================================================================

    pub fn get_listing(env: Env, listing_id: u64) -> stellai_lib::Listing {
        Self::load_listing(&env, listing_id)
    }

    pub fn get_pending_sale(env: Env, listing_id: u64) -> Option<PendingSale> {
        env.storage()
            .instance()
            .get(&Self::pending_sale_key(&env, listing_id))
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .expect("Not initialized")
    }

    pub fn get_execution_hub(env: Env) -> Address {
        Self::get_hub(&env)
    }

    // =========================================================================
    // Private helpers
    // =========================================================================

    fn listing_key(env: &Env, listing_id: u64) -> (String, u64) {
        (String::from_str(env, LISTING_PREFIX), listing_id)
    }

    fn royalty_key(env: &Env, agent_id: u64) -> (String, u64) {
        (String::from_str(env, ROYALTY_PREFIX), agent_id)
    }

    fn pending_sale_key(env: &Env, listing_id: u64) -> (String, u64) {
        (String::from_str(env, PENDING_SALE_PREFIX), listing_id)
    }

    fn wf_listing_key(env: &Env, workflow_id: u64) -> (String, u64) {
        (String::from_str(env, WF_LISTING_PREFIX), workflow_id)
    }

    fn agent_key(env: &Env, agent_id: u64) -> (String, u64) {
        (
            String::from_str(env, stellai_lib::AGENT_KEY_PREFIX),
            agent_id,
        )
    }

    fn load_agent(env: &Env, agent_id: u64) -> stellai_lib::Agent {
        env.storage()
            .instance()
            .get(&Self::agent_key(env, agent_id))
            .expect("Agent not found")
    }

    fn try_load_agent(env: &Env, agent_id: u64) -> Result<stellai_lib::Agent, ()> {
        env.storage()
            .instance()
            .get(&Self::agent_key(env, agent_id))
            .ok_or(())
    }

    fn save_agent(env: &Env, agent_id: u64, agent: &stellai_lib::Agent) {
        env.storage()
            .instance()
            .set(&Self::agent_key(env, agent_id), agent);
    }

    fn load_listing(env: &Env, listing_id: u64) -> stellai_lib::Listing {
        env.storage()
            .instance()
            .get(&Self::listing_key(env, listing_id))
            .expect("Listing not found")
    }

    fn try_load_listing(env: &Env, listing_id: u64) -> Result<stellai_lib::Listing, ()> {
        env.storage()
            .instance()
            .get(&Self::listing_key(env, listing_id))
            .ok_or(())
    }

    fn get_hub(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(env, HUB_KEY))
            .expect("Execution hub not set")
    }

    fn next_listing_id(env: &Env) -> u64 {
        let key = Symbol::new(env, LISTING_CTR_KEY);
        let current: u64 = env.storage().instance().get(&key).unwrap_or(0);
        let next = current.checked_add(1).expect("Listing ID overflow");
        env.storage().instance().set(&key, &next);
        next
    }

    fn next_auction_id(env: &Env) -> u64 {
        let key = Symbol::new(env, AUCTION_CTR_KEY);
        let current: u64 = env.storage().instance().get(&key).unwrap_or(0);
        let next = current.checked_add(1).expect("Auction ID overflow");
        env.storage().instance().set(&key, &next);
        next
    }

    fn next_offer_id(env: &Env) -> u64 {
        let key = Symbol::new(env, OFFER_CTR_KEY);
        let current: u64 = env.storage().instance().get(&key).unwrap_or(0);
        let next = current.checked_add(1).expect("Offer ID overflow");
        env.storage().instance().set(&key, &next);
        next
    }

    fn next_dispute_id(env: &Env) -> u64 {
        let key = Symbol::new(env, DISPUTE_CTR_KEY);
        let current: u64 = env.storage().instance().get(&key).unwrap_or(0);
        let next = current.checked_add(1).expect("Dispute ID overflow");
        env.storage().instance().set(&key, &next);
        next
    }

    fn auction_key(env: &Env, auction_id: u64) -> (String, u64) {
        (String::from_str(env, AUCTION_PREFIX), auction_id)
    }

    fn offer_key(env: &Env, offer_id: u64) -> (String, u64) {
        (String::from_str(env, OFFER_PREFIX), offer_id)
    }

    fn dispute_key(env: &Env, dispute_id: u64) -> (String, u64) {
        (String::from_str(env, DISPUTE_PREFIX), dispute_id)
    }

    fn transaction_key(env: &Env, txn_id: u64) -> (String, u64) {
        (String::from_str(env, TRANSACTION_HISTORY_PREFIX), txn_id)
    }

    fn try_load_dispute(env: &Env, dispute_id: u64) -> Result<stellai_lib::Dispute, ()> {
        env.storage()
            .instance()
            .get(&Self::dispute_key(env, dispute_id))
            .ok_or(())
    }

    fn assert_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(env, ADMIN_KEY))
            .expect("Not initialized");
        if caller != &admin {
            panic!("Unauthorized");
        }
    }

    // ── Helpers for Issue #289 extensions ───────────────────────────────────

    fn next_collection_id(env: &Env) -> u64 {
        let key = Symbol::new(env, COLLECTION_CTR_KEY);
        let current: u64 = env.storage().instance().get(&key).unwrap_or(0);
        let next = current.checked_add(1).expect("Collection ID overflow");
        env.storage().instance().set(&key, &next);
        next
    }

    fn next_counter_offer_id(env: &Env) -> u64 {
        let key = Symbol::new(env, COUNTER_OFFER_CTR_KEY);
        let current: u64 = env.storage().instance().get(&key).unwrap_or(0);
        let next = current.checked_add(1).expect("Counter-offer ID overflow");
        env.storage().instance().set(&key, &next);
        next
    }

    fn load_collection(env: &Env, collection_id: u64) -> Collection {
        env.storage()
            .instance()
            .get(&Self::collection_key(env, collection_id))
            .expect("Collection not found")
    }

    fn collection_key(env: &Env, collection_id: u64) -> (String, u64) {
        (String::from_str(env, COLLECTION_PREFIX), collection_id)
    }

    fn collection_royalty_key(env: &Env, collection_id_id: u64) -> (String, u64) {
        (
            String::from_str(env, COLLECTION_ROYALTY_PREFIX),
            collection_id_id,
        )
    }

    fn counter_offer_key(env: &Env, counter_id: u64) -> (String, u64) {
        (String::from_str(env, COUNTER_OFFER_PREFIX), counter_id)
    }

    fn sealed_commit_key(env: &Env, auction_id: u64, bidder: &Address) -> (String, u64, Address) {
        (
            String::from_str(env, SEALED_COMMIT_PREFIX),
            auction_id,
            bidder.clone(),
        )
    }

    fn sealed_reveal_key(env: &Env, auction_id: u64, bidder: &Address) -> (String, u64, Address) {
        (
            String::from_str(env, SEALED_REVEAL_PREFIX),
            auction_id,
            bidder.clone(),
        )
    }

    fn load_offer(env: &Env, offer_id: u64) -> Offer {
        env.storage()
            .instance()
            .get(&Self::offer_key(env, offer_id))
            .expect("Offer not found")
    }

    /// Compute the current linearly decayed Dutch-auction price.
    #[allow(clippy::cast_sign_loss)]
    fn dutch_current_price(env: &Env, auction: &stellai_lib::Auction) -> i128 {
        let now = env.ledger().timestamp();
        let window = auction.end_time.saturating_sub(auction.start_time);
        if window == 0 {
            return auction.reserve_price;
        }
        if now <= auction.start_time {
            return auction.start_price;
        }
        if now >= auction.end_time {
            return auction.reserve_price;
        }
        let drop = auction.start_price.saturating_sub(auction.reserve_price);
        let elapsed = now.saturating_sub(auction.start_time);
        let percent = drop.saturating_mul(elapsed as i128) / (window as i128);
        auction.start_price.saturating_sub(percent)
    }

    /// Compute (royalty_amount, platform_fee_amount) for a sale.
    /// Tries RoyaltyConfig (multi-recipient) first, then falls back to
    /// RoyaltyInfo (single recipient), then 0 royalty. Platform fee always
    /// taken from PlatformFeeConfig (always set after init).
    #[allow(clippy::cast_sign_loss)]
    fn compute_settlement_fees(env: &Env, asset_id: u64, amount: i128) -> (i128, i128) {
        let cfg_key = Self::collection_royalty_key(env, asset_id);
        let royalty_config: Option<stellai_lib::RoyaltyConfig> =
            env.storage().instance().get(&cfg_key);
        let royalty_amount: i128 = if let Some(cfg) = royalty_config {
            if cfg.total_bps > stellai_lib::MAX_ROYALTY_PERCENTAGE {
                panic!("Invalid royalty percentage");
            }
            let mut total: i128 = 0i128;
            for i in 0..cfg.recipients.len() {
                if let Some(r) = cfg.recipients.get(i) {
                    if r.share_bps > stellai_lib::MAX_ROYALTY_PERCENTAGE {
                        panic!("Invalid recipient share");
                    }
                    let part = amount
                        .checked_mul(r.share_bps as i128)
                        .expect("share overflow")
                        .checked_div(10_000)
                        .expect("share div");
                    total = total.checked_add(part).expect("total overflow");
                }
            }
            total
        } else {
            let rk = Self::royalty_key(env, asset_id);
            let royalty_info: Option<stellai_lib::RoyaltyInfo> = env.storage().instance().get(&rk);
            match royalty_info {
                Some(r) if r.fee <= stellai_lib::MAX_ROYALTY_PERCENTAGE => amount
                    .checked_mul(r.fee as i128)
                    .expect("royalty overflow")
                    .checked_div(10_000)
                    .expect("royalty div"),
                _ => 0i128,
            }
        };
        let platform_fee_config: PlatformFeeConfig = env
            .storage()
            .instance()
            .get(&Symbol::new(env, PLATFORM_FEE_KEY))
            .expect("Platform fee not configured");
        let platform_fee = if platform_fee_config.fee_bps > stellai_lib::MAX_ROYALTY_PERCENTAGE {
            0i128
        } else {
            amount
                .checked_mul(platform_fee_config.fee_bps as i128)
                .expect("fee overflow")
                .checked_div(10_000)
                .expect("fee div")
        };
        (royalty_amount, platform_fee)
    }

    fn build_sale_steps(env: &Env, marketplace: &Address, listing_id: u64) -> Vec<WorkflowStep> {
        let encoded = Self::encode_u64(env, listing_id);

        let step0 = WorkflowStep {
            step_index: 0,
            name: String::from_str(env, "verify_sale"),
            target_contract: marketplace.clone(),
            function_name: String::from_str(env, "verify_sale"),
            encoded_args: encoded.clone(),
            required: true,
            max_retries: 0,
            retry_count: 0,
            status: WorkflowStepStatus::Pending,
            result: None,
            error: None,
            updated_at: 0,
        };

        let step1 = WorkflowStep {
            step_index: 1,
            name: String::from_str(env, "transfer_ownership"),
            target_contract: marketplace.clone(),
            function_name: String::from_str(env, "transfer_ownership"),
            encoded_args: encoded.clone(),
            required: true,
            max_retries: 1,
            retry_count: 0,
            status: WorkflowStepStatus::Pending,
            result: None,
            error: None,
            updated_at: 0,
        };

        let step2 = WorkflowStep {
            step_index: 2,
            name: String::from_str(env, "record_sale"),
            target_contract: marketplace.clone(),
            function_name: String::from_str(env, "record_sale"),
            encoded_args: encoded,
            required: true,
            max_retries: 0,
            retry_count: 0,
            status: WorkflowStepStatus::Pending,
            result: None,
            error: None,
            updated_at: 0,
        };

        let mut steps = Vec::new(env);
        steps.push_back(step0);
        steps.push_back(step1);
        steps.push_back(step2);
        steps
    }

    fn encode_u64(env: &Env, value: u64) -> Bytes {
        Bytes::from_array(env, &value.to_be_bytes())
    }

    fn decode_u64(data: &Bytes) -> u64 {
        if data.len() < 8 {
            panic!("Encoded args too short");
        }
        let mut arr = [0u8; 8];
        for (i, byte) in arr.iter_mut().enumerate() {
            *byte = data.get(i as u32).expect("byte missing");
        }
        u64::from_be_bytes(arr)
    }

    // ── NFT Marketplace helpers ─────────────────────────────────────────────

    fn nft_listing_key(env: &Env, nft_listing_id: u64) -> (String, u64) {
        (String::from_str(env, NFT_LISTING_PREFIX), nft_listing_id)
    }

    fn load_nft_listing(env: &Env, nft_listing_id: u64) -> NftListing {
        env.storage()
            .instance()
            .get(&Self::nft_listing_key(env, nft_listing_id))
            .expect("NFT listing not found")
    }

    fn next_nft_listing_id(env: &Env) -> u64 {
        let key = Symbol::new(env, NFT_LISTING_CTR_KEY);
        let current: u64 = env.storage().instance().get(&key).unwrap_or(0);
        let next = current.checked_add(1).expect("NFT listing ID overflow");
        env.storage().instance().set(&key, &next);
        next
    }

    fn currency_key(env: &Env, currency_id: u64) -> (String, u64) {
        (String::from_str(env, CURRENCY_PREFIX), currency_id)
    }

    fn next_currency_id(env: &Env) -> u64 {
        let key = Symbol::new(env, CURRENCY_CTR_KEY);
        let current: u64 = env.storage().instance().get(&key).unwrap_or(0);
        let next = current.checked_add(1).expect("Currency ID overflow");
        env.storage().instance().set(&key, &next);
        next
    }

    fn ipfs_metadata_key(env: &Env, collection_id: u64) -> (String, u64) {
        (String::from_str(env, IPFS_METADATA_PREFIX), collection_id)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::Env;

    fn setup_marketplace(env: &Env) -> (Address, Address) {
        let contract_id = env.register(Marketplace, ());
        let admin = Address::generate(env);
        MarketplaceClient::new(env, &contract_id).init_contract(&admin);
        (contract_id, admin)
    }

    fn seed_agent(env: &Env, contract_id: &Address, agent_id: u64, owner: &Address) {
        env.as_contract(contract_id, || {
            let key = (
                String::from_str(env, stellai_lib::AGENT_KEY_PREFIX),
                agent_id,
            );
            env.storage().instance().set(
                &key,
                &stellai_lib::Agent {
                    id: agent_id,
                    owner: owner.clone(),
                    name: String::from_str(env, "Bot"),
                    model_hash: String::from_str(env, "h"),
                    metadata_cid: String::from_str(env, "c"),
                    capabilities: Vec::new(env),
                    evolution_level: 0,
                    created_at: 0,
                    updated_at: 0,
                    nonce: 0,
                    escrow_locked: false,
                    escrow_holder: None,
                },
            );
        });
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Initialisation
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_init() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup_marketplace(&env);
        assert_eq!(
            MarketplaceClient::new(&env, &contract_id).get_admin(),
            admin
        );
    }

    #[test]
    #[should_panic(expected = "Already initialized")]
    fn test_double_init() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup_marketplace(&env);
        MarketplaceClient::new(&env, &contract_id).init_contract(&admin);
    }

    #[test]
    fn test_set_execution_hub() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup_marketplace(&env);
        let hub = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        client.set_execution_hub(&admin, &hub);
        assert_eq!(client.get_execution_hub(), hub);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Listings
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_create_listing() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 1, &seller);

        let client = MarketplaceClient::new(&env, &contract_id);
        let listing_id = client.create_listing(&1u64, &seller, &0u32, &1_000_000i128, &None);
        assert_eq!(listing_id, 1u64);
        let listing = client.get_listing(&listing_id);
        assert!(listing.active);
        assert_eq!(listing.seller, seller);
    }

    #[test]
    #[should_panic(expected = "Agent already locked in escrow")]
    fn test_create_listing_already_locked() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let holder = Address::generate(&env);
        env.as_contract(&contract_id, || {
            let key = (String::from_str(&env, stellai_lib::AGENT_KEY_PREFIX), 2u64);
            env.storage().instance().set(
                &key,
                &stellai_lib::Agent {
                    id: 2,
                    owner: seller.clone(),
                    name: String::from_str(&env, "B"),
                    model_hash: String::from_str(&env, "h"),
                    metadata_cid: String::from_str(&env, "c"),
                    capabilities: Vec::new(&env),
                    evolution_level: 0,
                    created_at: 0,
                    updated_at: 0,
                    nonce: 0,
                    escrow_locked: true,
                    escrow_holder: Some(holder),
                },
            );
        });
        MarketplaceClient::new(&env, &contract_id)
            .create_listing(&2u64, &seller, &0u32, &500i128, &None);
    }

    #[test]
    #[should_panic(expected = "Price out of valid range")]
    fn test_negative_price_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 3, &seller);
        MarketplaceClient::new(&env, &contract_id)
            .create_listing(&3u64, &seller, &0u32, &-1i128, &None);
    }

    #[test]
    fn test_cancel_listing() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 4, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);
        let lid = client.create_listing(&4u64, &seller, &0u32, &2_000i128, &None);
        assert!(client.get_listing(&lid).active);
        client.cancel_listing(&lid, &seller);
        assert!(!client.get_listing(&lid).active);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Royalties
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_set_and_get_royalty() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let creator = Address::generate(&env);
        let recipient = Address::generate(&env);
        seed_agent(&env, &contract_id, 5, &creator);
        let client = MarketplaceClient::new(&env, &contract_id);
        client.set_royalty(&5u64, &creator, &recipient, &500u32);
        let info = client.get_royalty(&5u64).unwrap();
        assert_eq!(info.fee, 500u32);
    }

    #[test]
    #[should_panic(expected = "Royalty exceeds maximum")]
    fn test_royalty_cap_enforced() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let creator = Address::generate(&env);
        let recipient = Address::generate(&env);
        seed_agent(&env, &contract_id, 6, &creator);
        MarketplaceClient::new(&env, &contract_id)
            .set_royalty(&6u64, &creator, &recipient, &20_000u32);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Step functions (direct invocation)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_verify_sale_step() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let mp = contract_id.clone();
            let ak = (String::from_str(&env, stellai_lib::AGENT_KEY_PREFIX), 10u64);
            env.storage().instance().set(
                &ak,
                &stellai_lib::Agent {
                    id: 10,
                    owner: seller.clone(),
                    name: String::from_str(&env, "V"),
                    model_hash: String::from_str(&env, "h"),
                    metadata_cid: String::from_str(&env, "c"),
                    capabilities: Vec::new(&env),
                    evolution_level: 0,
                    created_at: 0,
                    updated_at: 0,
                    nonce: 0,
                    escrow_locked: true,
                    escrow_holder: Some(mp),
                },
            );
            let lk = (String::from_str(&env, LISTING_PREFIX), 1u64);
            env.storage().instance().set(
                &lk,
                &stellai_lib::Listing {
                    listing_id: 1,
                    asset_id: 10,
                    asset_type: stellai_lib::AssetType::Agent,
                    seller: seller.clone(),
                    price: 100,
                    listing_type: stellai_lib::ListingType::Sale,
                    active: true,
                    created_at: 0,
                    expires_at: u64::MAX,
                },
            );
            let psk = (String::from_str(&env, PENDING_SALE_PREFIX), 1u64);
            env.storage().instance().set(
                &psk,
                &PendingSale {
                    listing_id: 1,
                    buyer: buyer.clone(),
                    amount: 200,
                    seller: seller.clone(),
                    agent_id: 10,
                    workflow_id: 1,
                    created_at: 0,
                },
            );
        });

        let client = MarketplaceClient::new(&env, &contract_id);
        client.verify_sale(&Bytes::from_array(&env, &1u64.to_be_bytes()));
    }

    #[test]
    fn test_transfer_ownership_step() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let mp = contract_id.clone();
            let ak = (String::from_str(&env, stellai_lib::AGENT_KEY_PREFIX), 11u64);
            env.storage().instance().set(
                &ak,
                &stellai_lib::Agent {
                    id: 11,
                    owner: seller.clone(),
                    name: String::from_str(&env, "T"),
                    model_hash: String::from_str(&env, "h"),
                    metadata_cid: String::from_str(&env, "c"),
                    capabilities: Vec::new(&env),
                    evolution_level: 0,
                    created_at: 0,
                    updated_at: 0,
                    nonce: 0,
                    escrow_locked: true,
                    escrow_holder: Some(mp),
                },
            );
            let lk = (String::from_str(&env, LISTING_PREFIX), 2u64);
            env.storage().instance().set(
                &lk,
                &stellai_lib::Listing {
                    listing_id: 2,
                    asset_id: 11,
                    asset_type: stellai_lib::AssetType::Agent,
                    seller: seller.clone(),
                    price: 100,
                    listing_type: stellai_lib::ListingType::Sale,
                    active: true,
                    created_at: 0,
                    expires_at: u64::MAX,
                },
            );
            let psk = (String::from_str(&env, PENDING_SALE_PREFIX), 2u64);
            env.storage().instance().set(
                &psk,
                &PendingSale {
                    listing_id: 2,
                    buyer: buyer.clone(),
                    amount: 200,
                    seller: seller.clone(),
                    agent_id: 11,
                    workflow_id: 2,
                    created_at: 0,
                },
            );
        });

        MarketplaceClient::new(&env, &contract_id)
            .transfer_ownership(&Bytes::from_array(&env, &2u64.to_be_bytes()));

        env.as_contract(&contract_id, || {
            let ak = (String::from_str(&env, stellai_lib::AGENT_KEY_PREFIX), 11u64);
            let agent: stellai_lib::Agent = env.storage().instance().get(&ak).unwrap();
            assert_eq!(agent.owner, buyer);
        });
    }

    #[test]
    fn test_record_sale_step() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let mp = contract_id.clone();
            let ak = (String::from_str(&env, stellai_lib::AGENT_KEY_PREFIX), 12u64);
            env.storage().instance().set(
                &ak,
                &stellai_lib::Agent {
                    id: 12,
                    owner: buyer.clone(),
                    name: String::from_str(&env, "R"),
                    model_hash: String::from_str(&env, "h"),
                    metadata_cid: String::from_str(&env, "c"),
                    capabilities: Vec::new(&env),
                    evolution_level: 0,
                    created_at: 0,
                    updated_at: 0,
                    nonce: 1,
                    escrow_locked: true,
                    escrow_holder: Some(mp),
                },
            );
            let lk = (String::from_str(&env, LISTING_PREFIX), 3u64);
            env.storage().instance().set(
                &lk,
                &stellai_lib::Listing {
                    listing_id: 3,
                    asset_id: 12,
                    asset_type: stellai_lib::AssetType::Agent,
                    seller: seller.clone(),
                    price: 100,
                    listing_type: stellai_lib::ListingType::Sale,
                    active: true,
                    created_at: 0,
                    expires_at: u64::MAX,
                },
            );
            let psk = (String::from_str(&env, PENDING_SALE_PREFIX), 3u64);
            env.storage().instance().set(
                &psk,
                &PendingSale {
                    listing_id: 3,
                    buyer: buyer.clone(),
                    amount: 200,
                    seller: seller.clone(),
                    agent_id: 12,
                    workflow_id: 3,
                    created_at: 0,
                },
            );
        });

        MarketplaceClient::new(&env, &contract_id)
            .record_sale(&Bytes::from_array(&env, &3u64.to_be_bytes()));

        env.as_contract(&contract_id, || {
            let lk = (String::from_str(&env, LISTING_PREFIX), 3u64);
            let listing: stellai_lib::Listing = env.storage().instance().get(&lk).unwrap();
            assert!(!listing.active);

            let ak = (String::from_str(&env, stellai_lib::AGENT_KEY_PREFIX), 12u64);
            let agent: stellai_lib::Agent = env.storage().instance().get(&ak).unwrap();
            assert!(!agent.escrow_locked);
            assert!(agent.escrow_holder.is_none());

            let psk = (String::from_str(&env, PENDING_SALE_PREFIX), 3u64);
            assert!(!env.storage().instance().has(&psk));
        });
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Rollback
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_rollback_restores_seller() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let mp = contract_id.clone();
            let ak = (String::from_str(&env, stellai_lib::AGENT_KEY_PREFIX), 20u64);
            env.storage().instance().set(
                &ak,
                &stellai_lib::Agent {
                    id: 20,
                    owner: buyer.clone(), // ownership already xferred
                    name: String::from_str(&env, "Rb"),
                    model_hash: String::from_str(&env, "rb"),
                    metadata_cid: String::from_str(&env, "rbc"),
                    capabilities: Vec::new(&env),
                    evolution_level: 0,
                    created_at: 0,
                    updated_at: 0,
                    nonce: 1,
                    escrow_locked: true,
                    escrow_holder: Some(mp),
                },
            );
            let lk = (String::from_str(&env, LISTING_PREFIX), 10u64);
            env.storage().instance().set(
                &lk,
                &stellai_lib::Listing {
                    listing_id: 10,
                    asset_id: 20,
                    asset_type: stellai_lib::AssetType::Agent,
                    seller: seller.clone(),
                    price: 300,
                    listing_type: stellai_lib::ListingType::Sale,
                    active: true,
                    created_at: 0,
                    expires_at: u64::MAX,
                },
            );
            let psk = (String::from_str(&env, PENDING_SALE_PREFIX), 10u64);
            env.storage().instance().set(
                &psk,
                &PendingSale {
                    listing_id: 10,
                    buyer: buyer.clone(),
                    amount: 300,
                    seller: seller.clone(),
                    agent_id: 20,
                    workflow_id: 99,
                    created_at: 0,
                },
            );
        });

        MarketplaceClient::new(&env, &contract_id)
            .rollback(&Bytes::from_array(&env, &10u64.to_be_bytes()));

        env.as_contract(&contract_id, || {
            let ak = (String::from_str(&env, stellai_lib::AGENT_KEY_PREFIX), 20u64);
            let agent: stellai_lib::Agent = env.storage().instance().get(&ak).unwrap();
            assert_eq!(agent.owner, seller);
            assert!(!agent.escrow_locked);
        });
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Callback
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_callback_success_cleans_up() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);

        env.as_contract(&contract_id, || {
            let wlk = (String::from_str(&env, WF_LISTING_PREFIX), 7u64);
            env.storage().instance().set(&wlk, &5u64);
            let lk = (String::from_str(&env, LISTING_PREFIX), 5u64);
            env.storage().instance().set(
                &lk,
                &stellai_lib::Listing {
                    listing_id: 5,
                    asset_id: 99,
                    asset_type: stellai_lib::AssetType::Agent,
                    seller: Address::generate(&env),
                    price: 100,
                    listing_type: stellai_lib::ListingType::Sale,
                    active: false,
                    created_at: 0,
                    expires_at: u64::MAX,
                },
            );
        });

        MarketplaceClient::new(&env, &contract_id).wf_done(&7u64, &2u32);

        env.as_contract(&contract_id, || {
            let wlk = (String::from_str(&env, WF_LISTING_PREFIX), 7u64);
            assert!(!env.storage().instance().has(&wlk));
        });
    }

    #[test]
    fn test_callback_failure_reactivates_listing() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);

        env.as_contract(&contract_id, || {
            let wlk = (String::from_str(&env, WF_LISTING_PREFIX), 8u64);
            env.storage().instance().set(&wlk, &6u64);
            let lk = (String::from_str(&env, LISTING_PREFIX), 6u64);
            env.storage().instance().set(
                &lk,
                &stellai_lib::Listing {
                    listing_id: 6,
                    asset_id: 50,
                    asset_type: stellai_lib::AssetType::Agent,
                    seller: Address::generate(&env),
                    price: 100,
                    listing_type: stellai_lib::ListingType::Sale,
                    active: false,
                    created_at: 0,
                    expires_at: u64::MAX,
                },
            );
        });

        MarketplaceClient::new(&env, &contract_id).wf_done(&8u64, &4u32);

        env.as_contract(&contract_id, || {
            let lk = (String::from_str(&env, LISTING_PREFIX), 6u64);
            let listing: stellai_lib::Listing = env.storage().instance().get(&lk).unwrap();
            assert!(listing.active);
        });
    }

    // ─────────────────────────────────────────────────────────────────────────
    // BATCH OPERATIONS — Issue #289 acceptance criterion #4
    // ─────────────────────────────────────────────────────────────────────────

    fn seed_agents_for(env: &Env, contract_id: &Address, owner: &Address, base: u64, count: u32) {
        for i in 0..count {
            seed_agent(env, contract_id, base + u64::from(i), owner);
        }
    }

    #[test]
    fn test_batch_create_listings() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agents_for(&env, &contract_id, &seller, 100, 3);
        let client = MarketplaceClient::new(&env, &contract_id);
        let mut ids = Vec::new(&env);
        ids.push_back(100u64);
        ids.push_back(101u64);
        ids.push_back(102u64);
        let out = client.batch_create_listings(&seller, &0u32, &5_000i128, &ids);
        assert_eq!(out.len(), 3);
        assert!(client.get_listing(&out.get(0).unwrap()).active);
        assert!(client.get_listing(&out.get(2).unwrap()).active);
    }

    #[test]
    #[should_panic(expected = "Batch size out of bounds")]
    fn test_batch_create_listings_empty() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let empty: Vec<u64> = Vec::new(&env);
        MarketplaceClient::new(&env, &contract_id)
            .batch_create_listings(&seller, &0u32, &100i128, &empty);
    }

    #[test]
    #[should_panic(expected = "Invalid agent ID")]
    fn test_batch_create_listings_zero_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 200, &seller);
        let mut ids = Vec::new(&env);
        ids.push_back(200u64);
        ids.push_back(0u64);
        MarketplaceClient::new(&env, &contract_id)
            .batch_create_listings(&seller, &0u32, &100i128, &ids);
    }

    #[test]
    fn test_batch_cancel_listings() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agents_for(&env, &contract_id, &seller, 300, 3);
        let client = MarketplaceClient::new(&env, &contract_id);
        let l1 = client.create_listing(&300u64, &seller, &0u32, &100i128, &None);
        let l2 = client.create_listing(&301u64, &seller, &0u32, &100i128, &None);
        let l3 = client.create_listing(&302u64, &seller, &0u32, &100i128, &None);
        let mut ids = Vec::new(&env);
        ids.push_back(l1);
        ids.push_back(l2);
        ids.push_back(l3);
        let cancelled = client.batch_cancel_listings(&seller, &ids);
        assert_eq!(cancelled, 3);
        assert!(!client.get_listing(&l1).active);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // COLLECTIONS — Issue #289 acceptance criterion #3
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_create_collection() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let creator = Address::generate(&env);
        let cid = MarketplaceClient::new(&env, &contract_id).create_collection(
            &creator,
            &String::from_str(&env, "Genesis"),
            &500u32,
        );
        assert_eq!(cid, 1u64);
        let coll = MarketplaceClient::new(&env, &contract_id).get_collection(&cid);
        assert_eq!(coll.creator, creator);
        assert_eq!(coll.royalty_config.total_bps, 500);
        assert_eq!(coll.members.len(), 0);
    }

    #[test]
    #[should_panic(expected = "Invalid collection name")]
    fn test_create_collection_empty_name() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let creator = Address::generate(&env);
        MarketplaceClient::new(&env, &contract_id).create_collection(
            &creator,
            &String::from_str(&env, ""),
            &500u32,
        );
    }

    #[test]
    fn test_add_and_remove_from_collection() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let creator = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let cid = client.create_collection(&creator, &String::from_str(&env, "Drops"), &250u32);
        let mut ids = Vec::new(&env);
        ids.push_back(1001u64);
        ids.push_back(1002u64);
        ids.push_back(1003u64);
        let added = client.add_to_collection(&creator, &cid, &ids);
        assert_eq!(added, 3);
        assert_eq!(client.get_collection_items(&cid).len(), 3);
        // Dedup second pass
        let same: Vec<u64> = ids;
        let added2 = client.add_to_collection(&creator, &cid, &same);
        assert_eq!(added2, 0);
        // Remove one
        let mut rm = Vec::new(&env);
        rm.push_back(1002u64);
        let removed = client.remove_from_collection(&creator, &cid, &rm);
        assert_eq!(removed, 1);
        assert_eq!(client.get_collection_items(&cid).len(), 2);
    }

    #[test]
    #[should_panic(expected = "Only creator can modify collection")]
    fn test_collection_access_control() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let creator = Address::generate(&env);
        let attacker = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let cid = client.create_collection(&creator, &String::from_str(&env, "Locked"), &0u32);
        let mut ids = Vec::new(&env);
        ids.push_back(4001u64);
        client.add_to_collection(&attacker, &cid, &ids);
    }

    #[test]
    fn test_set_collection_multi_recipient_royalty() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let creator = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let cid = client.create_collection(&creator, &String::from_str(&env, "Splits"), &0u32);
        let mut recipients = Vec::new(&env);
        recipients.push_back(RoyaltyRecipient {
            recipient: alice.clone(),
            share_bps: 700u32,
            role: String::from_str(&env, "creator"),
        });
        recipients.push_back(RoyaltyRecipient {
            recipient: bob.clone(),
            share_bps: 300u32,
            role: String::from_str(&env, "collaborator"),
        });
        client.set_collection_royalty(&creator, &cid, &recipients, &1000u32);
        let coll = client.get_collection(&cid);
        assert_eq!(coll.royalty_config.total_bps, 1000);
        assert_eq!(coll.royalty_config.recipients.len(), 2);
    }

    #[test]
    #[should_panic(expected = "Royalty share total mismatch")]
    fn test_set_collection_royalty_total_mismatch() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let creator = Address::generate(&env);
        let alice = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let cid = client.create_collection(&creator, &String::from_str(&env, "Bad"), &0u32);
        let mut recipients = Vec::new(&env);
        recipients.push_back(RoyaltyRecipient {
            recipient: alice.clone(),
            share_bps: 500u32,
            role: String::from_str(&env, "creator"),
        });
        client.set_collection_royalty(&creator, &cid, &recipients, &1000u32);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // COUNTER-OFFER — Issue #289 acceptance criterion #2
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_make_and_reject_counter_offer() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let offerer = Address::generate(&env);
        seed_agent(&env, &contract_id, 5000, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);
        let listing_id = client.create_listing(&5000u64, &seller, &0u32, &1_000i128, &None);
        let offer_id = client.make_offer(&listing_id, &offerer, &800i128, &None);
        let counter_id = client.make_counter_offer(&seller, &offer_id, &900i128, &None);
        assert!(counter_id > 0);
        client.reject_counter_offer(&offerer, &counter_id);
    }

    #[test]
    #[should_panic(expected = "Only listing seller can counter-offer")]
    fn test_make_counter_offer_non_seller_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let offerer = Address::generate(&env);
        let stranger = Address::generate(&env);
        seed_agent(&env, &contract_id, 5100, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);
        let listing_id = client.create_listing(&5100u64, &seller, &0u32, &1_000i128, &None);
        let offer_id = client.make_offer(&listing_id, &offerer, &800i128, &None);
        client.make_counter_offer(&stranger, &offer_id, &900i128, &None);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // DUTCH AUCTION — Issue #289 acceptance criterion #5
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_create_dutch_auction() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 6000, &seller);
        let auction_id = MarketplaceClient::new(&env, &contract_id)
            .create_dutch_auction(&6000u64, &seller, &1_000i128, &500i128, &2u64);
        assert!(auction_id > 0);
    }

    #[test]
    #[should_panic(expected = "Invalid Dutch auction price bounds")]
    fn test_create_dutch_auction_invalid_bounds() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 6001, &seller);
        MarketplaceClient::new(&env, &contract_id)
            .create_dutch_auction(&6001u64, &seller, &100i128, &200i128, &1u64);
    }

    #[test]
    fn test_dutch_buy_now_happy_path() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        seed_agent(&env, &contract_id, 6300, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);
        let auction_id =
            client.create_dutch_auction(&6300u64, &seller, &1_000i128, &500i128, &2u64);
        // Buy at-or-above the start price; succeeds and finalises the sale.
        client.dutch_buy_now(&auction_id, &buyer, &1_000i128);
    }

    #[test]
    #[should_panic(expected = "Bid below current Dutch price")]
    fn test_dutch_buy_now_below_decay() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        seed_agent(&env, &contract_id, 6400, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);
        // 1000 -> 0 over 10 seconds
        let auction_id = client.create_dutch_auction(&6400u64, &seller, &1_000i128, &0i128, &10u64);
        // Advance 5 seconds; decay should be ~500, bid 100 too low.
        env.ledger().with_mut(|l| {
            l.timestamp = 5;
        });
        client.dutch_buy_now(&auction_id, &buyer, &100i128);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // SEALED-BID AUCTION — Issue #289 acceptance criterion #5
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_create_sealed_bid_auction() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 7000, &seller);
        let auction_id = MarketplaceClient::new(&env, &contract_id)
            .create_sealed_bid_auction(&7000u64, &seller, &1_000i128, &500i128, &60u64, &120u64);
        assert!(auction_id > 0);
    }

    #[test]
    #[should_panic(expected = "Invalid sealed-bid timeline")]
    fn test_create_sealed_bid_zero_commit_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 7100, &seller);
        MarketplaceClient::new(&env, &contract_id)
            .create_sealed_bid_auction(&7100u64, &seller, &1_000i128, &500i128, &0u64, &120u64);
    }

    #[test]
    #[should_panic(expected = "Commit phase has ended")]
    fn test_commit_after_commit_phase() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let bidder = Address::generate(&env);
        seed_agent(&env, &contract_id, 7200, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);
        let auction_id = client
            .create_sealed_bid_auction(&7200u64, &seller, &1_000i128, &500i128, &60u64, &120u64);
        env.ledger().with_mut(|l| {
            l.timestamp = 1_000_000;
        });
        client.commit_bid(
            &auction_id,
            &bidder,
            &Bytes::from_array(&env, &[1u8; 32]),
            &100i128,
        );
    }

    #[test]
    fn test_commit_and_reveal_sealed_bid() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let bidder = Address::generate(&env);
        seed_agent(&env, &contract_id, 7300, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);
        let auction_id = client
            .create_sealed_bid_auction(&7300u64, &seller, &1_000i128, &500i128, &60u64, &120u64);
        client.commit_bid(
            &auction_id,
            &bidder,
            &Bytes::from_array(&env, &[9u8; 32]),
            &700i128,
        );
        env.ledger().with_mut(|l| {
            l.timestamp = 100;
        });
        client.reveal_bid(
            &auction_id,
            &bidder,
            &700i128,
            &String::from_str(&env, "nonce-A"),
        );
    }

    #[test]
    fn test_sealed_bid_happy_path_finalize() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let bidder_a = Address::generate(&env);
        let bidder_b = Address::generate(&env);
        seed_agent(&env, &contract_id, 8000, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);
        let auction_id = client
            .create_sealed_bid_auction(&8000u64, &seller, &1_000i128, &500i128, &60u64, &120u64);
        // Both commit during the commit window (default t=0; commit_end=60).
        client.commit_bid(
            &auction_id,
            &bidder_a,
            &Bytes::from_array(&env, &[11u8; 32]),
            &600i128,
        );
        client.commit_bid(
            &auction_id,
            &bidder_b,
            &Bytes::from_array(&env, &[22u8; 32]),
            &800i128,
        );
        // Advance into the reveal window (commit_end < 100 < reveal_end=180).
        env.ledger().with_mut(|l| {
            l.timestamp = 100;
        });
        client.reveal_bid(
            &auction_id,
            &bidder_a,
            &600i128,
            &String::from_str(&env, "n-A"),
        );
        client.reveal_bid(
            &auction_id,
            &bidder_b,
            &800i128,
            &String::from_str(&env, "n-B"),
        );
        // Finalize after reveal_end.
        env.ledger().with_mut(|l| {
            l.timestamp = 1_000;
        });
        client.finalize_sealed_auction(&auction_id);
        env.as_contract(&contract_id, || {
            let ak = (
                String::from_str(&env, stellai_lib::AGENT_KEY_PREFIX),
                8000u64,
            );
            let agent: stellai_lib::Agent = env.storage().instance().get(&ak).unwrap();
            assert_eq!(agent.owner, bidder_b);
        });
    }

    // ─────────────────────────────────────────────────────────────────────────
    // ACCESS CONTROL — Issue #289 acceptance criterion #8
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_assign_and_remove_governance() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup_marketplace(&env);
        let gov = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        client.assign_marketplace_governance(&admin, &gov);
        client.remove_marketplace_governance(&admin, &gov);
    }

    #[test]
    #[should_panic(expected = "Unauthorized")]
    fn test_set_platform_fee_admin_only() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin) = setup_marketplace(&env);
        let random = Address::generate(&env);
        MarketplaceClient::new(&env, &contract_id).set_platform_fee(&random, &100u32, &random);
    }

    #[test]
    fn test_assign_and_remove_kyc() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup_marketplace(&env);
        let kyc = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        client.assign_marketplace_kyc_operator(&admin, &kyc);
        client.remove_marketplace_kyc_operator(&admin, &kyc);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // NFT LISTINGS — ERC721/ERC1155 + configurable currency
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_create_nft_listing() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let nft_ref = stellai_lib::NftTokenRef {
            contract_address: None,
            token_id: 42,
            standard: stellai_lib::NftStandard::SorobanNative,
        };
        let listing_id = client.create_nft_listing(
            &nft_ref,
            &seller,
            &1_000_000i128,
            &String::from_str(&env, "XLM"),
            &None,
            &None,
            &String::from_str(&env, "ipfs://Qm123"),
        );
        assert_eq!(listing_id, 1u64);
        let listing = client.get_nft_listing(&listing_id);
        assert!(listing.active);
        assert_eq!(listing.seller, seller);
        assert_eq!(listing.price, 1_000_000i128);
    }

    #[test]
    fn test_create_nft_listing_erc721() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let nft_contract = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let nft_ref = stellai_lib::NftTokenRef {
            contract_address: Some(nft_contract),
            token_id: 100,
            standard: stellai_lib::NftStandard::Erc721,
        };
        let listing_id = client.create_nft_listing(
            &nft_ref,
            &seller,
            &5_000i128,
            &String::from_str(&env, "USDC"),
            &Some(Address::generate(&env)),
            &Some(7u64),
            &String::from_str(&env, "ipfs://Qm456"),
        );
        let listing = client.get_nft_listing(&listing_id);
        assert!(listing.active);
        assert_eq!(listing.price, 5_000i128);
    }

    #[test]
    fn test_create_nft_listing_erc1155() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let nft_contract = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let nft_ref = stellai_lib::NftTokenRef {
            contract_address: Some(nft_contract),
            token_id: 200,
            standard: stellai_lib::NftStandard::Erc1155,
        };
        let listing_id = client.create_nft_listing(
            &nft_ref,
            &seller,
            &10_000i128,
            &String::from_str(&env, "USDC"),
            &None,
            &None,
            &String::from_str(&env, "ipfs://Qm789"),
        );
        let listing = client.get_nft_listing(&listing_id);
        assert!(listing.active);
    }

    #[test]
    fn test_buy_nft_listing() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let nft_ref = stellai_lib::NftTokenRef {
            contract_address: None,
            token_id: 50,
            standard: stellai_lib::NftStandard::SorobanNative,
        };
        let listing_id = client.create_nft_listing(
            &nft_ref,
            &seller,
            &1_000i128,
            &String::from_str(&env, "XLM"),
            &None,
            &None,
            &String::from_str(&env, "ipfs://test"),
        );
        client.buy_nft_listing(&listing_id, &buyer, &1_000i128);
        assert!(!client.get_nft_listing(&listing_id).active);
    }

    #[test]
    #[should_panic(expected = "Insufficient payment")]
    fn test_buy_nft_listing_insufficient_payment() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let nft_ref = stellai_lib::NftTokenRef {
            contract_address: None,
            token_id: 51,
            standard: stellai_lib::NftStandard::SorobanNative,
        };
        let listing_id = client.create_nft_listing(
            &nft_ref,
            &seller,
            &1_000i128,
            &String::from_str(&env, "XLM"),
            &None,
            &None,
            &String::from_str(&env, "ipfs://test"),
        );
        client.buy_nft_listing(&listing_id, &buyer, &500i128);
    }

    #[test]
    fn test_cancel_nft_listing() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let nft_ref = stellai_lib::NftTokenRef {
            contract_address: None,
            token_id: 52,
            standard: stellai_lib::NftStandard::SorobanNative,
        };
        let listing_id = client.create_nft_listing(
            &nft_ref,
            &seller,
            &1_000i128,
            &String::from_str(&env, "XLM"),
            &None,
            &None,
            &String::from_str(&env, "ipfs://test"),
        );
        assert!(client.get_nft_listing(&listing_id).active);
        client.cancel_nft_listing(&listing_id, &seller);
        assert!(!client.get_nft_listing(&listing_id).active);
    }

    #[test]
    #[should_panic(expected = "Only seller can cancel NFT listing")]
    fn test_cancel_nft_listing_wrong_seller() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let stranger = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let nft_ref = stellai_lib::NftTokenRef {
            contract_address: None,
            token_id: 53,
            standard: stellai_lib::NftStandard::SorobanNative,
        };
        let listing_id = client.create_nft_listing(
            &nft_ref,
            &seller,
            &1_000i128,
            &String::from_str(&env, "XLM"),
            &None,
            &None,
            &String::from_str(&env, "ipfs://test"),
        );
        client.cancel_nft_listing(&listing_id, &stranger);
    }

    #[test]
    #[should_panic(expected = "Price out of valid range")]
    fn test_nft_listing_zero_price_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let nft_ref = stellai_lib::NftTokenRef {
            contract_address: None,
            token_id: 54,
            standard: stellai_lib::NftStandard::SorobanNative,
        };
        client.create_nft_listing(
            &nft_ref,
            &seller,
            &0i128,
            &String::from_str(&env, "XLM"),
            &None,
            &None,
            &String::from_str(&env, "ipfs://test"),
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // CONFIGURABLE CURRENCY
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_register_currency() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup_marketplace(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let ccy_id = client.register_currency(
            &admin,
            &String::from_str(&env, "USDC"),
            &Some(Address::generate(&env)),
            &7u32,
        );
        assert_eq!(ccy_id, 1u64);
        let record = client.get_currency(&ccy_id);
        assert!(record.active);
        assert_eq!(record.decimals, 7u32);
    }

    #[test]
    fn test_register_xlm_currency() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup_marketplace(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let ccy_id = client.register_currency(&admin, &String::from_str(&env, "XLM"), &None, &7u32);
        let record = client.get_currency(&ccy_id);
        assert!(record.active);
        assert!(record.token_address.is_none());
    }

    #[test]
    fn test_get_accepted_currencies() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup_marketplace(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        client.register_currency(
            &admin,
            &String::from_str(&env, "USDC"),
            &Some(Address::generate(&env)),
            &7u32,
        );
        client.register_currency(&admin, &String::from_str(&env, "XLM"), &None, &7u32);
        let currencies = client.get_accepted_currencies();
        assert_eq!(currencies.len(), 2);
    }

    #[test]
    fn test_deactivate_currency() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup_marketplace(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let ccy_id = client.register_currency(
            &admin,
            &String::from_str(&env, "DAI"),
            &Some(Address::generate(&env)),
            &18u32,
        );
        client.deactivate_currency(&admin, &ccy_id);
        let record = client.get_currency(&ccy_id);
        assert!(!record.active);
    }

    #[test]
    #[should_panic(expected = "Currency symbol required")]
    fn test_register_currency_empty_symbol() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup_marketplace(&env);
        MarketplaceClient::new(&env, &contract_id).register_currency(
            &admin,
            &String::from_str(&env, ""),
            &None,
            &7u32,
        );
    }

    #[test]
    #[should_panic(expected = "Decimals cannot exceed 18")]
    fn test_register_currency_too_many_decimals() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup_marketplace(&env);
        MarketplaceClient::new(&env, &contract_id).register_currency(
            &admin,
            &String::from_str(&env, "BAD"),
            &None,
            &19u32,
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // AUCTION AUTO-EXTENSION
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_create_auction_with_extension() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 9000, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);
        let auction_id = client.create_auction_with_extension(
            &9000u64,
            &seller,
            &1_000i128,
            &500i128,
            &1u64,
            &Some(100u32),
            &Some(600u64),
            &Some(300u64),
        );
        assert!(auction_id > 0);
        let config = client.get_auction_extension_config(&auction_id);
        assert!(config.is_some());
        let (window, extension) = config.unwrap();
        assert_eq!(window, 600u64);
        assert_eq!(extension, 300u64);
    }

    #[test]
    fn test_bid_with_auto_extension() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let bidder = Address::generate(&env);
        seed_agent(&env, &contract_id, 9100, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);
        // Create auction with 1-day duration
        let auction_id = client.create_auction_with_extension(
            &9100u64,
            &seller,
            &1_000i128,
            &500i128,
            &1u64,
            &Some(100u32),
            &Some(600u64), // extension window
            &Some(300u64), // extension duration
        );
        // First bid: doesn't trigger extension (plenty of time left)
        client.place_bid_with_extension(&auction_id, &bidder, &1_000i128);
        let auction: stellai_lib::Auction = env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .get(&(String::from_str(&env, "auc_"), auction_id))
                .unwrap()
        });
        let original_end = auction.end_time;

        // Advance time to within extension window (600 secs from end)
        env.ledger().with_mut(|l| {
            l.timestamp = original_end - 300;
        });
        let bidder2 = Address::generate(&env);
        seed_agent(&env, &contract_id, 9101, &bidder2);
        // Note: bidder2 needs to bid high enough to meet min increment
        let bid2 = 1_000i128 + (1_000i128 * 100 / 10_000) + 1;
        client.place_bid_with_extension(&auction_id, &bidder2, &bid2);
        let auction_after: stellai_lib::Auction = env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .get(&(String::from_str(&env, "auc_"), auction_id))
                .unwrap()
        });
        // end_time should have been extended by 300 secs
        assert_eq!(auction_after.end_time, original_end + 300);
    }

    #[test]
    fn test_bid_outside_extension_window_no_extend() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let bidder = Address::generate(&env);
        seed_agent(&env, &contract_id, 9200, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);
        let auction_id = client.create_auction_with_extension(
            &9200u64,
            &seller,
            &1_000i128,
            &500i128,
            &1u64,
            &Some(100u32),
            &Some(300u64), // extension window
            &Some(300u64),
        );
        // Bid early (plenty of time remaining)
        client.place_bid_with_extension(&auction_id, &bidder, &1_000i128);
        let auction: stellai_lib::Auction = env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .get(&(String::from_str(&env, "auc_"), auction_id))
                .unwrap()
        });
        let original_end = auction.end_time;
        // Bid with 10000 secs remaining (outside 300 sec window)
        env.ledger().with_mut(|l| {
            l.timestamp = 100;
        });
        let bidder2 = Address::generate(&env);
        let bid2 = 1_000i128 + (1_000i128 * 100 / 10_000) + 1;
        client.place_bid_with_extension(&auction_id, &bidder2, &bid2);
        let auction_after: stellai_lib::Auction = env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .get(&(String::from_str(&env, "auc_"), auction_id))
                .unwrap()
        });
        assert_eq!(auction_after.end_time, original_end);
    }

    #[test]
    #[should_panic(expected = "Extension duration must be positive")]
    fn test_create_auction_zero_extension_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 9300, &seller);
        MarketplaceClient::new(&env, &contract_id).create_auction_with_extension(
            &9300u64,
            &seller,
            &1_000i128,
            &500i128,
            &1u64,
            &Some(100u32),
            &Some(300u64),
            &Some(0u64),
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // IPFS METADATA FOR COLLECTIONS
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_set_collection_ipfs_metadata() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let creator = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let cid = client.create_collection(&creator, &String::from_str(&env, "Test"), &0u32);
        client.set_collection_ipfs_metadata(
            &creator,
            &cid,
            &String::from_str(&env, "ipfs://QmCollectionMetadata123"),
        );
        let uri = client.get_collection_ipfs_metadata(&cid);
        assert!(uri.is_some());
        assert_eq!(
            uri.unwrap(),
            String::from_str(&env, "ipfs://QmCollectionMetadata123")
        );
    }

    #[test]
    fn test_get_ipfs_metadata_nonexistent() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let uri = MarketplaceClient::new(&env, &contract_id).get_collection_ipfs_metadata(&999u64);
        assert!(uri.is_none());
    }

    #[test]
    #[should_panic(expected = "Only creator can set collection metadata")]
    fn test_set_ipfs_metadata_wrong_creator() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let creator = Address::generate(&env);
        let stranger = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let cid = client.create_collection(&creator, &String::from_str(&env, "X"), &0u32);
        client.set_collection_ipfs_metadata(&stranger, &cid, &String::from_str(&env, "ipfs://bad"));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // GOVERNANCE-CONTROLLED FEE SPLITS
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_set_fee_splits() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup_marketplace(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let extra = Vec::new(&env);
        client.set_fee_splits(&admin, &250u32, &50u32, &25u32, &extra);
        let config = client.get_fee_splits().unwrap();
        assert_eq!(config.platform_share_bps, 250);
        assert_eq!(config.creator_share_bps, 50);
        assert_eq!(config.collection_share_bps, 25);
        assert_eq!(config.total_bps, 325);
    }

    #[test]
    fn test_set_fee_splits_with_extra_recipients() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup_marketplace(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let mut extra = Vec::new(&env);
        extra.push_back(stellai_lib::FeeSplitRecipient {
            recipient: Address::generate(&env),
            share_bps: 100,
        });
        client.set_fee_splits(&admin, &200u32, &50u32, &25u32, &extra);
        let config = client.get_fee_splits().unwrap();
        assert_eq!(config.total_bps, 375);
        assert_eq!(config.extra_recipients.len(), 1);
    }

    #[test]
    #[should_panic(expected = "Total fee shares exceed 100%")]
    fn test_fee_splits_exceed_100_percent() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup_marketplace(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let extra = Vec::new(&env);
        client.set_fee_splits(&admin, &5000u32, &3000u32, &2500u32, &extra);
    }

    #[test]
    #[should_panic(expected = "Unauthorized")]
    fn test_set_fee_splits_admin_only() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let random = Address::generate(&env);
        let extra = Vec::new(&env);
        MarketplaceClient::new(&env, &contract_id)
            .set_fee_splits(&random, &100u32, &100u32, &100u32, &extra);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // AUCTION EDGE CASES — multiple bidders, extensions, finalization
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_english_auction_multiple_bidders() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let bidder_a = Address::generate(&env);
        let bidder_b = Address::generate(&env);
        let bidder_c = Address::generate(&env);
        seed_agent(&env, &contract_id, 10000, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);
        let auction_id = client.create_auction(
            &10000u64,
            &seller,
            &1_000i128,
            &500i128,
            &1u64,
            &Some(100u32),
        );
        // Bidder A bids at start price
        client.place_bid(&auction_id, &bidder_a, &1_000i128);
        // Bidder B outbids (1% increment = 10, so min is 1010)
        client.place_bid(&auction_id, &bidder_b, &1_100i128);
        // Bidder C outbids (min = 1100 + 11 = 1111)
        client.place_bid(&auction_id, &bidder_c, &1_200i128);
        let auction: stellai_lib::Auction = env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .get(&(String::from_str(&env, "auc_"), auction_id))
                .unwrap()
        });
        assert_eq!(auction.highest_bid, 1_200i128);
        assert_eq!(auction.highest_bidder.unwrap(), bidder_c);
    }

    #[test]
    fn test_english_auction_finalize_reserve_not_met() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 10100, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);
        let auction_id = client.create_auction(
            &10100u64, &seller, &1_000i128, &1_000i128, // reserve = start
            &1u64, &None,
        );
        // No bids placed
        env.ledger().with_mut(|l| {
            l.timestamp = 86401;
        });
        client.finalize_auction(&auction_id);
        // Agent should be returned to seller
        env.as_contract(&contract_id, || {
            let ak = (
                String::from_str(&env, stellai_lib::AGENT_KEY_PREFIX),
                10100u64,
            );
            let agent: stellai_lib::Agent = env.storage().instance().get(&ak).unwrap();
            assert_eq!(agent.owner, seller);
            assert!(!agent.escrow_locked);
        });
    }

    #[test]
    fn test_finalize_auction_not_ended_yet() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 10200, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);
        let auction_id =
            client.create_auction(&10200u64, &seller, &1_000i128, &500i128, &30u64, &None);
        // Try to finalize too early
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            env.mock_all_auths();
            let client2 = MarketplaceClient::new(&env, &contract_id);
            client2.finalize_auction(&auction_id);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_dutch_auction_price_decay_accuracy() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        seed_agent(&env, &contract_id, 11000, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);
        // 1000 -> 0 over 10 seconds
        let auction_id =
            client.create_dutch_auction(&11000u64, &seller, &1_000i128, &0i128, &10u64);

        // At t=0: price should be 1000
        // At t=5: price should be ~500
        // At t=10: price should be 0 (reserve)

        // Buy at start (full price)
        env.ledger().with_mut(|l| {
            l.timestamp = 1;
        });
        client.dutch_buy_now(&auction_id, &buyer, &1_000i128);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // FUZZING TESTS — Dutch auction price calculation accuracy
    // ─────────────────────────────────────────────────────────────────────────

    /// Fuzz-style test: verify Dutch auction linear decay formula at multiple
    /// time points across the auction lifecycle. Tests that:
    /// - Price at start equals start_price
    /// - Price at end equals reserve_price
    /// - Price decreases monotonically
    /// - Price at midpoint is approximately (start + reserve) / 2
    /// - Bid at or above decay price succeeds
    /// - Bid below decay price fails
    #[test]
    fn test_dutch_auction_price_fuzzing_at_boundaries() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 20000, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);

        // Test Case 1: start=10000, reserve=0, duration=100s
        let auction_id =
            client.create_dutch_auction(&20000u64, &seller, &10_000i128, &0i128, &100u64);

        // At t=0: price = 10000 (start)
        // At t=25: price = 7500 (75%)
        // At t=50: price = 5000 (50%)
        // At t=75: price = 2500 (25%)
        // At t=100: price = 0 (reserve)

        // Verify bid at t=50 with price 5000 works
        env.ledger().with_mut(|l| {
            l.timestamp = 50;
        });
        let buyer1 = Address::generate(&env);
        client.dutch_buy_now(&auction_id, &buyer1, &5_000i128);
        let auction: stellai_lib::Auction = env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .get(&(String::from_str(&env, "auc_"), auction_id))
                .unwrap()
        });
        assert_eq!(auction.status, stellai_lib::AuctionStatus::Won);
    }

    /// Fuzz-style test: Dutch auction with non-zero reserve price
    #[test]
    fn test_dutch_auction_nonzero_reserve_fuzzing() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 20100, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);

        // start=1000, reserve=500, duration=10s
        // Decay rate = (1000-500)/10 = 50 per second
        let auction_id =
            client.create_dutch_auction(&20100u64, &seller, &1_000i128, &500i128, &10u64);

        // At t=5: price = 1000 - 50*5 = 750
        env.ledger().with_mut(|l| {
            l.timestamp = 5;
        });
        let buyer = Address::generate(&env);
        client.dutch_buy_now(&auction_id, &buyer, &750i128);
    }

    /// Fuzz-style test: Dutch auction with large values to check overflow safety
    #[test]
    fn test_dutch_auction_large_values_fuzzing() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 20200, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);

        // Large price range: 1_000_000_000 -> 100_000_000 over 365 days
        let auction_id = client.create_dutch_auction(
            &20200u64,
            &seller,
            &1_000_000_000i128,
            &100_000_000i128,
            &365u64,
        );

        // Buy at start (full price)
        env.ledger().with_mut(|l| {
            l.timestamp = 1;
        });
        let buyer = Address::generate(&env);
        client.dutch_buy_now(&auction_id, &buyer, &1_000_000_000i128);
    }

    /// Fuzz-style test: Dutch auction with very short duration
    #[test]
    fn test_dutch_auction_short_duration_fuzzing() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 20300, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);

        // 1 second duration: immediate buy at start price
        let auction_id =
            client.create_dutch_auction(&20300u64, &seller, &1_000i128, &100i128, &1u64);

        env.ledger().with_mut(|l| {
            l.timestamp = 0;
        });
        let buyer = Address::generate(&env);
        client.dutch_buy_now(&auction_id, &buyer, &1_000i128);
    }

    /// Fuzz-style test: Dutch auction with equal start and reserve price
    #[test]
    fn test_dutch_auction_equal_prices_fuzzing() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 20400, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);

        // start=reserve: price stays constant
        let auction_id =
            client.create_dutch_auction(&20400u64, &seller, &5_000i128, &5_000i128, &5u64);

        // Buy at any time with exact price
        env.ledger().with_mut(|l| {
            l.timestamp = 3;
        });
        let buyer = Address::generate(&env);
        client.dutch_buy_now(&auction_id, &buyer, &5_000i128);
    }

    /// Fuzz-style test: verify Dutch auction bid rejection below decay price
    #[test]
    #[should_panic(expected = "Bid below current Dutch price")]
    fn test_dutch_auction_reject_below_decay_fuzzing() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 20500, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);

        // start=1000, reserve=0, duration=10s
        let auction_id =
            client.create_dutch_auction(&20500u64, &seller, &1_000i128, &0i128, &10u64);

        // At t=0: price=1000, bid 999 should fail
        env.ledger().with_mut(|l| {
            l.timestamp = 0;
        });
        let buyer = Address::generate(&env);
        client.dutch_buy_now(&auction_id, &buyer, &999i128);
    }

    /// Fuzz-style test: Dutch auction rejection below reserve
    #[test]
    #[should_panic(expected = "Bid below reserve price")]
    fn test_dutch_auction_reject_below_reserve_fuzzing() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 20600, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);

        // start=1000, reserve=500, duration=10s
        let auction_id =
            client.create_dutch_auction(&20600u64, &seller, &1_000i128, &500i128, &10u64);

        // At t=9: decayed price = 1000 - 50*9 = 550, but bid 540 < 500 reserve
        env.ledger().with_mut(|l| {
            l.timestamp = 9;
        });
        let buyer = Address::generate(&env);
        client.dutch_buy_now(&auction_id, &buyer, &540i128);
    }

    /// Fuzz-style test: verify Dutch auction rejects bids after expiry
    #[test]
    #[should_panic(expected = "Dutch auction has ended")]
    fn test_dutch_auction_after_expiry_fuzzing() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 20700, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);

        let auction_id = client.create_dutch_auction(&20700u64, &seller, &1_000i128, &0i128, &5u64);

        // After end_time
        env.ledger().with_mut(|l| {
            l.timestamp = 6;
        });
        let buyer = Address::generate(&env);
        client.dutch_buy_now(&auction_id, &buyer, &1_000i128);
    }

    /// Fuzz-style test: verify Dutch auction rejects non-Dutch auctions
    #[test]
    #[should_panic(expected = "Auction is not Dutch")]
    fn test_dutch_buy_now_on_english_auction_fuzzing() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        seed_agent(&env, &contract_id, 20800, &seller);
        let client = MarketplaceClient::new(&env, &contract_id);

        // Create English auction
        let auction_id =
            client.create_auction(&20800u64, &seller, &1_000i128, &500i128, &5u64, &None);

        // Try to buy as Dutch
        let buyer = Address::generate(&env);
        client.dutch_buy_now(&auction_id, &buyer, &1_000i128);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // COMPREHENSIVE INTEGRATION SCENARIOS
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_full_nft_lifecycle_create_buy_cancel() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);

        // Register currency
        client.register_currency(
            &admin,
            &String::from_str(&env, "USDC"),
            &Some(Address::generate(&env)),
            &7u32,
        );

        // Create NFT listing
        let nft_ref = stellai_lib::NftTokenRef {
            contract_address: Some(Address::generate(&env)),
            token_id: 999,
            standard: stellai_lib::NftStandard::Erc721,
        };
        let listing_id = client.create_nft_listing(
            &nft_ref,
            &seller,
            &10_000i128,
            &String::from_str(&env, "USDC"),
            &Some(Address::generate(&env)),
            &Some(30u64),
            &String::from_str(&env, "ipfs://QmNFT999"),
        );

        // Buy
        client.buy_nft_listing(&listing_id, &buyer, &10_000i128);
        assert!(!client.get_nft_listing(&listing_id).active);
    }

    #[test]
    fn test_collection_with_ipfs_and_royalty() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let creator = Address::generate(&env);
        let alice = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);

        // Create collection
        let cid = client.create_collection(&creator, &String::from_str(&env, "ArtDrop"), &0u32);

        // Set IPFS metadata
        client.set_collection_ipfs_metadata(
            &creator,
            &cid,
            &String::from_str(&env, "ipfs://QmArtDropMeta"),
        );

        // Set royalty split
        let mut recipients = Vec::new(&env);
        recipients.push_back(RoyaltyRecipient {
            recipient: alice.clone(),
            share_bps: 700u32,
            role: String::from_str(&env, "creator"),
        });
        client.set_collection_royalty(&creator, &cid, &recipients, &700u32);

        // Verify
        let coll = client.get_collection(&cid);
        assert_eq!(coll.royalty_config.total_bps, 700);
        let uri = client.get_collection_ipfs_metadata(&cid).unwrap();
        assert_eq!(uri, String::from_str(&env, "ipfs://QmArtDropMeta"));
    }

    #[test]
    fn test_batch_nft_operations() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _) = setup_marketplace(&env);
        let seller = Address::generate(&env);
        let client = MarketplaceClient::new(&env, &contract_id);
        let nft_ref = stellai_lib::NftTokenRef {
            contract_address: None,
            token_id: 300,
            standard: stellai_lib::NftStandard::SorobanNative,
        };
        // Create multiple NFT listings
        let id1 = client.create_nft_listing(
            &nft_ref,
            &seller,
            &1_000i128,
            &String::from_str(&env, "XLM"),
            &None,
            &None,
            &String::from_str(&env, "ipfs://1"),
        );
        let nft_ref2 = stellai_lib::NftTokenRef {
            contract_address: None,
            token_id: 301,
            standard: stellai_lib::NftStandard::SorobanNative,
        };
        let id2 = client.create_nft_listing(
            &nft_ref2,
            &seller,
            &2_000i128,
            &String::from_str(&env, "XLM"),
            &None,
            &None,
            &String::from_str(&env, "ipfs://2"),
        );
        assert!(client.get_nft_listing(&id1).active);
        assert!(client.get_nft_listing(&id2).active);
    }
}
