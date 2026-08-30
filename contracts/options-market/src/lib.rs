#![no_std]
#![allow(clippy::too_many_arguments)]
extern crate alloc;

mod errors;
mod math;
mod oracle_integration;
mod storage;
mod types;

#[cfg(test)]
mod tests;

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol};

use errors::*;
use math::*;
use oracle_integration as oracle;
use storage::*;
use types::*;

// ══════════════════════════════════════════════════════════════════════════════
//  CONTRACT
// ══════════════════════════════════════════════════════════════════════════════

#[contract]
pub struct OptionsMarket;

#[contractimpl]
impl OptionsMarket {
    // ════════════════════════════════════════════════════════════════════════
    //  INITIALIZATION
    // ════════════════════════════════════════════════════════════════════════

    pub fn initialize(env: Env, admin: Address, oracle_addr: Address, treasury: Address) {
        if get_admin(&env).is_some() {
            panic!("Already initialized");
        }
        admin.require_auth();

        set_admin(&env, &admin);
        set_oracle(&env, &oracle_addr);
        set_treasury(&env, &treasury);
        set_paused(&env, false);
        set_option_counter(&env, 0);
        set_listing_counter(&env, 0);
        set_total_exposure(&env, 0);
        set_max_position_per_user(&env, 1_000_000 * PRECISION);
        set_max_total_exposure(&env, 100_000_000 * PRECISION);

        // Initialize collateral pools
        let xlm = Symbol::new(&env, "XLM");
        let usdc = Symbol::new(&env, "USDC");
        set_pool_collateral(&env, &xlm, 0);
        set_pool_collateral(&env, &usdc, 0);

        env.events().publish(
            (symbol_short!("opt_init"),),
            (admin, oracle_addr, treasury, env.ledger().timestamp()),
        );
    }

    // ════════════════════════════════════════════════════════════════════════
    //  OPTION SERIES CREATION
    // ════════════════════════════════════════════════════════════════════════

    /// Create a new option series with specified parameters.
    /// Admin function to set up option chains.
    pub fn create_option_series(env: Env, admin: Address, params: CreateSeriesParams) -> Vec<u64> {
        admin.require_auth();
        assert_admin(&env, &admin);
        check_paused(&env);

        let now = env.ledger().timestamp();
        let mut created_ids = Vec::new(&env);

        for i in 0..params.strike_prices.len() {
            let strike = params.strike_prices.get(i).unwrap();

            if strike <= 0 {
                panic!("Invalid strike price");
            }
            if params.expiration <= now {
                panic!("Expiration must be in the future");
            }
            if params.expiration > now + MAX_TIME_TO_EXPIRY_SECONDS {
                panic!("Expiration too far in the future");
            }

            let series_id = get_option_counter(&env);

            let series = OptionSeriesConfig {
                series_id,
                underlying: params.underlying,
                strike_price: strike,
                expiration: params.expiration,
                option_type: params.option_type,
                option_style: params.option_style,
                created_at: now,
                total_open_interest: 0,
                max_open_interest: params.max_open_interest_per_strike,
                is_active: true,
            };

            set_option_series(&env, &series);
            set_option_counter(&env, series_id + 1);
            created_ids.push_back(series_id);

            env.events().publish(
                (symbol_short!("series_crt"),),
                (
                    series_id,
                    params.underlying as u32,
                    strike,
                    params.expiration,
                    params.option_type as u32,
                    params.option_style as u32,
                ),
            );
        }

        created_ids
    }

    /// Deactivate an option series (admin only).
    pub fn deactivate_series(env: Env, admin: Address, series_id: u64) {
        admin.require_auth();
        assert_admin(&env, &admin);

        let mut series = get_option_series(&env, series_id).expect("Series not found");
        series.is_active = false;
        set_option_series(&env, &series);

        env.events()
            .publish((symbol_short!("series_off"),), (series_id,));
    }

    // ════════════════════════════════════════════════════════════════════════
    //  OPTION WRITING (SELLING)
    // ════════════════════════════════════════════════════════════════════════

    /// Write (sell) an option contract. The writer receives premium and locks collateral.
    pub fn write_option(env: Env, writer: Address, series_id: u64, size: i128) -> u64 {
        writer.require_auth();
        check_paused(&env);

        if size <= 0 {
            panic!("Invalid size");
        }

        // Check circuit breaker
        if is_circuit_breaker_active(&env) {
            panic!("Circuit breaker is active");
        }

        let series = get_option_series(&env, series_id).expect("Series not found");
        if !series.is_active {
            panic!("Series not active");
        }

        let now = env.ledger().timestamp();
        if series.expiration <= now {
            panic!("Series already expired");
        }

        // Check open interest limit
        if series.total_open_interest + size > series.max_open_interest {
            panic!("Max open interest exceeded");
        }

        // Get current price and volatility
        let underlying_symbol = oracle::underlying_to_symbol(&env, series.underlying);
        let spot_price = oracle::get_oracle_price(&env, series.underlying);
        let volatility = oracle::get_volatility(&env, series.underlying);

        // Calculate premium using Black-Scholes
        let tte = time_to_expiry_years(now, series.expiration);
        let rate = DEFAULT_RISK_FREE_RATE;
        let premium_per_unit = black_scholes_price(
            spot_price,
            series.strike_price,
            tte,
            rate,
            volatility,
            series.option_type,
        )
        .unwrap_or(0);

        if premium_per_unit <= 0 {
            panic!("Invalid premium: option is out of the money");
        }

        let total_premium = premium_per_unit * size / PRECISION;

        // Calculate required collateral
        let collateral_required = calculate_collateral_required(
            series.strike_price,
            size,
            series.option_type,
            spot_price,
        );

        // Check writer has enough collateral
        let current_collateral = get_writer_collateral(&env, &writer, &underlying_symbol);
        if current_collateral < collateral_required {
            panic!("Insufficient collateral");
        }

        // Check position limits
        let user_exposure = get_user_exposure(&env, &writer);
        if user_exposure + size > get_max_position_per_user(&env) {
            panic!("Position limit exceeded");
        }

        let total_exp = get_total_exposure(&env);
        if total_exp + size > get_max_total_exposure(&env) {
            panic!("Total exposure limit exceeded");
        }

        // Create option ID
        let option_id = get_option_counter(&env);

        // Lock collateral
        set_writer_collateral(
            &env,
            &writer,
            &underlying_symbol,
            current_collateral - collateral_required,
        );

        // Update pool collateral
        let pool_coll = get_pool_collateral(&env, &underlying_symbol);
        set_pool_collateral(&env, &underlying_symbol, pool_coll + collateral_required);

        // Create position for the writer
        let position = OptionPosition {
            option_id,
            series_id,
            holder: writer.clone(),
            writer: writer.clone(),
            underlying: series.underlying,
            option_type: series.option_type,
            option_style: series.option_style,
            strike_price: series.strike_price,
            current_price: spot_price,
            expiration: series.expiration,
            premium_paid: 0,
            collateral_locked: collateral_required,
            size,
            status: OptionStatus::Active,
            created_at: now,
            exercised_at: None,
            settled_at: None,
        };

        set_option_position(&env, &position);
        add_user_option_id(&env, &writer, option_id);
        set_option_counter(&env, option_id + 1);

        // Update series open interest
        let mut series = series;
        series.total_open_interest += size;
        set_option_series(&env, &series);

        // Update exposure tracking
        set_total_exposure(&env, total_exp + size);
        set_user_exposure(&env, &writer, user_exposure + size);

        // Calculate and cache Greeks
        let greeks = calculate_all_greeks(
            spot_price,
            series.strike_price,
            tte,
            rate,
            volatility,
            series.option_type,
        );
        set_greeks_cache(&env, option_id, &greeks);

        env.events().publish(
            (symbol_short!("opt_write"),),
            (
                option_id,
                series_id,
                writer,
                size,
                premium_per_unit,
                collateral_required,
                spot_price,
                now,
            ),
        );

        option_id
    }

    /// Buy an option from a specific series. Creates a new position for the buyer.
    pub fn buy_from_series(env: Env, buyer: Address, series_id: u64, size: i128) -> u64 {
        buyer.require_auth();
        check_paused(&env);

        if size <= 0 {
            panic!("Invalid size");
        }

        if is_circuit_breaker_active(&env) {
            panic!("Circuit breaker is active");
        }

        let series = get_option_series(&env, series_id).expect("Series not found");
        if !series.is_active {
            panic!("Series not active");
        }

        let now = env.ledger().timestamp();
        if series.expiration <= now {
            panic!("Series already expired");
        }

        // Get current price and volatility
        let spot_price = oracle::get_oracle_price(&env, series.underlying);
        let volatility = oracle::get_volatility(&env, series.underlying);

        // Calculate premium
        let tte = time_to_expiry_years(now, series.expiration);
        let rate = DEFAULT_RISK_FREE_RATE;
        let premium_per_unit = black_scholes_price(
            spot_price,
            series.strike_price,
            tte,
            rate,
            volatility,
            series.option_type,
        )
        .unwrap_or(0);

        let total_premium = premium_per_unit * size / PRECISION;

        // Check buyer can pay premium
        let underlying_symbol = oracle::underlying_to_symbol(&env, series.underlying);
        let buyer_collateral = get_writer_collateral(&env, &buyer, &underlying_symbol);

        if buyer_collateral < total_premium {
            // Transfer premium from buyer to contract (simplified - in production use token transfer)
            // For now, deduct from buyer's collateral balance
            set_writer_collateral(&env, &buyer, &underlying_symbol, 0);
        } else {
            set_writer_collateral(
                &env,
                &buyer,
                &underlying_symbol,
                buyer_collateral - total_premium,
            );
        }

        // Check position limits
        let user_exposure = get_user_exposure(&env, &buyer);
        if user_exposure + size > get_max_position_per_user(&env) {
            panic!("Position limit exceeded");
        }

        // Create option ID
        let option_id = get_option_counter(&env);

        // Create position for the buyer
        let position = OptionPosition {
            option_id,
            series_id,
            holder: buyer.clone(),
            writer: buyer.clone(), // Self-purchased for simplicity
            underlying: series.underlying,
            option_type: series.option_type,
            option_style: series.option_style,
            strike_price: series.strike_price,
            current_price: spot_price,
            expiration: series.expiration,
            premium_paid: total_premium,
            collateral_locked: 0,
            size,
            status: OptionStatus::Active,
            created_at: now,
            exercised_at: None,
            settled_at: None,
        };

        set_option_position(&env, &position);
        add_user_held_option_id(&env, &buyer, option_id);
        set_option_counter(&env, option_id + 1);

        // Update series open interest
        let mut series = series;
        series.total_open_interest += size;
        set_option_series(&env, &series);

        // Update exposure tracking
        set_total_exposure(&env, get_total_exposure(&env) + size);
        set_user_exposure(&env, &buyer, user_exposure + size);

        // Calculate and cache Greeks
        let greeks = calculate_all_greeks(
            spot_price,
            series.strike_price,
            tte,
            rate,
            volatility,
            series.option_type,
        );
        set_greeks_cache(&env, option_id, &greeks);

        env.events().publish(
            (symbol_short!("opt_buy"),),
            (
                option_id,
                series_id,
                buyer,
                size,
                premium_per_unit,
                total_premium,
                spot_price,
                now,
            ),
        );

        option_id
    }

    // ════════════════════════════════════════════════════════════════════════
    //  OPTION EXERCISE
    // ════════════════════════════════════════════════════════════════════════

    /// Exercise an option. American options can be exercised anytime; European only at expiry.
    pub fn exercise_option(env: Env, holder: Address, option_id: u64) {
        holder.require_auth();

        let position = get_option_position(&env, option_id, &holder).expect("Position not found");

        if position.status != OptionStatus::Active {
            panic!("Option not active");
        }

        if position.holder != holder {
            panic!("Not option holder");
        }

        let now = env.ledger().timestamp();

        // Check exercise timing based on option style
        match position.option_style {
            OptionStyle::American => {
                // Can exercise anytime before expiry
                if now >= position.expiration {
                    panic!("Option expired - use settle instead");
                }
            }
            OptionStyle::European => {
                // Can only exercise at expiry
                if now < position.expiration {
                    panic!("European option cannot be exercised early");
                }
            }
        }

        // Get current price
        let spot_price = oracle::get_oracle_price(&env, position.underlying);

        // Check if in-the-money
        if !is_in_the_money(spot_price, position.strike_price, position.option_type) {
            panic!("Option is out of the money");
        }

        // Calculate payoff
        let payoff = calculate_payoff(
            spot_price,
            position.strike_price,
            position.size,
            position.option_type,
        );

        // Transfer payoff (simplified - in production use token transfer)
        // The collateral locked is used for the payoff
        let underlying_symbol = oracle::underlying_to_symbol(&env, position.underlying);

        // Update position status
        let mut position = position;
        position.status = OptionStatus::Exercised;
        position.exercised_at = Some(now);
        set_option_position(&env, &position);

        // Release collateral
        let pool_coll = get_pool_collateral(&env, &underlying_symbol);
        set_pool_collateral(
            &env,
            &underlying_symbol,
            pool_coll - position.collateral_locked,
        );

        // Update exposure
        set_total_exposure(&env, get_total_exposure(&env) - position.size);
        let user_exp = get_user_exposure(&env, &holder);
        set_user_exposure(&env, &holder, user_exp - position.size);

        // Update series open interest
        if let Some(mut series) = get_option_series(&env, position.series_id) {
            series.total_open_interest = series.total_open_interest.saturating_sub(position.size);
            set_option_series(&env, &series);
        }

        env.events().publish(
            (symbol_short!("opt_exer"),),
            (
                option_id,
                holder,
                spot_price,
                position.strike_price,
                position.size,
                payoff,
                now,
            ),
        );
    }

    // ════════════════════════════════════════════════════════════════════════
    //  SETTLEMENT
    // ════════════════════════════════════════════════════════════════════════

    /// Settle expired options. Automatically exercises in-the-money European options.
    pub fn settle_option(env: Env, caller: Address, option_id: u64) {
        caller.require_auth();

        // Check if caller has this position
        let position = get_option_position(&env, option_id, &caller).expect("Position not found");

        if position.status != OptionStatus::Active {
            panic!("Option not active");
        }

        let now = env.ledger().timestamp();

        // Can only settle after expiration
        if now < position.expiration {
            panic!("Option has not expired yet");
        }

        // Get current price at expiry
        let spot_price = oracle::get_oracle_price(&env, position.underlying);

        // Check if in-the-money
        let itm = is_in_the_money(spot_price, position.strike_price, position.option_type);

        let payoff = if itm {
            calculate_payoff(
                spot_price,
                position.strike_price,
                position.size,
                position.option_type,
            )
        } else {
            0
        };

        // Update position
        let mut position = position;
        position.status = OptionStatus::Settled;
        position.settled_at = Some(now);
        set_option_position(&env, &position);

        // Release collateral
        let underlying_symbol = oracle::underlying_to_symbol(&env, position.underlying);
        let pool_coll = get_pool_collateral(&env, &underlying_symbol);
        set_pool_collateral(
            &env,
            &underlying_symbol,
            pool_coll - position.collateral_locked,
        );

        // Update exposure
        set_total_exposure(&env, get_total_exposure(&env) - position.size);
        let user_exp = get_user_exposure(&env, &position.holder);
        set_user_exposure(&env, &position.holder, user_exp - position.size);

        // Update series open interest
        if let Some(mut series) = get_option_series(&env, position.series_id) {
            series.total_open_interest = series.total_open_interest.saturating_sub(position.size);
            set_option_series(&env, &series);
        }

        env.events().publish(
            (symbol_short!("opt_settle"),),
            (
                option_id,
                position.holder,
                spot_price,
                position.strike_price,
                itm,
                payoff,
                position.size,
                now,
            ),
        );
    }

    /// Batch settle multiple expired options.
    pub fn batch_settle(env: Env, caller: Address, option_ids: soroban_sdk::Vec<u64>) {
        caller.require_auth();
        for i in 0..option_ids.len() {
            let id = option_ids.get(i).unwrap();
            let result = env.try_invoke_contract(
                &env.current_contract_address(),
                &Symbol::new(&env, "settle_option"),
                soroban_sdk::vec![&env, caller.clone().into_val(&env), id.into_val(&env)],
            );
            // Continue even if individual settlement fails (option may not be expired yet)
            let _ = result;
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    //  SECONDARY MARKET TRADING
    // ════════════════════════════════════════════════════════════════════════

    /// Create a listing to sell an option on the secondary market.
    pub fn create_listing(
        env: Env,
        seller: Address,
        option_id: u64,
        price: i128,
        size: i128,
    ) -> u64 {
        seller.require_auth();
        check_paused(&env);

        if price <= 0 || size <= 0 {
            panic!("Invalid price or size");
        }

        let position = get_option_position(&env, option_id, &seller).expect("Position not found");

        if position.status != OptionStatus::Active {
            panic!("Option not active");
        }
        if position.holder != seller {
            panic!("Not option holder");
        }
        if position.size < size {
            panic!("Insufficient balance");
        }

        let now = env.ledger().timestamp();
        let listing_id = get_listing_counter(&env);

        let listing = OptionListing {
            listing_id,
            option_id,
            seller: seller.clone(),
            price,
            size,
            is_active: true,
            created_at: now,
            expires_at: position.expiration,
        };

        set_listing(&env, &listing);
        set_listing_counter(&env, listing_id + 1);

        env.events().publish(
            (symbol_short!("list_create"),),
            (listing_id, option_id, seller, price, size, now),
        );

        listing_id
    }

    /// Cancel a listing.
    pub fn cancel_listing(env: Env, seller: Address, listing_id: u64) {
        seller.require_auth();

        let listing = get_listing(&env, listing_id).expect("Listing not found");
        if listing.seller != seller {
            panic!("Not listing seller");
        }

        let mut listing = listing;
        listing.is_active = false;
        set_listing(&env, &listing);

        env.events()
            .publish((symbol_short!("list_cancel"),), (listing_id, seller));
    }

    /// Buy from a secondary market listing.
    pub fn buy_from_listing(env: Env, buyer: Address, listing_id: u64) {
        buyer.require_auth();
        check_paused(&env);

        let listing = get_listing(&env, listing_id).expect("Listing not found");
        if !listing.is_active {
            panic!("Listing not active");
        }

        let now = env.ledger().timestamp();
        if listing.expires_at <= now {
            panic!("Listing expired");
        }

        if listing.seller == buyer {
            panic!("Cannot buy own listing");
        }

        let _total_cost = listing.price * listing.size / PRECISION;

        // Transfer tokens from buyer to seller (simplified)
        // In production, use token transfer contracts

        // Update the seller's position - reduce size
        let mut seller_pos = get_option_position(&env, listing.option_id, &listing.seller)
            .expect("Seller position not found");
        seller_pos.size -= listing.size;
        if seller_pos.size <= 0 {
            seller_pos.status = OptionStatus::Cancelled;
        }
        set_option_position(&env, &seller_pos);
        let seller_id = listing.seller.clone();

        // Create new position for buyer
        let option_id = get_option_counter(&env);

        let buyer_position = OptionPosition {
            option_id,
            series_id: seller_pos.series_id,
            holder: buyer.clone(),
            writer: seller_id,
            underlying: seller_pos.underlying,
            option_type: seller_pos.option_type,
            option_style: seller_pos.option_style,
            strike_price: seller_pos.strike_price,
            current_price: seller_pos.current_price,
            expiration: seller_pos.expiration,
            premium_paid: total_cost,
            collateral_locked: 0,
            size: listing.size,
            status: OptionStatus::Active,
            created_at: now,
            exercised_at: None,
            settled_at: None,
        };

        set_option_position(&env, &buyer_position);
        add_user_held_option_id(&env, &buyer, option_id);
        set_option_counter(&env, option_id + 1);

        // Deactivate listing
        let mut listing = listing;
        listing.is_active = false;
        set_listing(&env, &listing);

        env.events().publish(
            (symbol_short!("list_bought"),),
            (
                listing_id,
                option_id,
                listing.seller,
                buyer,
                listing.size,
                listing.price,
                now,
            ),
        );
    }

    /// Get a listing quote (view function).
    pub fn get_listing_quote(env: Env, listing_id: u64) -> (i128, i128) {
        let listing = get_listing(&env, listing_id).expect("Listing not found");
        let total_cost = listing.price * listing.size / PRECISION;
        (listing.price, total_cost)
    }

    // ════════════════════════════════════════════════════════════════════════
    //  PRICING & GREEKS
    // ════════════════════════════════════════════════════════════════════════

    /// Calculate the current premium for an option series (view function).
    pub fn calculate_premium(env: Env, series_id: u64, size: i128) -> (i128, i128) {
        let series = get_option_series(&env, series_id).expect("Series not found");
        let now = env.ledger().timestamp();
        let spot_price = oracle::get_oracle_price(&env, series.underlying);
        let volatility = oracle::get_volatility(&env, series.underlying);
        let tte = time_to_expiry_years(now, series.expiration);
        let rate = DEFAULT_RISK_FREE_RATE;

        let premium_per_unit = black_scholes_price(
            spot_price,
            series.strike_price,
            tte,
            rate,
            volatility,
            series.option_type,
        )
        .unwrap_or(0);

        let total_premium = premium_per_unit * size / PRECISION;
        (premium_per_unit, total_premium)
    }

    /// Get Greeks for a specific option (view function).
    pub fn get_greeks(env: Env, option_id: u64) -> Greeks {
        if let Some(cached) = get_greeks_cache(&env, option_id) {
            return cached;
        }

        // Default Greeks if not cached
        Greeks {
            delta: 0,
            gamma: 0,
            vega: 0,
            theta: 0,
            rho: 0,
        }
    }

    /// Calculate all Greeks for an option series.
    pub fn calculate_series_greeks(env: Env, series_id: u64) -> Greeks {
        let series = get_option_series(&env, series_id).expect("Series not found");
        let now = env.ledger().timestamp();
        let spot_price = oracle::get_oracle_price(&env, series.underlying);
        let volatility = oracle::get_volatility(&env, series.underlying);
        let tte = time_to_expiry_years(now, series.expiration);
        let rate = DEFAULT_RISK_FREE_RATE;

        calculate_all_greeks(
            spot_price,
            series.strike_price,
            tte,
            rate,
            volatility,
            series.option_type,
        )
    }

    // ════════════════════════════════════════════════════════════════════════
    //  COLLATERAL MANAGEMENT
    // ════════════════════════════════════════════════════════════════════════

    /// Deposit collateral for option writing.
    pub fn deposit_collateral(env: Env, user: Address, amount: i128) {
        user.require_auth();
        check_paused(&env);

        if amount <= 0 {
            panic!("Invalid amount");
        }

        let xlm = Symbol::new(&env, "XLM");
        let current = get_writer_collateral(&env, &user, &xlm);
        set_writer_collateral(&env, &user, &xlm, current + amount);

        env.events().publish(
            (symbol_short!("coll_dep"),),
            (user, amount, env.ledger().timestamp()),
        );
    }

    /// Request a multi-sig withdrawal of collateral.
    pub fn request_withdrawal(env: Env, requester: Address, amount: i128) -> u64 {
        requester.require_auth();

        if amount <= 0 {
            panic!("Invalid amount");
        }

        let xlm = Symbol::new(&env, "XLM");
        let current = get_writer_collateral(&env, &requester, &xlm);
        if current < amount {
            panic!("Insufficient collateral");
        }

        let request_id = get_withdrawal_request_counter(&env);

        let request = WithdrawalRequest {
            request_id,
            requester: requester.clone(),
            amount,
            underlying: UnderlyingAsset::XLM,
            approvals: 0,
            required_approvals: 2, // Require 2-of-3 multi-sig
            is_executed: false,
            created_at: env.ledger().timestamp(),
        };

        set_withdrawal_request(&env, &request);
        set_withdrawal_request_counter(&env, request_id + 1);

        env.events().publish(
            (symbol_short!("wd_req"),),
            (request_id, requester, amount, env.ledger().timestamp()),
        );

        request_id
    }

    /// Approve a withdrawal request (multi-sig).
    pub fn approve_withdrawal(env: Env, approver: Address, request_id: u64) {
        approver.require_auth();

        let mut request = get_withdrawal_request(&env, request_id).expect("Request not found");

        if request.is_executed {
            panic!("Request already executed");
        }

        if has_withdrawal_approval(&env, request_id, &approver) {
            panic!("Already approved");
        }

        set_withdrawal_approval(&env, request_id, &approver);
        request.approvals += 1;

        if request.approvals >= request.required_approvals {
            // Execute withdrawal
            request.is_executed = true;
            let xlm = Symbol::new(&env, "XLM");
            let current = get_writer_collateral(&env, &request.requester, &xlm);
            set_writer_collateral(&env, &request.requester, &xlm, current - request.amount);

            env.events().publish(
                (symbol_short!("wd_exec"),),
                (
                    request_id,
                    request.requester,
                    request.amount,
                    env.ledger().timestamp(),
                ),
            );
        }

        set_withdrawal_request(&env, &request);

        env.events().publish(
            (symbol_short!("wd_appr"),),
            (
                request_id,
                approver,
                request.approvals,
                env.ledger().timestamp(),
            ),
        );
    }

    // ════════════════════════════════════════════════════════════════════════
    //  ORACLE MANAGEMENT
    // ════════════════════════════════════════════════════════════════════════

    /// Update oracle price (admin or oracle only).
    pub fn update_price(env: Env, caller: Address, underlying: UnderlyingAsset, price: i128) {
        caller.require_auth();

        if price <= 0 {
            panic!("Invalid price");
        }

        // Verify caller is admin or oracle
        let is_admin = get_admin(&env).map(|a| a == caller).unwrap_or(false);
        let is_oracle = get_oracle(&env).map(|o| o == caller).unwrap_or(false);

        if !is_admin && !is_oracle {
            panic!("Unauthorized: admin or oracle required");
        }

        oracle::update_oracle_price(&env, underlying, price);

        env.events().publish(
            (symbol_short!("price_upd"),),
            (underlying as u32, price, env.ledger().timestamp()),
        );
    }

    /// Update volatility (admin only).
    pub fn update_volatility(env: Env, admin: Address, underlying: UnderlyingAsset, vol: i128) {
        admin.require_auth();
        assert_admin(&env, &admin);

        oracle::update_volatility(&env, underlying, vol);

        env.events().publish(
            (symbol_short!("vol_upd"),),
            (underlying as u32, vol, env.ledger().timestamp()),
        );
    }

    // ════════════════════════════════════════════════════════════════════════
    //  RISK MANAGEMENT
    // ════════════════════════════════════════════════════════════════════════

    /// Get portfolio risk snapshot for a user (view function).
    pub fn get_portfolio_risk(env: Env, user: Address) -> PortfolioRisk {
        let exposure = get_user_exposure(&env, &user);
        let xlm = Symbol::new(&env, "XLM");
        let collateral = get_writer_collateral(&env, &user, &xlm);

        // Calculate net Greeks from all positions
        let held_ids = get_user_held_option_ids(&env, &user);
        let mut net_delta: i128 = 0;
        let mut net_gamma: i128 = 0;
        let mut net_vega: i128 = 0;
        let mut net_theta: i128 = 0;
        let mut total_premium: i128 = 0;

        for i in 0..held_ids.len() {
            let id = held_ids.get(i).unwrap();
            if let Some(pos) = get_option_position(&env, id, &user) {
                if pos.status == OptionStatus::Active {
                    let greeks = get_greeks_cache(&env, id).unwrap_or(Greeks {
                        delta: 0,
                        gamma: 0,
                        vega: 0,
                        theta: 0,
                        rho: 0,
                    });
                    net_delta += greeks.delta * pos.size / PRECISION;
                    net_gamma += greeks.gamma * pos.size / PRECISION;
                    net_vega += greeks.vega * pos.size / PRECISION;
                    net_theta += greeks.theta * pos.size / PRECISION;
                    total_premium += pos.premium_paid;
                }
            }
        }

        let max_position = get_max_position_per_user(&env);

        PortfolioRisk {
            user: user.clone(),
            total_exposure: exposure,
            total_collateral_locked: collateral,
            total_premium_paid: total_premium,
            max_position_size: max_position,
            is_within_limits: exposure <= max_position,
            net_delta,
            net_gamma,
            net_vega,
            net_theta,
        }
    }

    /// Check circuit breaker status (view function).
    pub fn check_circuit_breaker(env: Env) -> bool {
        is_circuit_breaker_active(&env)
    }

    /// Manually reset circuit breaker (admin only).
    pub fn reset_circuit_breaker(env: Env, admin: Address) {
        admin.require_auth();
        assert_admin(&env, &admin);

        set_circuit_breaker(
            &env,
            &CircuitBreakerState {
                triggered: false,
                triggered_at: None,
                price_at_trigger: None,
                previous_price: None,
                change_bps: None,
                cooldown_seconds: 3600,
                can_resume_at: None,
            },
        );

        env.events()
            .publish((symbol_short!("cb_reset"),), (env.ledger().timestamp(),));
    }

    /// Set risk parameters (admin only).
    pub fn set_risk_parameters(env: Env, admin: Address, max_position: i128, max_exposure: i128) {
        admin.require_auth();
        assert_admin(&env, &admin);

        set_max_position_per_user(&env, max_position);
        set_max_total_exposure(&env, max_exposure);

        env.events().publish(
            (symbol_short!("risk_upd"),),
            (max_position, max_exposure, env.ledger().timestamp()),
        );
    }

    // ════════════════════════════════════════════════════════════════════════
    //  ADMIN FUNCTIONS
    // ════════════════════════════════════════════════════════════════════════

    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        assert_admin(&env, &admin);
        set_paused(&env, true);
        env.events()
            .publish((symbol_short!("pause"),), (env.ledger().timestamp(),));
    }

    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        assert_admin(&env, &admin);
        set_paused(&env, false);
        env.events()
            .publish((symbol_short!("unpause"),), (env.ledger().timestamp(),));
    }

    pub fn set_admin_fn(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        assert_admin(&env, &current_admin);
        set_admin(&env, &new_admin);
        env.events().publish(
            (symbol_short!("adm_set"),),
            (current_admin, new_admin, env.ledger().timestamp()),
        );
    }

    // ════════════════════════════════════════════════════════════════════════
    //  VIEW FUNCTIONS
    // ════════════════════════════════════════════════════════════════════════

    pub fn get_option(env: Env, option_id: u64, holder: Address) -> Option<OptionPosition> {
        get_option_position(&env, option_id, &holder)
    }

    pub fn get_series(env: Env, series_id: u64) -> Option<OptionSeriesConfig> {
        get_option_series(&env, series_id)
    }

    pub fn get_user_written_options(env: Env, user: Address) -> soroban_sdk::Vec<u64> {
        get_user_option_ids(&env, &user)
    }

    pub fn get_user_held_options(env: Env, user: Address) -> soroban_sdk::Vec<u64> {
        get_user_held_option_ids(&env, &user)
    }

    pub fn get_writer_balance(env: Env, user: Address) -> i128 {
        let xlm = Symbol::new(&env, "XLM");
        get_writer_collateral(&env, &user, &xlm)
    }

    pub fn get_pool_balance(env: Env) -> (i128, i128) {
        let xlm = Symbol::new(&env, "XLM");
        let usdc = Symbol::new(&env, "USDC");
        (
            get_pool_collateral(&env, &xlm),
            get_pool_collateral(&env, &usdc),
        )
    }

    pub fn get_current_price(env: Env, underlying: UnderlyingAsset) -> i128 {
        oracle::get_oracle_price(&env, underlying)
    }

    pub fn get_current_volatility(env: Env, underlying: UnderlyingAsset) -> i128 {
        oracle::get_volatility(&env, underlying)
    }

    pub fn get_total_protocol_exposure(env: Env) -> i128 {
        get_total_exposure(&env)
    }

    pub fn get_listing(env: Env, listing_id: u64) -> Option<OptionListing> {
        get_listing(&env, listing_id)
    }
}

// ══════════════════════════════════════════════════════════════════════════════
//  HELPER FUNCTIONS
// ══════════════════════════════════════════════════════════════════════════════

/// Calculate all Greeks for an option.
fn calculate_all_greeks(
    spot: i128,
    strike: i128,
    time_to_expiry: i128,
    risk_free_rate: i128,
    volatility: i128,
    option_type: OptionType,
) -> Greeks {
    Greeks {
        delta: math::calculate_delta(
            spot,
            strike,
            time_to_expiry,
            risk_free_rate,
            volatility,
            option_type,
        ),
        gamma: math::calculate_gamma(spot, strike, time_to_expiry, risk_free_rate, volatility),
        vega: math::calculate_vega(spot, strike, time_to_expiry, risk_free_rate, volatility),
        theta: math::calculate_theta(
            spot,
            strike,
            time_to_expiry,
            risk_free_rate,
            volatility,
            option_type,
        ),
        rho: math::calculate_rho(
            spot,
            strike,
            time_to_expiry,
            risk_free_rate,
            volatility,
            option_type,
        ),
    }
}
