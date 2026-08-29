#![no_std]

use soroban_sdk::{contract, contractimpl, vec, Address, Env, String, Symbol, Vec};

const ADMIN_KEY: &str = "admin";
const LISTING_COUNTER_KEY: &str = "listing_counter";
const LISTING_KEY_PREFIX: &str = "listing_";
const ROYALTY_KEY_PREFIX: &str = "royalty_";
const AGENT_NFT_CONTRACT_KEY: &str = "agent_nft_contract";
const PREDICTION_MARKET_CONTRACT_KEY: &str = "prediction_market_contract";
const PREDICTION_SHARES_KEY_PREFIX: &str = "prediction_shares_";
const MARKET_SHARES_INDEX_PREFIX: &str = "market_shares_"; // Index of all shares IDs per market
const MARKET_COUNTER_KEY: &str = "market_counter";
const MARKET_KEY_PREFIX: &str = "market_";

#[contract]
pub struct Marketplace;

#[contractimpl]
impl Marketplace {
    /// Initialize contract with admin
    pub fn init_contract(env: Env, admin: Address) {
        let admin_data = env
            .storage()
            .instance()
            .get::<_, Address>(&Symbol::new(&env, ADMIN_KEY));
        if admin_data.is_some() {
            panic!("Contract already initialized");
        }

        admin.require_auth();
        env.storage()
            .instance()
            .set(&Symbol::new(&env, ADMIN_KEY), &admin);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, LISTING_COUNTER_KEY), &0u64);
    }

    /// Set the AgentNFT contract address (called once by admin)
    pub fn set_agent_nft_contract(env: Env, admin: Address, agent_nft_contract: Address) {
        Self::verify_admin(&env, &admin);
        env.storage().instance().set(
            &Symbol::new(&env, AGENT_NFT_CONTRACT_KEY),
            &agent_nft_contract,
        );

        env.events().publish(
            (Symbol::new(&env, "agent_nft_contract_set"),),
            agent_nft_contract,
        );
    }

    /// Get the AgentNFT contract address
    fn get_agent_nft_contract(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(env, AGENT_NFT_CONTRACT_KEY))
            .expect("AgentNFT contract not set")
    }

    /// Set the PredictionMarket contract address (called once by admin)
    pub fn set_prediction_market_contract(env: Env, admin: Address, prediction_market_contract: Address) {
        Self::verify_admin(&env, &admin);
        env.storage().instance().set(
            &Symbol::new(&env, PREDICTION_MARKET_CONTRACT_KEY),
            &prediction_market_contract,
        );

        env.events().publish(
            (Symbol::new(&env, "prediction_market_contract_set"),),
            prediction_market_contract,
        );
    }

    /// Get the PredictionMarket contract address
    fn get_prediction_market_contract(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(env, PREDICTION_MARKET_CONTRACT_KEY))
            .expect("PredictionMarket contract not set")
    }

    /// Verify caller is admin
    fn verify_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(env, ADMIN_KEY))
            .expect("Admin not set");

        if caller != &admin {
            panic!("Unauthorized: caller is not admin");
        }
    }

    /// Safe addition with overflow checks
    fn safe_add(a: u64, b: u64) -> u64 {
        a.checked_add(b).expect("Arithmetic overflow in safe_add")
    }

    /// Safe multiplication with overflow checks for price calculations
    fn safe_mul_i128(a: i128, b: u32) -> i128 {
        a.checked_mul(b as i128)
            .expect("Arithmetic overflow in multiplication")
    }

    /// Create a new listing with comprehensive validation and escrow locking
    pub fn create_listing(
        env: Env,
        asset_id: u64,
        asset_type: u32, // 0=Agent, 1=PredictionShares
        seller: Address,
        listing_type: u32, // 0=Sale, 1=Lease, 2=Auction
        price: i128,
        duration_days: Option<u64>, // For leases
    ) -> u64 {
        seller.require_auth();

        // Input validation
        if asset_id == 0 {
            panic!("Invalid asset ID");
        }
        if asset_type > 1 {
            panic!("Invalid asset type");
        }
        if listing_type > 2 {
            panic!("Invalid listing type");
        }

        // Price bounds checking to prevent overflow/underflow
        if price < stellai_lib::PRICE_LOWER_BOUND || price > stellai_lib::PRICE_UPPER_BOUND {
            panic!("Price out of valid range");
        }

        // Validate lease duration if applicable
        if listing_type == 1 {
            let duration = duration_days.expect("Duration required for lease listings");
            if duration == 0 || duration > stellai_lib::MAX_DURATION_DAYS {
                panic!("Lease duration out of valid range");
            }
        }

        // Verify asset exists and seller is owner
        if asset_type == 0 {
            // Agent asset
            let agent_nft_contract = Self::get_agent_nft_contract(&env);
            let agent_key_str = String::from_str(&env, "agent_");
            let agent: stellai_lib::Agent = env
                .storage()
                .instance()
                .get(&agent_key_str)
                .expect("Agent not found");

            if agent.owner != seller {
                panic!("Unauthorized: only agent owner can create listings");
            }

            // Check if agent is already locked in escrow
            if agent.escrow_locked {
                panic!("Agent is already locked in escrow");
            }
        } else {
            // Prediction shares asset
            let prediction_shares_key_str = String::from_str(&env, &format!("{}{}", PREDICTION_SHARES_KEY_PREFIX, asset_id));
            let shares: stellai_lib::PredictionShares = env
                .storage()
                .instance()
                .get(&prediction_shares_key_str)
                .expect("Prediction shares not found");

            if shares.owner != seller {
                panic!("Unauthorized: only shares owner can create listings");
            }

            // Check if shares are already locked in escrow
            if shares.escrow_locked {
                panic!("Prediction shares are already locked in escrow");
            }
        }

        // Generate listing ID safely
        let counter: u64 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, LISTING_COUNTER_KEY))
            .unwrap_or(0);
        let listing_id = Self::safe_add(counter, 1);

        // Create listing
        let listing = stellai_lib::Listing {
            listing_id,
            asset_id,
            asset_type: match asset_type {
                0 => stellai_lib::AssetType::Agent,
                1 => stellai_lib::AssetType::PredictionShares,
                _ => panic!("Invalid asset type"),
            },
            seller: seller.clone(),
            price,
            listing_type: match listing_type {
                0 => stellai_lib::ListingType::Sale,
                1 => stellai_lib::ListingType::Lease,
                2 => stellai_lib::ListingType::Auction,
                _ => panic!("Invalid listing type"),
            },
            active: true,
            created_at: env.ledger().timestamp(),
        };

        // Store listing
        let key_str = String::from_str(&env, LISTING_KEY_PREFIX);
        env.storage().instance().set(&key_str, &listing);

        // Update counter
        env.storage()
            .instance()
            .set(&Symbol::new(&env, LISTING_COUNTER_KEY), &listing_id);

        // Lock asset in escrow
        let marketplace_address = env.current_contract_address();
        if asset_type == 0 {
            // Lock agent in escrow
            let agent_nft_contract = Self::get_agent_nft_contract(&env);
            agent_nft_contract.require_auth();
            // Note: In production, this would be a cross-contract call
            // For now, we'll simulate the lock by updating the agent directly
            let agent_key_str = String::from_str(&env, "agent_");
            let mut agent: stellai_lib::Agent = env
                .storage()
                .instance()
                .get(&agent_key_str)
                .expect("Agent not found");
            let mut updated_agent = agent.clone();
            updated_agent.escrow_locked = true;
            updated_agent.escrow_holder = Some(marketplace_address.clone());
            updated_agent.updated_at = env.ledger().timestamp();
            env.storage().instance().set(&agent_key_str, &updated_agent);

            env.events().publish(
                (Symbol::new(&env, "agent_escrow_locked"),),
                (asset_id, seller.clone(), marketplace_address.clone()),
            );
        } else {
            // Lock prediction shares in escrow
            let prediction_market_contract = Self::get_prediction_market_contract(&env);
            prediction_market_contract.require_auth();
            // Note: In production, this would be a cross-contract call
            let prediction_shares_key_str = String::from_str(&env, &format!("{}{}", PREDICTION_SHARES_KEY_PREFIX, asset_id));
            let mut shares: stellai_lib::PredictionShares = env
                .storage()
                .instance()
                .get(&prediction_shares_key_str)
                .expect("Prediction shares not found");
            let mut updated_shares = shares.clone();
            updated_shares.escrow_locked = true;
            updated_shares.escrow_holder = Some(marketplace_address.clone());
            updated_shares.updated_at = env.ledger().timestamp();
            env.storage().instance().set(&prediction_shares_key_str, &updated_shares);

            env.events().publish(
                (Symbol::new(&env, "prediction_shares_escrow_locked"),),
                (asset_id, seller.clone(), marketplace_address.clone()),
            );
        }

        env.events().publish(
            (Symbol::new(&env, "listing_created"),),
            (listing_id, asset_id, asset_type, seller.clone(), price),
        );

        listing_id
    }

    /// Purchase or lease an asset (agent or prediction shares) with comprehensive security checks and escrow release
    pub fn buy_asset(
        env: Env,
        listing_id: u64,
        buyer: Address,
        _payment_token: Address, // In production, would transfer from this token contract
        amount: i128,
    ) {
        buyer.require_auth();

        if listing_id == 0 {
            panic!("Invalid listing ID");
        }

        // Get listing
        let listing_key_str = String::from_str(&env, LISTING_KEY_PREFIX);
        let mut listing: stellai_lib::Listing = env
            .storage()
            .instance()
            .get(&listing_key_str)
            .expect("Listing not found");

        // Validation checks
        if !listing.active {
            panic!("Listing is not active");
        }
        if amount < listing.price {
            panic!("Insufficient payment amount");
        }

        // Prevent payment overflow issues
        if amount > stellai_lib::PRICE_UPPER_BOUND {
            panic!("Payment amount exceeds safe maximum");
        }

        // Verify asset is locked in escrow by this marketplace contract
        let marketplace_address = env.current_contract_address();
        if listing.asset_type == stellai_lib::AssetType::Agent {
            // Get agent to verify it's locked in escrow
            let agent_key_str = String::from_str(&env, "agent_");
            let mut agent: stellai_lib::Agent = env
                .storage()
                .instance()
                .get(&agent_key_str)
                .expect("Agent not found");

            // Verify agent is locked by this marketplace contract
            if !agent.escrow_locked {
                panic!("Agent is not locked in escrow");
            }

            match &agent.escrow_holder {
                Some(holder) => {
                    if holder != &marketplace_address {
                        panic!("Agent is locked by a different contract");
                    }
                }
                None => panic!("Agent escrow holder not set"),
            }
        } else {
            // Get prediction shares to verify they're locked in escrow
            let prediction_shares_key_str = String::from_str(&env, &format!("{}{}", PREDICTION_SHARES_KEY_PREFIX, listing.asset_id));
            let mut shares: stellai_lib::PredictionShares = env
                .storage()
                .instance()
                .get(&prediction_shares_key_str)
                .expect("Prediction shares not found");

            // Verify shares are locked by this marketplace contract
            if !shares.escrow_locked {
                panic!("Prediction shares are not locked in escrow");
            }

            match &shares.escrow_holder {
                Some(holder) => {
                    if holder != &marketplace_address {
                        panic!("Prediction shares are locked by a different contract");
                    }
                }
                None => panic!("Prediction shares escrow holder not set"),
            }
        }

        // Get royalty info if exists
        let royalty_key_str = String::from_str(&env, ROYALTY_KEY_PREFIX);
        let royalty_info: Option<stellai_lib::RoyaltyInfo> =
            env.storage().instance().get(&royalty_key_str);

        // Calculate and validate royalty (if exists)
        let mut royalty_amount: i128 = 0;
        if let Some(royalty) = &royalty_info {
            if royalty.percentage > stellai_lib::MAX_ROYALTY_PERCENTAGE {
                panic!("Invalid royalty percentage");
            }
            // Safe calculation: (amount * percentage) / 10000
            royalty_amount = Self::safe_mul_i128(amount, royalty.percentage)
                .checked_div(10000)
                .expect("Division by zero");
        }

        // Calculate seller amount (with safe arithmetic)
        let seller_amount = amount
            .checked_sub(royalty_amount)
            .expect("Arithmetic underflow in seller amount calculation");

        // In production:
        // - Transfer payment_token from buyer to seller
        // - Transfer royalty to royalty recipient
        // - Transfer agent NFT from seller to buyer
        // - Update agent ownership

        // Release asset from escrow and transfer ownership
        if listing.asset_type == stellai_lib::AssetType::Agent {
            // Transfer agent ownership
            let agent_key_str = String::from_str(&env, "agent_");
            let mut agent: stellai_lib::Agent = env
                .storage()
                .instance()
                .get(&agent_key_str)
                .expect("Agent not found");
            
            agent.escrow_locked = false;
            agent.escrow_holder = None;
            agent.owner = buyer.clone();
            agent.updated_at = env.ledger().timestamp();
            agent.nonce = agent.nonce.checked_add(1).expect("Nonce overflow");

            env.storage().instance().set(&agent_key_str, &agent);

            env.events().publish(
                (Symbol::new(&env, "agent_sold"),),
                (
                    listing_id,
                    listing.asset_id,
                    buyer.clone(),
                    seller_amount,
                    royalty_amount,
                ),
            );

            env.events().publish(
                (Symbol::new(&env, "agent_escrow_released"),),
                (listing.asset_id, buyer.clone(), marketplace_address),
            );
        } else {
            // Transfer prediction shares ownership
            let prediction_shares_key_str = String::from_str(&env, &format!("{}{}", PREDICTION_SHARES_KEY_PREFIX, listing.asset_id));
            let mut shares: stellai_lib::PredictionShares = env
                .storage()
                .instance()
                .get(&prediction_shares_key_str)
                .expect("Prediction shares not found");
            
            shares.escrow_locked = false;
            shares.escrow_holder = None;
            shares.owner = buyer.clone();
            shares.updated_at = env.ledger().timestamp();

            env.storage().instance().set(&prediction_shares_key_str, &shares);

            env.events().publish(
                (Symbol::new(&env, "prediction_shares_sold"),),
                (
                    listing_id,
                    listing.asset_id,
                    buyer.clone(),
                    seller_amount,
                    royalty_amount,
                ),
            );

            env.events().publish(
                (Symbol::new(&env, "prediction_shares_escrow_released"),),
                (listing.asset_id, buyer.clone(), marketplace_address),
            );
        }

        // Deactivate listing
        listing.active = false;
        env.storage().instance().set(&listing_key_str, &listing);

        env.events().publish(
            (Symbol::new(&env, "listing_cancelled"),),
            (listing_id, listing.asset_type as u32, seller),
        );
    }

    /// Create a dispute for a marketplace transaction (supports both agent and prediction shares transactions)
    pub fn create_dispute(
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

        // Get the listing to verify it exists and was completed
        let listing_key_str = String::from_str(&env, LISTING_KEY_PREFIX);
        let listing: stellai_lib::Listing = env
            .storage()
            .instance()
            .get(&listing_key_str)
            .expect("Listing not found");

        // Only allow disputes on completed transactions (inactive listings)
        if listing.active {
            panic!("Cannot dispute an active listing - transaction must be completed first");
        }

        // Verify the initiator is either the buyer or seller of the transaction
        // In production, we would store the buyer as part of the transaction record
        // For this integration, we'll verify the initiator is either the seller or admin
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .expect("Admin not set");

        if initiator != listing.seller && initiator != admin {
            panic!("Unauthorized: only transaction participants or admin can create disputes");
        }

        // Generate dispute ID
        let counter: u64 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "dispute_counter"))
            .unwrap_or(0);
        let dispute_id = Self::safe_add(counter, 1);

        // Create dispute object
        let dispute = stellai_lib::Dispute {
            dispute_id,
            listing_id,
            asset_type: listing.asset_type,
            initiator: initiator.clone(),
            reason,
            evidence_cid,
            status: stellai_lib::DisputeStatus::Open,
            created_at: env.ledger().timestamp(),
            resolved_at: None,
        };

        // Store the dispute
        let dispute_key_str = String::from_str(&env, &format!("dispute_{}", dispute_id));
        env.storage().instance().set(&dispute_key_str, &dispute);

        // Update dispute counter
        env.storage().instance().set(&Symbol::new(&env, "dispute_counter"), &dispute_id);

        env.events().publish(
            (Symbol::new(&env, "dispute_created"),),
            (dispute_id, listing_id, listing.asset_type as u32, initiator),
        );

        dispute_id
    }

    /// Resolve a dispute (only callable by admin)
    pub fn resolve_dispute(
        env: Env,
        dispute_id: u64,
        caller: Address,
        resolution: String,
        ruling: u32, // 0=refund_buyer, 1=seller_keeps_funds, 2=split_funds
    ) {
        // Verify caller is admin
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .expect("Admin not set");
        if caller != admin {
            panic!("Unauthorized: only admin can resolve disputes");
        }

        // Get the dispute
        let dispute_key_str = String::from_str(&env, &format!("dispute_{}", dispute_id));
        let mut dispute: stellai_lib::Dispute = env
            .storage()
            .instance()
            .get(&dispute_key_str)
            .expect("Dispute not found");

        // Verify dispute is open
        if dispute.status != stellai_lib::DisputeStatus::Open {
            panic!("Cannot resolve a dispute that is not open");
        }

        // Update dispute status
        dispute.status = stellai_lib::DisputeStatus::Resolved;
        dispute.resolved_at = Some(env.ledger().timestamp());
        env.storage().instance().set(&dispute_key_str, &dispute);

        env.events().publish(
            (Symbol::new(&env, "dispute_resolved"),),
            (dispute_id, resolution, ruling),
        );
    }

    /// Resolve a prediction market (only callable by the oracle assigned to that market)
    pub fn resolve_prediction_market(
        env: Env,
        market_id: u64,
        caller: Address,
        outcome: u32, // 0=Yes, 1=No, 2=Invalid
    ) {
        // Get the prediction market
        let market_key_str = String::from_str(&env, &format!("{}{}", MARKET_KEY_PREFIX, market_id));
        let mut market: stellai_lib::PredictionMarket = env
            .storage()
            .instance()
            .get(&market_key_str)
            .expect("Prediction market not found");

        // Verify the caller is the assigned oracle for this market
        if market.oracle_address != caller {
            panic!("Unauthorized: only the market's oracle can resolve it");
        }

        // Verify market hasn't been resolved yet
        if market.resolved {
            panic!("Market has already been resolved");
        }

        // Verify the market has ended
        if env.ledger().timestamp() < market.end_timestamp {
            panic!("Cannot resolve market before end timestamp");
        }

        // Set the outcome
        market.resolved = true;
        market.outcome = match outcome {
            0 => Some(stellai_lib::PredictionOutcome::Yes),
            1 => Some(stellai_lib::PredictionOutcome::No),
            2 => Some(stellai_lib::PredictionOutcome::Invalid),
            _ => panic!("Invalid outcome"),
        };

        // Update the market in storage
        env.storage().instance().set(&market_key_str, &market);

        // Trigger automated payout processing
        Self::process_market_payouts(&env, market_id, market.outcome.unwrap());

        env.events().publish(
            (Symbol::new(&env, "market_resolved"),),
            (market_id, outcome, caller),
        );
    }

    /// Process payouts for all shareholders in a resolved prediction market
    fn process_market_payouts(env: &Env, market_id: u64, outcome: stellai_lib::PredictionOutcome) {
        // Get all shares for this market from the index
        let market_shares_index_key = String::from_str(env, &format!("{}{}", MARKET_SHARES_INDEX_PREFIX, market_id));
        let shares_index: Vec<u64> = env.storage().instance()
            .get(&market_shares_index_key)
            .unwrap_or_else(|| soroban_sdk::vec![env]);

        // Get the market to access total shares
        let market_key_str = String::from_str(env, &format!("{}{}", MARKET_KEY_PREFIX, market_id));
        let market: stellai_lib::PredictionMarket = env
            .storage()
            .instance()
            .get(&market_key_str)
            .expect("Prediction market not found");

        let mut total_payout_processed: u128 = 0;
        let mut payout_recipients: Vec<(Address, u128)> = soroban_sdk::vec![env];

        // If the market was invalid, return all funds proportionally (full refunds)
        if outcome == stellai_lib::PredictionOutcome::Invalid {
            // Process refunds for all shareholders - return 100% of their original funds
            for shares_id in shares_index.iter() {
                let shares_key_str = String::from_str(env, &format!("{}{}", PREDICTION_SHARES_KEY_PREFIX, shares_id));
                let mut shares: stellai_lib::PredictionShares = env
                    .storage()
                    .instance()
                    .get(&shares_key_str)
                    .expect("Prediction shares not found");

                // In production, this would transfer the refund amount to the shareholder
                // For now, we'll record the payout and update shares state
                let total_shares = shares.shares_yes + shares.shares_no;
                total_payout_processed = total_payout_processed.checked_add(total_shares).expect("Payout overflow");
                
                payout_recipients.push_back((shares.owner.clone(), total_shares));
                
                // Mark shares as processed (paid out)
                shares.updated_at = env.ledger().timestamp();
                env.storage().instance().set(&shares_key_str, &shares);
            }

            env.events().publish(
                (Symbol::new(env, "market_payouts_processed"),),
                (market_id, "refund", total_payout_processed, payout_recipients.len()),
            );
            return;
        }

        // For valid outcomes (Yes/No), process payouts to winning shareholders
        let winning_shares_total = if outcome == stellai_lib::PredictionOutcome::Yes {
            market.total_shares_yes
        } else {
            market.total_shares_no
        };

        // If no one holds winning shares, nothing to process
        if winning_shares_total == 0 {
            env.events().publish(
                (Symbol::new(env, "market_payouts_processed"),),
                (market_id, format!("{:?}", outcome), 0u128, 0u32),
            );
            return;
        }

        // Total pool of funds to distribute to winners (sum of all losing shares)
        let total_pool = if outcome == stellai_lib::PredictionOutcome::Yes {
            market.total_shares_no
        } else {
            market.total_shares_yes
        };

        // Process payouts to each winning shareholder
        for shares_id in shares_index.iter() {
            let shares_key_str = String::from_str(env, &format!("{}{}", PREDICTION_SHARES_KEY_PREFIX, shares_id));
            let mut shares: stellai_lib::PredictionShares = env
                .storage()
                .instance()
                .get(&shares_key_str)
                .expect("Prediction shares not found");

            // Check if this shareholder holds winning shares
            let user_winning_shares = if outcome == stellai_lib::PredictionOutcome::Yes {
                shares.shares_yes
            } else {
                shares.shares_no
            };

            if user_winning_shares > 0 {
                // Calculate proportional payout: (user_winning_shares / winning_shares_total) * total_pool
                // Using integer arithmetic to avoid floating points: (user_winning_shares * total_pool) / winning_shares_total
                let user_payout = (user_winning_shares as u128)
                    .checked_mul(total_pool as u128)
                    .and_then(|product| product.checked_div(winning_shares_total as u128))
                    .expect("Payout calculation overflow/division by zero");

                total_payout_processed = total_payout_processed
                    .checked_add(user_payout)
                    .expect("Total payout overflow");

                payout_recipients.push_back((shares.owner.clone(), user_payout));

                // In production, this would transfer the payout amount to the shareholder's address
                // Here we would call the token contract's transfer function:
                // token_contract.transfer(env, &env.current_contract_address(), &shares.owner, user_payout as i128);
            }

            // Update shares state
            shares.updated_at = env.ledger().timestamp();
            env.storage().instance().set(&shares_key_str, &shares);
        }

        env.events().publish(
            (Symbol::new(env, "market_payouts_processed"),),
            (market_id, format!("{:?}", outcome), total_payout_processed, payout_recipients.len()),
        );
    }

    /// Create prediction shares for a user (called when user buys into a prediction market)
    pub fn create_prediction_shares(
        env: Env,
        market_id: u64,
        owner: Address,
        shares_yes: u128,
        shares_no: u128,
    ) -> u64 {
        owner.require_auth();

        // Get the market to verify it exists and is still active
        let market_key_str = String::from_str(&env, &format!("{}{}", MARKET_KEY_PREFIX, market_id));
        let market: stellai_lib::PredictionMarket = env
            .storage()
            .instance()
            .get(&market_key_str)
            .expect("Prediction market not found");

        if market.resolved {
            panic!("Cannot create shares for a resolved market");
        }

        // Generate shares ID
        let counter: u64 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, "shares_counter"))
            .unwrap_or(0);
        let shares_id = Self::safe_add(counter, 1);

        // Create shares object
        let shares = stellai_lib::PredictionShares {
            shares_id,
            market_id,
            owner: owner.clone(),
            shares_yes,
            shares_no,
            created_at: env.ledger().timestamp(),
            updated_at: env.ledger().timestamp(),
            escrow_locked: false,
            escrow_holder: None,
        };

        // Store the shares
        let shares_key_str = String::from_str(&env, &format!("{}{}", PREDICTION_SHARES_KEY_PREFIX, shares_id));
        env.storage().instance().set(&shares_key_str, &shares);

        // Add shares ID to the market's shares index
        let market_shares_index_key = String::from_str(&env, &format!("{}{}", MARKET_SHARES_INDEX_PREFIX, market_id));
        let mut shares_index: Vec<u64> = env.storage().instance()
            .get(&market_shares_index_key)
            .unwrap_or_else(|| soroban_sdk::vec![&env]);
        shares_index.push_back(shares_id);
        env.storage().instance().set(&market_shares_index_key, &shares_index);

        // Update the shares counter
        env.storage().instance().set(&Symbol::new(env, "shares_counter"), &shares_id);

        // Update the market's total shares
        let mut updated_market = market.clone();
        updated_market.total_shares_yes = updated_market.total_shares_yes.checked_add(shares_yes).expect("Shares overflow");
        updated_market.total_shares_no = updated_market.total_shares_no.checked_add(shares_no).expect("Shares overflow");
        env.storage().instance().set(&market_key_str, &updated_market);

        env.events().publish(
            (Symbol::new(&env, "prediction_shares_created"),),
            (shares_id, market_id, owner, shares_yes, shares_no),
        );

        shares_id
    }

    /// Create a new prediction market (admin or authorized creator)
    pub fn create_prediction_market(
        env: Env,
        question: String,
        category: String,
        end_timestamp: u64,
        oracle_address: Address,
        creator: Address,
    ) -> u64 {
        creator.require_auth();

        // Input validation
        if end_timestamp <= env.ledger().timestamp() {
            panic!("End timestamp must be in the future");
        }

        // Generate market ID
        let counter: u64 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, MARKET_COUNTER_KEY))
            .unwrap_or(0);
        let market_id = Self::safe_add(counter, 1);

        // Create market object
        let market = stellai_lib::PredictionMarket {
            market_id,
            question,
            category,
            end_timestamp,
            resolved: false,
            outcome: None,
            total_shares_yes: 0,
            total_shares_no: 0,
            oracle_address,
            created_at: env.ledger().timestamp(),
            creator: creator.clone(),
        };

        // Store the market
        let market_key_str = String::from_str(&env, &format!("{}{}", MARKET_KEY_PREFIX, market_id));
        env.storage().instance().set(&market_key_str, &market);

        // Update the market counter
        env.storage().instance().set(&Symbol::new(env, MARKET_COUNTER_KEY), &market_id);

        env.events().publish(
            (Symbol::new(&env, "prediction_market_created"),),
            (market_id, creator, oracle_address, end_timestamp),
        );

        market_id
    }

    /// Cancel a listing with proper authorization and escrow release
    pub fn cancel_listing(env: Env, listing_id: u64, seller: Address) {
        seller.require_auth();

        if listing_id == 0 {
            panic!("Invalid listing ID");
        }

        let listing_key_str = String::from_str(&env, LISTING_KEY_PREFIX);
        let mut listing: stellai_lib::Listing = env
            .storage()
            .instance()
            .get(&listing_key_str)
            .expect("Listing not found");

        if listing.seller != seller {
            panic!("Unauthorized: only seller can cancel listing");
        }

        let marketplace_address = env.current_contract_address();
        if listing.asset_type == stellai_lib::AssetType::Agent {
            // Get agent to release from escrow
            let agent_key_str = String::from_str(&env, "agent_");
            let mut agent: stellai_lib::Agent = env
                .storage()
                .instance()
                .get(&agent_key_str)
                .expect("Agent not found");

            // Verify agent is locked by this marketplace contract
            if !agent.escrow_locked {
                panic!("Agent is not locked in escrow");
            }

            match &agent.escrow_holder {
                Some(holder) => {
                    if holder != &marketplace_address {
                        panic!("Agent is locked by a different contract");
                    }
                }
                None => panic!("Agent escrow holder not set"),
            }

            // Release agent from escrow back to original owner
            agent.escrow_locked = false;
            agent.escrow_holder = None;
            agent.updated_at = env.ledger().timestamp();
            agent.nonce = agent.nonce.checked_add(1).expect("Nonce overflow");

            env.storage().instance().set(&agent_key_str, &agent);

            env.events().publish(
                (Symbol::new(&env, "agent_escrow_released"),),
                (listing.asset_id, seller.clone(), marketplace_address),
            );
        } else {
            // Get prediction shares to release from escrow
            let prediction_shares_key_str = String::from_str(&env, &format!("{}{}", PREDICTION_SHARES_KEY_PREFIX, listing.asset_id));
            let mut shares: stellai_lib::PredictionShares = env
                .storage()
                .instance()
                .get(&prediction_shares_key_str)
                .expect("Prediction shares not found");

            // Verify shares are locked by this marketplace contract
            if !shares.escrow_locked {
                panic!("Prediction shares are not locked in escrow");
            }

            match &shares.escrow_holder {
                Some(holder) => {
                    if holder != &marketplace_address {
                        panic!("Prediction shares are locked by a different contract");
                    }
                }
                None => panic!("Prediction shares escrow holder not set"),
            }

            // Release shares from escrow back to original owner
            shares.escrow_locked = false;
            shares.escrow_holder = None;
            shares.updated_at = env.ledger().timestamp();

            env.storage().instance().set(&prediction_shares_key_str, &shares);

            env.events().publish(
                (Symbol::new(&env, "prediction_shares_escrow_released"),),
                (listing.asset_id, seller.clone(), marketplace_address),
            );
        }

        // Deactivate listing
        listing.active = false;
        env.storage().instance().set(&listing_key_str, &listing);

        env.events().publish(
            (Symbol::new(&env, "listing_cancelled"),),
            (listing_id, listing.asset_id, listing.asset_type as u32, seller.clone()),
        );
    }

    /// Get active listings (with pagination to prevent DoS)
    pub fn get_listings(env: Env, offset: u32, limit: u32) -> soroban_sdk::Vec<stellai_lib::Listing> {
        // Limit query size to prevent DoS
        if limit > 100 || limit == 0 {
            panic!("Limit must be between 1 and 100");
        }
        if offset > 1_000_000 {
            panic!("Offset exceeds maximum allowed");
        }

        // In production, this would query from a more efficient data structure
        // For now, returning empty vec - would iterate stored listings
        soroban_sdk::Vec::new(&env)
    }

    /// Set royalty info for an agent with validation
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
            panic!("Royalty percentage exceeds maximum (100%)");
        }

        // Get agent to verify caller is creator
        let agent_key_str = String::from_str(&env, "agent_");
        let agent: stellai_lib::Agent = env
            .storage()
            .instance()
            .get(&agent_key_str)
            .expect("Agent not found");

        if agent.owner != creator {
            panic!("Unauthorized: only agent creator can set royalty");
        }

        let royalty_info = stellai_lib::RoyaltyInfo {
            recipient,
            percentage,
        };

        let royalty_key_str = String::from_str(&env, ROYALTY_KEY_PREFIX);
        env.storage()
            .instance()
            .set(&royalty_key_str, &royalty_info);

        env.events()
            .publish((Symbol::new(&env, "royalty_set"),), (agent_id, percentage));
    }

    /// Get royalty info for an agent
    pub fn get_royalty(env: Env, agent_id: u64) -> Option<stellai_lib::RoyaltyInfo> {
        if agent_id == 0 {
            panic!("Invalid agent ID");
        }

        let royalty_key_str = String::from_str(&env, ROYALTY_KEY_PREFIX);
        env.storage().instance().get(&royalty_key_str)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_tests;