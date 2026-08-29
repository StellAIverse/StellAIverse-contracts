#![no_std]

mod errors;
mod math;
mod storage;
mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contractimpl, symbol_short, token::TokenClient, Address, Env, String, Symbol,
};

use math::*;
use storage::*;
use types::*;

const MAX_TRADING_FEE_BPS: u32 = 500; // 5% max fee
const MIN_RESOLUTION_WINDOW: u64 = 60; // 1 minute minimum
const MAX_RESOLUTION_WINDOW: u64 = 365 * 24 * 3600; // 1 year max

// ── Contract ────────────────────────────────────────────────────────────────

#[contract]
pub struct PredictionMarket;

#[contractimpl]
impl PredictionMarket {
    // ── Initialization ──────────────────────────────────────────────────

    /// Initialize the prediction market contract.
    pub fn initialize(env: Env, admin: Address, oracle: Address) {
        if storage::get_admin(&env).is_some() {
            panic!("Already initialized");
        }
        admin.require_auth();

        storage::set_admin(&env, &admin);
        storage::set_default_oracle(&env, &oracle);
        storage::set_market_counter(&env, 0);
        storage::set_trading_paused(&env, false);

        env.events().publish(
            (symbol_short!("pm_init"),),
            (admin, oracle, env.ledger().timestamp()),
        );
    }

    // ── Market Lifecycle ────────────────────────────────────────────────

    /// Create a new prediction market with CPMM pools for each outcome.
    /// Uses CreateMarketParams to stay within Soroban's 10-parameter limit.
    pub fn create_market(env: Env, creator: Address, params: CreateMarketParams) -> u64 {
        creator.require_auth();

        if params.num_outcomes < 2 || params.num_outcomes > MAX_OUTCOMES as u32 {
            panic!("Invalid number of outcomes (2-10)");
        }
        if params.outcome_names.len() != params.num_outcomes {
            panic!("Outcome names count must match num_outcomes");
        }
        if params.trading_fee_bps > MAX_TRADING_FEE_BPS {
            panic!("Fee too high");
        }
        if params.resolution_window_duration < MIN_RESOLUTION_WINDOW
            || params.resolution_window_duration > MAX_RESOLUTION_WINDOW
        {
            panic!("Invalid resolution window");
        }
        if params.initial_liquidity <= 0 {
            panic!("Initial liquidity must be positive");
        }
        if params.max_outcome_supply <= 0 {
            panic!("Invalid market cap");
        }

        let market_id = storage::get_market_counter(&env);
        let now = env.ledger().timestamp();

        let market = PredictionMarketV2 {
            market_id,
            question: params.question,
            category: params.category,
            creator: creator.clone(),
            collateral_token: params.collateral_token.clone(),
            oracle_source: params.oracle_source,
            num_outcomes: params.num_outcomes,
            outcome_names: params.outcome_names,
            status: MarketStatus::Active,
            created_at: now,
            resolution_window_start: now,
            resolution_window_end: now + params.resolution_window_duration,
            resolved_outcome: None,
            total_collateral: 0,
            max_outcome_supply: params.max_outcome_supply,
            trading_fee_bps: params.trading_fee_bps,
        };

        storage::set_market(&env, &market);
        storage::set_market_counter(&env, market_id + 1);

        // Seed each outcome pool with initial liquidity.
        let contract_addr = env.current_contract_address();
        let collateral_client = TokenClient::new(&env, &params.collateral_token);
        let total_initial = params.initial_liquidity * params.num_outcomes as i128;
        collateral_client.transfer(&creator, &contract_addr, &total_initial);

        // For each outcome pool, seed with equal collateral and virtual outcome tokens.
        let outcome_per_pool = params.initial_liquidity; // virtual outcome tokens

        for i in 0..params.num_outcomes {
            let pool = OutcomePool {
                collateral_reserve: params.initial_liquidity,
                outcome_reserve: outcome_per_pool,
                lp_total_supply: isqrt(params.initial_liquidity * outcome_per_pool),
            };
            storage::set_outcome_pool(&env, market_id, i, &pool);
            storage::set_total_outcome_supply(&env, market_id, i, 0);
        }

        storage::set_market_collateral(&env, market_id, total_initial);
        storage::set_lp_total_supply(
            &env,
            market_id,
            isqrt(params.initial_liquidity * params.initial_liquidity)
                * params.num_outcomes as i128,
        );

        env.events().publish(
            (Symbol::new(&env, "MarketCreated"),),
            (
                market_id,
                creator,
                params.num_outcomes,
                params.initial_liquidity,
                params.trading_fee_bps,
            ),
        );

        market_id
    }

    /// Resolve a market via oracle data. Only callable by the market's oracle source.
    pub fn resolve_market(env: Env, oracle_caller: Address, market_id: u64, outcome: u32) {
        oracle_caller.require_auth();

        let mut market = storage::get_market(&env, market_id).expect("Market not found");

        // Verify oracle authorization
        if oracle_caller != market.oracle_source {
            if let Some(default) = storage::get_default_oracle(&env) {
                if oracle_caller != default {
                    panic!("Unauthorized oracle");
                }
            } else {
                panic!("Unauthorized oracle");
            }
        }

        if market.status == MarketStatus::Resolved {
            panic!("Market already resolved");
        }

        let now = env.ledger().timestamp();
        if now <= market.resolution_window_end {
            panic!("Resolution window not closed");
        }

        if outcome >= market.num_outcomes {
            panic!("Invalid outcome index");
        }

        market.status = MarketStatus::Resolved;
        market.resolved_outcome = Some(outcome);
        storage::set_market(&env, &market);

        env.events().publish(
            (Symbol::new(&env, "MarketResolved"),),
            (market_id, outcome, now),
        );
    }

    /// Early-close a market (admin or creator only).
    pub fn close_market_early(env: Env, caller: Address, market_id: u64) {
        caller.require_auth();

        let mut market = storage::get_market(&env, market_id).expect("Market not found");
        let is_admin = storage::get_admin(&env)
            .map(|a| a == caller)
            .unwrap_or(false);

        if !is_admin && caller != market.creator {
            panic!("Unauthorized");
        }

        if market.status != MarketStatus::Active {
            panic!("Market not active");
        }

        market.status = MarketStatus::Closed;
        storage::set_market(&env, &market);

        env.events().publish(
            (Symbol::new(&env, "MarketClosedEarly"),),
            (market_id, caller),
        );
    }

    // ── Trading (CPMM) ──────────────────────────────────────────────────

    /// Buy outcome tokens with collateral via the CPMM.
    /// Returns the amount of outcome tokens received.
    pub fn buy_outcome(
        env: Env,
        buyer: Address,
        market_id: u64,
        outcome_index: u32,
        collateral_in: i128,
        min_outcome_out: i128,
    ) -> i128 {
        buyer.require_auth();
        Self::assert_trading_active(&env, market_id);

        if collateral_in <= 0 {
            panic!("Invalid amount");
        }
        if outcome_index >= storage::get_market(&env, market_id).unwrap().num_outcomes {
            panic!("Invalid outcome index");
        }

        let mut pool = storage::get_outcome_pool(&env, market_id, outcome_index)
            .expect("Outcome pool not found");
        let market = storage::get_market(&env, market_id).unwrap();

        // Check market cap
        let current_supply = storage::get_total_outcome_supply(&env, market_id, outcome_index);
        let outcome_out = calculate_buy_amount(
            collateral_in,
            pool.collateral_reserve,
            pool.outcome_reserve,
            market.trading_fee_bps,
        );

        if outcome_out <= 0 {
            panic!("Output amount is zero");
        }
        if outcome_out < min_outcome_out {
            panic!("Slippage exceeded");
        }
        if current_supply + outcome_out > market.max_outcome_supply {
            panic!("Market cap exceeded");
        }

        // Transfer collateral from buyer to contract
        let contract_addr = env.current_contract_address();
        let collateral_client = TokenClient::new(&env, &market.collateral_token);
        collateral_client.transfer(&buyer, &contract_addr, &collateral_in);

        // Update CPMM reserves
        let fee_factor = 10_000 - market.trading_fee_bps as i128;
        let net_collateral = (collateral_in * fee_factor) / 10_000;
        pool.collateral_reserve += net_collateral;
        pool.outcome_reserve -= outcome_out;
        storage::set_outcome_pool(&env, market_id, outcome_index, &pool);

        // Credit outcome tokens to buyer
        let prev_balance = storage::get_outcome_balance(&env, market_id, outcome_index, &buyer);
        storage::set_outcome_balance(
            &env,
            market_id,
            outcome_index,
            &buyer,
            prev_balance + outcome_out,
        );

        // Update total supply
        storage::set_total_outcome_supply(
            &env,
            market_id,
            outcome_index,
            current_supply + outcome_out,
        );

        // Update user position
        Self::update_position(
            &env,
            market_id,
            outcome_index,
            &buyer,
            outcome_out,
            collateral_in,
        );

        // Update market total collateral
        let current_collateral = storage::get_market_collateral(&env, market_id);
        storage::set_market_collateral(&env, market_id, current_collateral + collateral_in);

        env.events().publish(
            (Symbol::new(&env, "OutcomeBought"),),
            (market_id, buyer, outcome_index, collateral_in, outcome_out),
        );

        outcome_out
    }

    /// Sell outcome tokens for collateral via the CPMM.
    /// Returns the amount of collateral received.
    pub fn sell_outcome(
        env: Env,
        seller: Address,
        market_id: u64,
        outcome_index: u32,
        outcome_amount: i128,
        min_collateral_out: i128,
    ) -> i128 {
        seller.require_auth();
        Self::assert_trading_active(&env, market_id);

        if outcome_amount <= 0 {
            panic!("Invalid amount");
        }
        if outcome_index >= storage::get_market(&env, market_id).unwrap().num_outcomes {
            panic!("Invalid outcome index");
        }

        let user_balance = storage::get_outcome_balance(&env, market_id, outcome_index, &seller);
        if user_balance < outcome_amount {
            panic!("Insufficient balance");
        }

        let mut pool = storage::get_outcome_pool(&env, market_id, outcome_index)
            .expect("Outcome pool not found");
        let market = storage::get_market(&env, market_id).unwrap();

        let collateral_out = calculate_sell_amount(
            outcome_amount,
            pool.collateral_reserve,
            pool.outcome_reserve,
            market.trading_fee_bps,
        );

        if collateral_out <= 0 {
            panic!("Output amount is zero");
        }
        if collateral_out < min_collateral_out {
            panic!("Slippage exceeded");
        }

        // Update CPMM reserves
        pool.collateral_reserve -= collateral_out;
        pool.outcome_reserve += outcome_amount;
        storage::set_outcome_pool(&env, market_id, outcome_index, &pool);

        // Debit outcome tokens from seller
        storage::set_outcome_balance(
            &env,
            market_id,
            outcome_index,
            &seller,
            user_balance - outcome_amount,
        );

        // Transfer collateral from contract to seller
        let contract_addr = env.current_contract_address();
        let collateral_client = TokenClient::new(&env, &market.collateral_token);
        collateral_client.transfer(&contract_addr, &seller, &collateral_out);

        // Update user position
        let entry_price = if user_balance > 0 {
            let pos = storage::get_user_position(&env, market_id, outcome_index, &seller);
            pos.map(|p| p.avg_entry_price).unwrap_or(0)
        } else {
            0
        };
        Self::update_position_on_sell(
            &env,
            market_id,
            outcome_index,
            &seller,
            outcome_amount,
            collateral_out,
            entry_price,
        );

        // Update market total collateral
        let current_collateral = storage::get_market_collateral(&env, market_id);
        storage::set_market_collateral(&env, market_id, current_collateral - collateral_out);

        // Update total supply
        let current_supply = storage::get_total_outcome_supply(&env, market_id, outcome_index);
        storage::set_total_outcome_supply(
            &env,
            market_id,
            outcome_index,
            current_supply - outcome_amount,
        );

        env.events().publish(
            (Symbol::new(&env, "OutcomeSold"),),
            (
                market_id,
                seller,
                outcome_index,
                outcome_amount,
                collateral_out,
            ),
        );

        collateral_out
    }

    /// Get the buy quote for a given collateral amount (does not execute).
    pub fn quote_buy(env: Env, market_id: u64, outcome_index: u32, collateral_in: i128) -> i128 {
        let pool = storage::get_outcome_pool(&env, market_id, outcome_index)
            .expect("Outcome pool not found");
        let market = storage::get_market(&env, market_id).unwrap();
        calculate_buy_amount(
            collateral_in,
            pool.collateral_reserve,
            pool.outcome_reserve,
            market.trading_fee_bps,
        )
    }

    /// Get the sell quote for a given outcome amount (does not execute).
    pub fn quote_sell(env: Env, market_id: u64, outcome_index: u32, outcome_amount: i128) -> i128 {
        let pool = storage::get_outcome_pool(&env, market_id, outcome_index)
            .expect("Outcome pool not found");
        let market = storage::get_market(&env, market_id).unwrap();
        calculate_sell_amount(
            outcome_amount,
            pool.collateral_reserve,
            pool.outcome_reserve,
            market.trading_fee_bps,
        )
    }

    // ── Liquidity Provision ─────────────────────────────────────────────

    /// Add liquidity to an outcome pool. Mints LP shares proportionally.
    /// Returns the number of LP shares minted.
    pub fn add_liquidity(
        env: Env,
        provider: Address,
        market_id: u64,
        outcome_index: u32,
        collateral_amount: i128,
        outcome_amount: i128,
    ) -> i128 {
        provider.require_auth();
        Self::assert_market_active(&env, market_id);

        if collateral_amount <= 0 || outcome_amount <= 0 {
            panic!("Amounts must be positive");
        }
        if outcome_index >= storage::get_market(&env, market_id).unwrap().num_outcomes {
            panic!("Invalid outcome index");
        }

        let mut pool = storage::get_outcome_pool(&env, market_id, outcome_index)
            .expect("Outcome pool not found");

        let lp_shares = calculate_lp_shares_add(collateral_amount, outcome_amount, &pool);
        if lp_shares <= 0 {
            panic!("Insufficient liquidity minted");
        }

        // Transfer collateral from provider
        let contract_addr = env.current_contract_address();
        let market = storage::get_market(&env, market_id).unwrap();
        let collateral_client = TokenClient::new(&env, &market.collateral_token);
        collateral_client.transfer(&provider, &contract_addr, &collateral_amount);

        // Update pool reserves
        pool.collateral_reserve += collateral_amount;
        pool.outcome_reserve += outcome_amount;
        pool.lp_total_supply += lp_shares;
        storage::set_outcome_pool(&env, market_id, outcome_index, &pool);

        // Credit LP shares
        let prev_shares = storage::get_lp_shares(&env, market_id, &provider);
        storage::set_lp_shares(&env, market_id, &provider, prev_shares + lp_shares);

        let prev_total = storage::get_lp_total_supply(&env, market_id);
        storage::set_lp_total_supply(&env, market_id, prev_total + lp_shares);

        // Update market collateral
        let current_collateral = storage::get_market_collateral(&env, market_id);
        storage::set_market_collateral(&env, market_id, current_collateral + collateral_amount);

        env.events().publish(
            (Symbol::new(&env, "LiquidityAdded"),),
            (
                market_id,
                provider,
                outcome_index,
                collateral_amount,
                outcome_amount,
                lp_shares,
            ),
        );

        lp_shares
    }

    /// Remove liquidity from an outcome pool.
    /// Returns (collateral_out, outcome_out).
    pub fn remove_liquidity(
        env: Env,
        provider: Address,
        market_id: u64,
        outcome_index: u32,
        lp_amount: i128,
    ) -> (i128, i128) {
        provider.require_auth();

        if lp_amount <= 0 {
            panic!("LP amount must be positive");
        }

        let user_shares = storage::get_lp_shares(&env, market_id, &provider);
        if user_shares < lp_amount {
            panic!("Insufficient LP tokens");
        }

        let pool = storage::get_outcome_pool(&env, market_id, outcome_index)
            .expect("Outcome pool not found");
        if pool.lp_total_supply <= 0 {
            panic!("No liquidity");
        }

        let (collateral_out, outcome_out) = calculate_lp_withdraw(lp_amount, &pool);

        // Transfer collateral back
        let contract_addr = env.current_contract_address();
        let market = storage::get_market(&env, market_id).unwrap();
        let collateral_client = TokenClient::new(&env, &market.collateral_token);
        collateral_client.transfer(&contract_addr, &provider, &collateral_out);

        // Update pool
        let mut pool = pool;
        pool.collateral_reserve -= collateral_out;
        pool.outcome_reserve -= outcome_out;
        pool.lp_total_supply -= lp_amount;
        storage::set_outcome_pool(&env, market_id, outcome_index, &pool);

        // Debit LP shares
        storage::set_lp_shares(&env, market_id, &provider, user_shares - lp_amount);
        let prev_total = storage::get_lp_total_supply(&env, market_id);
        storage::set_lp_total_supply(&env, market_id, prev_total - lp_amount);

        // Credit outcome tokens to provider
        let prev_balance = storage::get_outcome_balance(&env, market_id, outcome_index, &provider);
        storage::set_outcome_balance(
            &env,
            market_id,
            outcome_index,
            &provider,
            prev_balance + outcome_out,
        );

        // Update market collateral
        let current_collateral = storage::get_market_collateral(&env, market_id);
        storage::set_market_collateral(&env, market_id, current_collateral - collateral_out);

        env.events().publish(
            (Symbol::new(&env, "LiquidityRemoved"),),
            (
                market_id,
                provider,
                outcome_index,
                collateral_out,
                outcome_out,
                lp_amount,
            ),
        );

        (collateral_out, outcome_out)
    }

    // ── Order Book ──────────────────────────────────────────────────────

    /// Place a limit order on the order book.
    pub fn place_order(
        env: Env,
        owner: Address,
        market_id: u64,
        outcome_index: u32,
        side: OrderSide,
        price: i128,
        quantity: i128,
    ) -> u64 {
        owner.require_auth();
        Self::assert_trading_active(&env, market_id);

        if price <= 0 || quantity <= 0 {
            panic!("Invalid price or quantity");
        }
        if outcome_index >= storage::get_market(&env, market_id).unwrap().num_outcomes {
            panic!("Invalid outcome index");
        }

        // For sell orders, ensure the user has enough outcome tokens
        if side == OrderSide::Sell {
            let balance = storage::get_outcome_balance(&env, market_id, outcome_index, &owner);
            if balance < quantity {
                panic!("Insufficient balance for sell order");
            }
        }

        // For buy orders, ensure the user has enough collateral
        if side == OrderSide::Buy {
            let cost = (price * quantity) / DECIMAL_FACTOR;
            let contract_addr = env.current_contract_address();
            let market = storage::get_market(&env, market_id).unwrap();
            let collateral_client = TokenClient::new(&env, &market.collateral_token);
            let balance = collateral_client.balance(&owner);
            if balance < cost {
                panic!("Insufficient collateral for order");
            }
            // Lock collateral
            collateral_client.transfer(&owner, &contract_addr, &cost);
        }

        let order_id = storage::get_order_counter(&env, market_id);
        let now = env.ledger().timestamp();

        let order = LimitOrder {
            order_id,
            market_id,
            owner: owner.clone(),
            outcome_index,
            side,
            price,
            quantity,
            filled_quantity: 0,
            status: OrderStatus::Open,
            created_at: now,
            expires_at: None,
        };

        storage::set_order(&env, &order);
        storage::set_order_counter(&env, market_id, order_id + 1);

        env.events().publish(
            (Symbol::new(&env, "OrderPlaced"),),
            (
                market_id,
                order_id,
                owner,
                outcome_index,
                side as u32,
                price,
                quantity,
            ),
        );

        order_id
    }

    /// Cancel an open order. Returns locked collateral for buy orders.
    pub fn cancel_order(env: Env, owner: Address, market_id: u64, order_id: u64) {
        owner.require_auth();

        let mut order = storage::get_order(&env, market_id, order_id).expect("Order not found");

        if order.owner != owner {
            panic!("Unauthorized");
        }
        if order.status != OrderStatus::Open {
            panic!("Order not open");
        }

        order.status = OrderStatus::Cancelled;
        storage::set_order(&env, &order);

        // Return locked collateral for buy orders
        if order.side == OrderSide::Buy {
            let remaining = order.quantity - order.filled_quantity;
            let cost = (order.price * remaining) / DECIMAL_FACTOR;
            if cost > 0 {
                let contract_addr = env.current_contract_address();
                let market = storage::get_market(&env, market_id).unwrap();
                let collateral_client = TokenClient::new(&env, &market.collateral_token);
                collateral_client.transfer(&contract_addr, &owner, &cost);
            }
        }

        env.events().publish(
            (Symbol::new(&env, "OrderCancelled"),),
            (market_id, order_id, owner),
        );
    }

    /// Match a sell order against open buy orders on the order book.
    pub fn match_sell_order(env: Env, seller: Address, market_id: u64, order_id: u64) -> i128 {
        seller.require_auth();

        let mut sell_order =
            storage::get_order(&env, market_id, order_id).expect("Order not found");
        if sell_order.owner != seller {
            panic!("Unauthorized");
        }
        if sell_order.status != OrderStatus::Open {
            panic!("Order not open");
        }
        if sell_order.side != OrderSide::Sell {
            panic!("Not a sell order");
        }

        let mut remaining = sell_order.quantity - sell_order.filled_quantity;
        let mut total_collateral_received: i128 = 0;

        // Scan buy orders (simplified: linear scan)
        let order_counter = storage::get_order_counter(&env, market_id);
        for oid in 0..order_counter {
            if remaining <= 0 {
                break;
            }

            let mut buy_order = match storage::get_order(&env, market_id, oid) {
                Some(o) => o,
                None => continue,
            };

            if buy_order.status != OrderStatus::Open {
                continue;
            }
            if buy_order.side != OrderSide::Buy {
                continue;
            }
            if buy_order.outcome_index != sell_order.outcome_index {
                continue;
            }
            if buy_order.owner == seller {
                continue;
            }
            if buy_order.price < sell_order.price {
                continue;
            }

            let buy_remaining = buy_order.quantity - buy_order.filled_quantity;
            let fill_amount = if remaining < buy_remaining {
                remaining
            } else {
                buy_remaining
            };

            // Calculate collateral transfer
            let collateral = (buy_order.price * fill_amount) / DECIMAL_FACTOR;
            total_collateral_received += collateral;

            // Update buy order
            buy_order.filled_quantity += fill_amount;
            buy_order.status = if buy_order.filled_quantity >= buy_order.quantity {
                OrderStatus::Filled
            } else {
                OrderStatus::Open
            };
            storage::set_order(&env, &buy_order);

            // Credit outcome tokens to buyer
            let buyer_balance = storage::get_outcome_balance(
                &env,
                market_id,
                sell_order.outcome_index,
                &buy_order.owner,
            );
            storage::set_outcome_balance(
                &env,
                market_id,
                sell_order.outcome_index,
                &buy_order.owner,
                buyer_balance + fill_amount,
            );

            remaining -= fill_amount;
        }

        // Update sell order
        sell_order.filled_quantity = sell_order.quantity - remaining;
        sell_order.status = if remaining == 0 {
            OrderStatus::Filled
        } else {
            OrderStatus::Open
        };
        storage::set_order(&env, &sell_order);

        // Debit outcome tokens from seller
        let seller_balance =
            storage::get_outcome_balance(&env, market_id, sell_order.outcome_index, &seller);
        let sold = sell_order.filled_quantity;
        storage::set_outcome_balance(
            &env,
            market_id,
            sell_order.outcome_index,
            &seller,
            seller_balance - sold,
        );

        // Transfer collateral to seller
        if total_collateral_received > 0 {
            let contract_addr = env.current_contract_address();
            let market = storage::get_market(&env, market_id).unwrap();
            let collateral_client = TokenClient::new(&env, &market.collateral_token);
            collateral_client.transfer(&contract_addr, &seller, &total_collateral_received);
        }

        env.events().publish(
            (Symbol::new(&env, "OrderMatched"),),
            (market_id, order_id, seller, sold, total_collateral_received),
        );

        total_collateral_received
    }

    // ── Settlement & Redemption ─────────────────────────────────────────

    /// Redeem winning outcome tokens after market resolution.
    pub fn redeem_winning_tokens(env: Env, user: Address, market_id: u64) -> i128 {
        user.require_auth();

        let market = storage::get_market(&env, market_id).expect("Market not found");
        if market.status != MarketStatus::Resolved {
            panic!("Market not resolved");
        }

        let winning_outcome = match market.resolved_outcome {
            Some(o) => o,
            None => panic!("No resolved outcome"),
        };

        let balance = storage::get_outcome_balance(&env, market_id, winning_outcome, &user);
        if balance <= 0 {
            panic!("No winning tokens to redeem");
        }

        let collateral_amount = balance; // 1:1 redemption

        // Check the pool has enough collateral
        let pool =
            storage::get_outcome_pool(&env, market_id, winning_outcome).expect("Pool not found");
        if pool.collateral_reserve < collateral_amount {
            // Partial redemption if pool is depleted
            let payout = pool.collateral_reserve;
            let contract_addr = env.current_contract_address();
            let collateral_client = TokenClient::new(&env, &market.collateral_token);
            collateral_client.transfer(&contract_addr, &user, &payout);

            storage::set_outcome_balance(&env, market_id, winning_outcome, &user, 0);

            let mut pool = pool;
            pool.collateral_reserve = 0;
            storage::set_outcome_pool(&env, market_id, winning_outcome, &pool);

            let current_collateral = storage::get_market_collateral(&env, market_id);
            storage::set_market_collateral(&env, market_id, current_collateral - payout);

            env.events().publish(
                (Symbol::new(&env, "PartialRedemption"),),
                (market_id, user, winning_outcome, payout),
            );
            return payout;
        }

        // Full redemption
        let contract_addr = env.current_contract_address();
        let collateral_client = TokenClient::new(&env, &market.collateral_token);
        collateral_client.transfer(&contract_addr, &user, &collateral_amount);

        storage::set_outcome_balance(&env, market_id, winning_outcome, &user, 0);

        let mut pool = pool;
        pool.collateral_reserve -= collateral_amount;
        storage::set_outcome_pool(&env, market_id, winning_outcome, &pool);

        let current_collateral = storage::get_market_collateral(&env, market_id);
        storage::set_market_collateral(&env, market_id, current_collateral - collateral_amount);

        env.events().publish(
            (Symbol::new(&env, "TokensRedeemed"),),
            (market_id, user, winning_outcome, balance, collateral_amount),
        );

        collateral_amount
    }

    // ── Dispute Mechanism ───────────────────────────────────────────────

    /// Open a dispute against a market resolution. Requires a stake.
    pub fn open_dispute(
        env: Env,
        challenger: Address,
        market_id: u64,
        claimed_outcome: u32,
        evidence: String,
        stake_amount: i128,
    ) -> u64 {
        challenger.require_auth();

        let market = storage::get_market(&env, market_id).expect("Market not found");
        if market.status != MarketStatus::Resolved {
            panic!("Market must be resolved to dispute");
        }
        if claimed_outcome >= market.num_outcomes {
            panic!("Invalid outcome index");
        }
        if stake_amount <= 0 {
            panic!("Stake required");
        }

        // Lock stake
        let contract_addr = env.current_contract_address();
        let collateral_client = TokenClient::new(&env, &market.collateral_token);
        collateral_client.transfer(&challenger, &contract_addr, &stake_amount);

        let dispute_id = storage::get_dispute_counter(&env, market_id);
        let now = env.ledger().timestamp();

        let dispute = Dispute {
            dispute_id,
            market_id,
            challenger: challenger.clone(),
            claimed_outcome,
            evidence,
            stake_amount,
            status: DisputeStatus::Open,
            votes_for: 0,
            votes_against: 0,
            created_at: now,
            resolved_at: None,
        };

        storage::set_dispute(&env, &dispute);
        storage::set_dispute_counter(&env, market_id, dispute_id + 1);

        // Put market into disputed state
        let mut market = market;
        market.status = MarketStatus::Disputed;
        storage::set_market(&env, &market);

        env.events().publish(
            (Symbol::new(&env, "DisputeOpened"),),
            (
                market_id,
                dispute_id,
                challenger,
                claimed_outcome,
                stake_amount,
            ),
        );

        dispute_id
    }

    /// Vote on a dispute. Weight is based on the voter's outcome token holdings.
    pub fn vote_dispute(
        env: Env,
        voter: Address,
        market_id: u64,
        dispute_id: u64,
        support_challenger: bool,
    ) {
        voter.require_auth();

        let dispute = storage::get_dispute(&env, market_id, dispute_id).expect("Dispute not found");
        if dispute.status != DisputeStatus::Open && dispute.status != DisputeStatus::Voted {
            panic!("Dispute not open");
        }
        if storage::has_voted(&env, market_id, dispute_id, &voter) {
            panic!("Already voted");
        }

        // Weight = total outcome token balance across all outcomes
        let market = storage::get_market(&env, market_id).unwrap();
        let mut weight: i128 = 0;
        for i in 0..market.num_outcomes {
            weight += storage::get_outcome_balance(&env, market_id, i, &voter);
        }

        if weight <= 0 {
            panic!("No tokens to vote with");
        }

        storage::set_vote(&env, market_id, dispute_id, &voter, weight);

        let mut dispute = dispute;
        if support_challenger {
            dispute.votes_for += weight;
        } else {
            dispute.votes_against += weight;
        }
        dispute.status = DisputeStatus::Voted;
        storage::set_dispute(&env, &dispute);

        env.events().publish(
            (Symbol::new(&env, "DisputeVoteCast"),),
            (market_id, dispute_id, voter, weight, support_challenger),
        );
    }

    /// Resolve a dispute. If votes_for > votes_against, challenge succeeds.
    pub fn resolve_dispute(env: Env, admin: Address, market_id: u64, dispute_id: u64) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut dispute =
            storage::get_dispute(&env, market_id, dispute_id).expect("Dispute not found");
        if dispute.status == DisputeStatus::ResolvedUpheld
            || dispute.status == DisputeStatus::ResolvedRejected
        {
            panic!("Dispute already resolved");
        }

        let now = env.ledger().timestamp();

        if dispute.votes_for > dispute.votes_against {
            // Challenge succeeds: update market outcome
            let mut market = storage::get_market(&env, market_id).unwrap();
            market.resolved_outcome = Some(dispute.claimed_outcome);
            market.status = MarketStatus::Resolved;
            storage::set_market(&env, &market);

            // Return stake to challenger
            let contract_addr = env.current_contract_address();
            let collateral_client = TokenClient::new(&env, &market.collateral_token);
            collateral_client.transfer(&contract_addr, &dispute.challenger, &dispute.stake_amount);

            dispute.status = DisputeStatus::ResolvedUpheld;
            dispute.resolved_at = Some(now);
            storage::set_dispute(&env, &dispute);

            env.events().publish(
                (Symbol::new(&env, "DisputeUpheld"),),
                (
                    market_id,
                    dispute_id,
                    dispute.claimed_outcome,
                    dispute.stake_amount,
                ),
            );
        } else {
            // Challenge fails: original outcome stands, stake burned to governance
            let market = storage::get_market(&env, market_id).unwrap();
            if let Some(collector) = storage::get_governance_collector(&env) {
                let contract_addr = env.current_contract_address();
                let collateral_client = TokenClient::new(&env, &market.collateral_token);
                collateral_client.transfer(&contract_addr, &collector, &dispute.stake_amount);
            }

            dispute.status = DisputeStatus::ResolvedRejected;
            dispute.resolved_at = Some(now);
            storage::set_dispute(&env, &dispute);

            // Restore market to resolved
            let mut market = market;
            market.status = MarketStatus::Resolved;
            storage::set_market(&env, &market);

            env.events().publish(
                (Symbol::new(&env, "DisputeRejected"),),
                (market_id, dispute_id, dispute.stake_amount),
            );
        }
    }

    // ── View Functions ──────────────────────────────────────────────────

    /// Get full market data.
    pub fn query_market(env: Env, market_id: u64) -> PredictionMarketV2 {
        storage::get_market(&env, market_id).expect("Market not found")
    }

    /// Get the CPMM pool for a specific outcome.
    pub fn query_outcome_pool(env: Env, market_id: u64, outcome_index: u32) -> OutcomePool {
        storage::get_outcome_pool(&env, market_id, outcome_index).expect("Outcome pool not found")
    }

    /// Get a user's outcome token balance.
    pub fn query_user_balance(env: Env, market_id: u64, outcome_index: u32, user: Address) -> i128 {
        storage::get_outcome_balance(&env, market_id, outcome_index, &user)
    }

    /// Get a user's position tracking record.
    pub fn query_user_position(
        env: Env,
        market_id: u64,
        outcome_index: u32,
        user: Address,
    ) -> Option<UserOutcomePosition> {
        storage::get_user_position(&env, market_id, outcome_index, &user)
    }

    /// Get LP share balance for a provider.
    pub fn query_lp_shares(env: Env, market_id: u64, provider: Address) -> i128 {
        storage::get_lp_shares(&env, market_id, &provider)
    }

    /// Get total LP supply for a market.
    pub fn query_lp_total_supply(env: Env, market_id: u64) -> i128 {
        storage::get_lp_total_supply(&env, market_id)
    }

    /// Get the implied price of an outcome (0 to 1e18).
    pub fn query_outcome_price(env: Env, market_id: u64, outcome_index: u32) -> i128 {
        let pool = storage::get_outcome_pool(&env, market_id, outcome_index)
            .expect("Outcome pool not found");
        get_outcome_price(pool.collateral_reserve, pool.outcome_reserve)
    }

    /// Get total supply of an outcome token.
    pub fn query_total_outcome_supply(env: Env, market_id: u64, outcome_index: u32) -> i128 {
        storage::get_total_outcome_supply(&env, market_id, outcome_index)
    }

    /// Get an order by ID.
    pub fn query_order(env: Env, market_id: u64, order_id: u64) -> LimitOrder {
        storage::get_order(&env, market_id, order_id).expect("Order not found")
    }

    /// Get a dispute by ID.
    pub fn query_dispute(env: Env, market_id: u64, dispute_id: u64) -> Dispute {
        storage::get_dispute(&env, market_id, dispute_id).expect("Dispute not found")
    }

    /// Get the market's total collateral.
    pub fn query_market_collateral(env: Env, market_id: u64) -> i128 {
        storage::get_market_collateral(&env, market_id)
    }

    // ── Admin Functions ─────────────────────────────────────────────────

    /// Pause all trading (admin only).
    pub fn admin_pause_trading(env: Env, admin: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        storage::set_trading_paused(&env, true);
        env.events().publish(
            (symbol_short!("trd_pause"),),
            (admin, env.ledger().timestamp()),
        );
    }

    /// Resume all trading (admin only).
    pub fn admin_resume_trading(env: Env, admin: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        storage::set_trading_paused(&env, false);
        env.events().publish(
            (symbol_short!("trd_res"),),
            (admin, env.ledger().timestamp()),
        );
    }

    /// Set a new admin (admin only).
    pub fn admin_set_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::assert_admin(&env, &current_admin);
        storage::set_admin(&env, &new_admin);
        env.events()
            .publish((symbol_short!("adm_set"),), (current_admin, new_admin));
    }

    /// Set the governance fee collector and protocol fee share (admin only).
    pub fn admin_set_fee_config(env: Env, admin: Address, collector: Address, fee_share_bps: i128) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if !(0..=5000).contains(&fee_share_bps) {
            panic!("Invalid fee config");
        }
        storage::set_governance_collector(&env, &collector);
        storage::set_fee_share_bps(&env, fee_share_bps);
    }

    /// Set the default oracle (admin only).
    pub fn admin_set_oracle(env: Env, admin: Address, oracle: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        storage::set_default_oracle(&env, &oracle);
    }

    // ── Internal Helpers ────────────────────────────────────────────────

    fn assert_admin(env: &Env, caller: &Address) {
        let admin = storage::get_admin(env).expect("Admin not set");
        if *caller != admin {
            panic!("Unauthorized");
        }
    }

    fn assert_trading_active(env: &Env, market_id: u64) {
        if is_trading_paused(env) {
            panic!("Trading is paused");
        }
        let market = get_market(env, market_id).expect("Market not found");
        if market.status != MarketStatus::Active {
            panic!("Market not active");
        }
    }

    fn assert_market_active(env: &Env, market_id: u64) {
        let market = get_market(env, market_id).expect("Market not found");
        if market.status != MarketStatus::Active {
            panic!("Market not active");
        }
    }

    /// Update user position after a buy.
    fn update_position(
        env: &Env,
        market_id: u64,
        outcome_index: u32,
        user: &Address,
        quantity: i128,
        collateral_spent: i128,
    ) {
        let now = env.ledger().timestamp();
        let existing = get_user_position(env, market_id, outcome_index, user);

        let position = match existing {
            Some(mut pos) => {
                let total_cost =
                    pos.avg_entry_price * pos.quantity / DECIMAL_FACTOR + collateral_spent;
                pos.quantity += quantity;
                pos.avg_entry_price = if pos.quantity > 0 {
                    (total_cost * DECIMAL_FACTOR) / pos.quantity
                } else {
                    0
                };
                pos.updated_at = now;
                pos
            }
            None => UserOutcomePosition {
                market_id,
                outcome_index,
                owner: user.clone(),
                quantity,
                avg_entry_price: if quantity > 0 {
                    (collateral_spent * DECIMAL_FACTOR) / quantity
                } else {
                    0
                },
                realized_pnl: 0,
                created_at: now,
                updated_at: now,
            },
        };

        set_user_position(env, &position);
    }

    /// Update user position after a sell.
    fn update_position_on_sell(
        env: &Env,
        market_id: u64,
        outcome_index: u32,
        user: &Address,
        quantity: i128,
        collateral_received: i128,
        entry_price: i128,
    ) {
        let now = env.ledger().timestamp();
        let existing = get_user_position(env, market_id, outcome_index, user);

        let position = match existing {
            Some(mut pos) => {
                let cost_basis = (entry_price * quantity) / DECIMAL_FACTOR;
                let pnl = collateral_received - cost_basis;
                pos.realized_pnl += pnl;
                pos.quantity -= quantity;
                pos.updated_at = now;
                pos
            }
            None => UserOutcomePosition {
                market_id,
                outcome_index,
                owner: user.clone(),
                quantity: -quantity,
                avg_entry_price: 0,
                realized_pnl: if quantity > 0 { collateral_received } else { 0 },
                created_at: now,
                updated_at: now,
            },
        };

        if position.quantity <= 0 && position.realized_pnl == 0 {
            env.storage().persistent().remove(&DataKey::UserPosition(
                market_id,
                outcome_index,
                user.clone(),
            ));
        } else {
            set_user_position(env, &position);
        }
    }
}
