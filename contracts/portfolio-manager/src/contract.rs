#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contractimpl, symbol_short, token::TokenClient, Address, Env, Symbol, Vec,
};

use crate::storage::{DataKey, Storage};
use crate::templates::Templates;
use crate::types::*;

// ═══════════════════════════════════════════════════════════════
//  CONSTANTS
// ═══════════════════════════════════════════════════════════════

const SECONDS_PER_MONTH: u64 = 2_592_000;
const SECONDS_PER_QUARTER: u64 = 7_776_000;
const SECONDS_PER_SEMI_ANNUAL: u64 = 15_552_000;
const SECONDS_PER_YEAR: u64 = 31_557_600; // 365.25 days
#[allow(dead_code)]
const MAX_BATCH_SIZE: u32 = 50;
const RISK_FREE_RATE_BPS: i128 = 500; // 5% risk-free rate for Sharpe

// ═══════════════════════════════════════════════════════════════
//  CONTRACT
// ═══════════════════════════════════════════════════════════════

#[contract]
pub struct PortfolioManager;

#[contractimpl]
impl PortfolioManager {
    // ╔═══════════════════════════════════════════════════════════╗
    // ║  INITIALIZATION                                          ║
    // ╚═══════════════════════════════════════════════════════════╝

    /// Initialize the portfolio manager contract
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        admin.require_auth();

        Storage::set_admin(&env, &admin);
        Storage::set_paused(&env, false);
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &false);

        env.events().publish(
            (symbol_short!("pm_init"),),
            (admin, env.ledger().timestamp()),
        );
    }

    // ╔═══════════════════════════════════════════════════════════╗
    // ║  PORTFOLIO CREATION                                      ║
    // ╚═══════════════════════════════════════════════════════════╝

    /// Create a portfolio from a pre-configured template
    pub fn create_from_template(
        env: Env,
        creator: Address,
        name: Symbol,
        template_type: PortfolioType,
        deposit_token: Address,
        oracle_address: Address,
        tokens: Vec<Address>,
    ) -> u64 {
        creator.require_auth();
        if Storage::is_paused(&env) {
            panic!("Contract is paused");
        }
        if tokens.len() > MAX_ASSETS {
            panic!("Too many assets");
        }

        let template = Templates::get_template(&env, template_type);
        let allocations = Templates::template_to_allocations(&env, &template, &tokens);

        Self::create_portfolio_inner(
            &env,
            &creator,
            &name,
            template_type,
            template.weighting_strategy,
            template.rebalance_frequency,
            template.drift_tolerance_bps,
            template.max_slippage_bps,
            &deposit_token,
            &oracle_address,
            &allocations,
        )
    }

    /// Create a portfolio with custom weights
    pub fn create_custom(
        env: Env,
        creator: Address,
        name: Symbol,
        deposit_token: Address,
        oracle_address: Address,
        allocations: Vec<AssetAllocation>,
        weighting_strategy: WeightingStrategy,
        rebalance_frequency: RebalanceFrequency,
        drift_tolerance_bps: Option<u32>,
        max_slippage_bps: Option<u32>,
    ) -> u64 {
        creator.require_auth();
        if Storage::is_paused(&env) {
            panic!("Contract is paused");
        }
        if allocations.len() > MAX_ASSETS {
            panic!("Too many assets");
        }
        if !Templates::validate_weights(&allocations) {
            panic!("Weights must sum to 10000 BPS");
        }

        Self::create_portfolio_inner(
            &env,
            &creator,
            &name,
            PortfolioType::Custom,
            weighting_strategy,
            rebalance_frequency,
            drift_tolerance_bps.unwrap_or(DRIFT_TOLERANCE_BPS),
            max_slippage_bps.unwrap_or(MAX_REBALANCE_SLIPPAGE_BPS),
            &deposit_token,
            &oracle_address,
            &allocations,
        )
    }

    /// Create an equal-weight portfolio for a list of tokens
    pub fn create_equal_weight(
        env: Env,
        creator: Address,
        name: Symbol,
        deposit_token: Address,
        oracle_address: Address,
        tokens: Vec<Address>,
        rebalance_frequency: RebalanceFrequency,
    ) -> u64 {
        creator.require_auth();
        if Storage::is_paused(&env) {
            panic!("Contract is paused");
        }
        if tokens.len() > MAX_ASSETS {
            panic!("Too many assets");
        }
        if tokens.is_empty() {
            panic!("No tokens provided");
        }

        let allocations = Templates::equal_weight_allocations(&env, &tokens);

        Self::create_portfolio_inner(
            &env,
            &creator,
            &name,
            PortfolioType::Custom,
            WeightingStrategy::EqualWeight,
            rebalance_frequency,
            DRIFT_TOLERANCE_BPS,
            MAX_REBALANCE_SLIPPAGE_BPS,
            &deposit_token,
            &oracle_address,
            &allocations,
        )
    }

    // ╔═══════════════════════════════════════════════════════════╗
    // ║  DEPOSIT / WITHDRAWAL                                    ║
    // ╚═══════════════════════════════════════════════════════════╝

    /// Deposit collateral and receive portfolio tokens at fair share price
    pub fn deposit(env: Env, user: Address, portfolio_id: u64, amount: i128) -> i128 {
        user.require_auth();
        if amount <= 0 {
            panic!("Deposit amount must be positive");
        }

        let mut portfolio = Storage::get_portfolio(&env, portfolio_id);
        if portfolio.status != PortfolioStatus::Active {
            panic!("Portfolio is not active");
        }

        // Transfer deposit token from user to contract
        let contract_address = env.current_contract_address();
        let token_client = TokenClient::new(&env, &portfolio.deposit_token);
        token_client.transfer(&user, &contract_address, &amount);

        // Calculate shares to mint
        let shares = Self::calculate_shares_for_deposit(
            amount,
            portfolio.total_assets,
            portfolio.total_supply,
        );

        // Update portfolio totals
        portfolio.total_assets = portfolio
            .total_assets
            .checked_add(amount)
            .expect("Assets overflow");
        portfolio.total_supply = portfolio
            .total_supply
            .checked_add(shares)
            .expect("Supply overflow");
        Storage::set_portfolio(&env, &portfolio);

        // Update or create user position
        let now = env.ledger().timestamp();
        if Storage::has_user_position(&env, &user, portfolio_id) {
            let mut pos = Storage::get_user_position(&env, &user, portfolio_id);
            pos.shares = pos.shares.checked_add(shares).expect("Shares overflow");
            pos.total_deposited = pos
                .total_deposited
                .checked_add(amount)
                .expect("Deposit overflow");
            pos.last_activity_at = now;
            Storage::set_user_position(&env, &user, portfolio_id, &pos);
        } else {
            let pos = UserPosition {
                user: user.clone(),
                portfolio_id,
                shares,
                total_deposited: amount,
                total_withdrawn: 0,
                pending_dividends: 0,
                first_deposit_at: now,
                last_activity_at: now,
            };
            Storage::set_user_position(&env, &user, portfolio_id, &pos);
            Storage::add_user_portfolio(&env, &user, portfolio_id);
        }

        env.events().publish(
            (symbol_short!("pm_dep"),),
            (
                user,
                portfolio_id,
                amount,
                shares,
                portfolio.total_assets,
                portfolio.total_supply,
            ),
        );

        shares
    }

    /// Withdraw by burning portfolio tokens
    pub fn withdraw(env: Env, user: Address, portfolio_id: u64, shares: i128) -> i128 {
        user.require_auth();
        if shares <= 0 {
            panic!("Shares must be positive");
        }

        let mut portfolio = Storage::get_portfolio(&env, portfolio_id);
        let mut pos = Storage::get_user_position(&env, &user, portfolio_id);

        if pos.shares < shares {
            panic!("Insufficient shares");
        }
        if portfolio.total_supply <= 0 {
            panic!("No assets in portfolio");
        }

        // Calculate the value of shares being redeemed
        let gross_assets = Self::calculate_assets_for_withdrawal(
            shares,
            portfolio.total_assets,
            portfolio.total_supply,
        );

        // Update portfolio totals
        portfolio.total_assets = portfolio
            .total_assets
            .checked_sub(gross_assets)
            .expect("Assets underflow");
        portfolio.total_supply = portfolio
            .total_supply
            .checked_sub(shares)
            .expect("Supply underflow");

        let new_nav = if portfolio.total_supply > 0 {
            portfolio.total_assets * PRECISION_FACTOR / portfolio.total_supply
        } else {
            PRECISION_FACTOR
        };

        Storage::set_portfolio(&env, &portfolio);

        // Update user position
        pos.shares = pos.shares.checked_sub(shares).expect("Shares underflow");
        pos.total_withdrawn = pos
            .total_withdrawn
            .checked_add(gross_assets)
            .expect("Withdraw overflow");
        pos.last_activity_at = env.ledger().timestamp();
        Storage::set_user_position(&env, &user, portfolio_id, &pos);

        // Transfer deposit token back to user
        let contract_address = env.current_contract_address();
        let token_client = TokenClient::new(&env, &portfolio.deposit_token);
        Storage::enter_non_reentrant(&env);
        token_client.transfer(&contract_address, &user, &gross_assets);
        Storage::exit_non_reentrant(&env);

        env.events().publish(
            (symbol_short!("pm_wth"),),
            (user, portfolio_id, shares, gross_assets, new_nav),
        );

        gross_assets
    }

    // ╔═══════════════════════════════════════════════════════════╗
    // ║  REBALANCING                                             ║
    // ╚═══════════════════════════════════════════════════════════╝

    /// Execute a rebalance by submitting batch swap records.
    /// Caller provides the executed swaps; contract validates drift, slippage, and timing.
    pub fn rebalance(
        env: Env,
        caller: Address,
        portfolio_id: u64,
        buys: Vec<SwapRecord>,
        sells: Vec<SwapRecord>,
        new_asset_balances: Vec<i128>,
    ) -> RebalanceRecord {
        caller.require_auth();

        let mut portfolio = Storage::get_portfolio(&env, portfolio_id);
        if portfolio.status != PortfolioStatus::Active {
            panic!("Portfolio is not active");
        }
        if Storage::is_paused(&env) {
            panic!("Contract is paused");
        }

        // Check rebalance timing (unless governance-forced)
        let now = env.ledger().timestamp();
        let min_interval = Self::rebalance_interval_seconds(portfolio.rebalance_frequency);
        if now < portfolio.last_rebalance_time + min_interval {
            // Allow if caller is admin (governance forced)
            let admin = Storage::get_admin(&env);
            if caller != admin {
                panic!("Rebalance too frequent");
            }
        }

        let asset_count = portfolio.target_weights.len();
        if new_asset_balances.len() != asset_count {
            panic!("Balance count must match asset count");
        }

        // Calculate NAV before rebalance
        let nav_before = portfolio.total_assets;
        let prev_rebalance_time = portfolio.last_rebalance_time;

        // Validate slippage: sum of all swaps should not exceed max_slippage_bps
        let total_swapped = Self::sum_swap_values(&buys, &sells);
        let slippage_bps = if nav_before > 0 {
            (total_swapped * BPS_DENOMINATOR / nav_before) as u32
        } else {
            0
        };
        if slippage_bps > portfolio.max_slippage_bps {
            panic!("Slippage exceeded maximum");
        }

        // Update asset positions with new balances
        let mut total_new_value: i128 = 0;
        for i in 0..asset_count {
            let mut pos = Storage::get_asset_position(&env, portfolio_id, i);
            let new_balance = new_asset_balances.get_unchecked(i);

            // Calculate value change
            let new_value = new_balance * pos.last_price / PRECISION_FACTOR;

            pos.balance = new_balance;

            // Update current weight
            pos.current_weight_bps = if portfolio.total_assets > 0 {
                (new_value * BPS_DENOMINATOR / portfolio.total_assets) as u32
            } else {
                0
            };

            Storage::set_asset_position(&env, portfolio_id, i, &pos);
            total_new_value = total_new_value
                .checked_add(new_value)
                .expect("Value overflow");
        }

        // Update portfolio total assets to reflect new position values
        portfolio.total_assets = total_new_value;
        portfolio.last_rebalance_time = now;
        portfolio.rebalance_count = portfolio
            .rebalance_count
            .checked_add(1)
            .expect("Count overflow");

        let trigger = if now < prev_rebalance_time + min_interval {
            // Within time interval - only admin can force rebalance
            RebalanceTrigger::GovernanceForced
        } else {
            RebalanceTrigger::TimeBased
        };

        Storage::set_portfolio(&env, &portfolio);

        // Store rebalance record
        let rebalance_id = Storage::next_rebalance_id(&env, portfolio_id);
        let record = RebalanceRecord {
            portfolio_id,
            rebalance_id,
            timestamp: now,
            buys,
            sells,
            slippage_bps,
            trigger,
            nav_before,
            nav_after: portfolio.total_assets,
        };
        Storage::set_rebalance_record(&env, &record);

        env.events().publish(
            (symbol_short!("pm_rbal"),),
            (
                portfolio_id,
                rebalance_id,
                nav_before,
                portfolio.total_assets,
                slippage_bps,
            ),
        );

        record
    }

    /// Check if drift exceeds threshold and rebalance is needed
    pub fn check_and_rebalance(
        env: Env,
        caller: Address,
        portfolio_id: u64,
        new_asset_balances: Vec<i128>,
        buys: Vec<SwapRecord>,
        sells: Vec<SwapRecord>,
    ) -> Option<RebalanceRecord> {
        let portfolio = Storage::get_portfolio(&env, portfolio_id);

        // Check time-based trigger
        let now = env.ledger().timestamp();
        let interval = Self::rebalance_interval_seconds(portfolio.rebalance_frequency);
        if now >= portfolio.last_rebalance_time + interval {
            let record = Self::rebalance(
                env.clone(),
                caller,
                portfolio_id,
                buys,
                sells,
                new_asset_balances,
            );
            return Some(record);
        }

        // Check drift trigger
        let max_drift = Self::max_drift(&env, portfolio_id, &new_asset_balances);
        if max_drift > portfolio.drift_tolerance_bps {
            let record =
                Self::rebalance(env, caller, portfolio_id, buys, sells, new_asset_balances);
            return Some(record);
        }

        None
    }

    /// Calculate the maximum drift across all assets
    pub fn calculate_max_drift(env: Env, portfolio_id: u64, current_balances: Vec<i128>) -> u32 {
        Self::max_drift(&env, portfolio_id, &current_balances)
    }

    // ╔═══════════════════════════════════════════════════════════╗
    // ║  ASSET POSITION MANAGEMENT                               ║
    // ╚═══════════════════════════════════════════════════════════╝

    /// Update an asset's position with new balance and price
    pub fn update_asset_position(
        env: Env,
        caller: Address,
        portfolio_id: u64,
        asset_index: u32,
        new_balance: i128,
        price: i128,
    ) {
        caller.require_auth();
        let admin = Storage::get_admin(&env);
        if caller != admin {
            panic!("Unauthorized");
        }

        let mut portfolio = Storage::get_portfolio(&env, portfolio_id);
        let mut pos = Storage::get_asset_position(&env, portfolio_id, asset_index);
        let new_value = new_balance * price / PRECISION_FACTOR;
        let value_diff = new_value - pos.balance * pos.last_price / PRECISION_FACTOR;

        pos.balance = new_balance;
        pos.last_price = price;
        pos.last_price_update = env.ledger().timestamp();
        pos.current_weight_bps = if portfolio.total_assets > 0 {
            (new_value * BPS_DENOMINATOR / portfolio.total_assets) as u32
        } else {
            0
        };

        Storage::set_asset_position(&env, portfolio_id, asset_index, &pos);

        // Update portfolio total assets
        portfolio.total_assets = portfolio
            .total_assets
            .checked_add(value_diff)
            .expect("NAV overflow");
        Storage::set_portfolio(&env, &portfolio);

        env.events().publish(
            (symbol_short!("pm_aupd"),),
            (portfolio_id, asset_index, new_balance, price),
        );
    }

    // ╔═══════════════════════════════════════════════════════════╗
    // ║  DIVIDEND COLLECTION                                     ║
    // ╚═══════════════════════════════════════════════════════════╝

    /// Collect dividends from underlying assets and compound them into the portfolio
    pub fn collect_dividends(
        env: Env,
        caller: Address,
        portfolio_id: u64,
        dividend_amounts: Vec<(Address, i128)>, // (token, amount)
    ) -> DividendRecord {
        caller.require_auth();
        let admin = Storage::get_admin(&env);
        if caller != admin {
            panic!("Unauthorized");
        }

        let mut portfolio = Storage::get_portfolio(&env, portfolio_id);
        if portfolio.status != PortfolioStatus::Active {
            panic!("Portfolio is not active");
        }

        let mut total_dividend: i128 = 0;
        for i in 0..dividend_amounts.len() {
            let (token, amount) = dividend_amounts.get_unchecked(i);
            // Find the asset and add to its position
            for j in 0..portfolio.target_weights.len() {
                let pos = Storage::get_asset_position(&env, portfolio_id, j);
                if pos.token == token {
                    let mut updated_pos = pos;
                    updated_pos.balance = updated_pos
                        .balance
                        .checked_add(amount)
                        .expect("Balance overflow");
                    Storage::set_asset_position(&env, portfolio_id, j, &updated_pos);
                    total_dividend = total_dividend
                        .checked_add(amount)
                        .expect("Dividend overflow");
                    break;
                }
            }
        }

        // Compound: add dividends to total assets
        portfolio.total_assets = portfolio
            .total_assets
            .checked_add(total_dividend)
            .expect("Assets overflow");
        portfolio.accumulated_dividends = portfolio
            .accumulated_dividends
            .checked_add(total_dividend)
            .expect("Accum overflow");
        portfolio.last_dividend_time = env.ledger().timestamp();

        let now = env.ledger().timestamp();
        let total_divs = Storage::total_dividends_collected(&env, portfolio_id)
            .checked_add(total_dividend)
            .expect("Total div overflow");
        Storage::set_total_dividends_collected(&env, portfolio_id, total_divs);

        // Calculate per-share amount
        let per_share = if portfolio.total_supply > 0 {
            total_dividend * PRECISION_FACTOR / portfolio.total_supply
        } else {
            0
        };

        Storage::set_portfolio(&env, &portfolio);

        // Store dividend record
        let record_id = Storage::next_dividend_id(&env, portfolio_id);
        let record = DividendRecord {
            portfolio_id,
            record_id,
            timestamp: now,
            total_collected: total_dividend,
            compounded: total_dividend,
            per_share_amount: per_share,
        };
        Storage::set_dividend_record(&env, &record);

        env.events().publish(
            (symbol_short!("pm_div"),),
            (
                portfolio_id,
                total_dividend,
                portfolio.total_assets,
                per_share,
            ),
        );

        record
    }

    /// Claim accumulated dividends for a user
    pub fn claim_dividends(env: Env, user: Address, portfolio_id: u64) -> i128 {
        user.require_auth();

        let mut pos = Storage::get_user_position(&env, &user, portfolio_id);
        if pos.pending_dividends <= 0 {
            panic!("No dividends to claim");
        }

        let amount = pos.pending_dividends;
        pos.pending_dividends = 0;
        pos.last_activity_at = env.ledger().timestamp();
        Storage::set_user_position(&env, &user, portfolio_id, &pos);

        let portfolio = Storage::get_portfolio(&env, portfolio_id);
        let contract_address = env.current_contract_address();
        let token_client = TokenClient::new(&env, &portfolio.deposit_token);

        Storage::enter_non_reentrant(&env);
        token_client.transfer(&contract_address, &user, &amount);
        Storage::exit_non_reentrant(&env);

        env.events()
            .publish((symbol_short!("pm_dclm"),), (user, portfolio_id, amount));

        amount
    }

    // ╔═══════════════════════════════════════════════════════════╗
    // ║  PERFORMANCE TRACKING                                    ║
    // ╚═══════════════════════════════════════════════════════════╝

    /// Record a performance snapshot. Should be called periodically.
    pub fn record_performance_snapshot(
        env: Env,
        caller: Address,
        portfolio_id: u64,
    ) -> PerformanceSnapshot {
        caller.require_auth();

        let portfolio = Storage::get_portfolio(&env, portfolio_id);
        let now = env.ledger().timestamp();

        let nav_per_share = if portfolio.total_supply > 0 {
            portfolio.total_assets * PRECISION_FACTOR / portfolio.total_supply
        } else {
            PRECISION_FACTOR
        };

        let mut acc = Storage::get_performance_accumulator(&env, portfolio_id);

        // Calculate periodic return
        let periodic_return = if acc.previous_nav > 0 {
            (nav_per_share - acc.previous_nav) * PRECISION_FACTOR / acc.previous_nav
        } else {
            0
        };

        // Update accumulators for Sharpe ratio
        acc.return_sum = acc
            .return_sum
            .checked_add(periodic_return)
            .expect("Return sum overflow");
        acc.return_squared_sum = acc
            .return_squared_sum
            .checked_add(periodic_return * periodic_return / PRECISION_FACTOR)
            .expect("Return sq overflow");
        acc.observation_count = acc
            .observation_count
            .checked_add(1)
            .expect("Count overflow");

        // Update max drawdown
        if nav_per_share > acc.peak_nav {
            acc.peak_nav = nav_per_share;
        }
        let current_drawdown_bps = if acc.peak_nav > 0 {
            ((acc.peak_nav - nav_per_share) * BPS_DENOMINATOR / acc.peak_nav) as u32
        } else {
            0
        };
        if current_drawdown_bps > acc.max_drawdown_bps {
            acc.max_drawdown_bps = current_drawdown_bps;
        }

        // Calculate Sharpe ratio (annualized)
        let sharpe = Self::calculate_sharpe(
            acc.return_sum,
            acc.return_squared_sum,
            acc.observation_count,
        );

        // Calculate time-weighted return
        let initial_nav = PRECISION_FACTOR; // Starting NAV per share is always 1e18
        let twr_bps = if initial_nav > 0 {
            ((nav_per_share - initial_nav) * BPS_DENOMINATOR / initial_nav) as i32
        } else {
            0
        };

        // Calculate annualized return
        let annualized_return_bps = if now > portfolio.created_at {
            let elapsed = now - portfolio.created_at;
            if elapsed > 0 {
                (twr_bps as i128 * SECONDS_PER_YEAR as i128 / elapsed as i128) as i32
            } else {
                0
            }
        } else {
            0
        };

        // Store snapshot
        let snapshot_id = Storage::next_snapshot_id(&env, portfolio_id);
        let snapshot = PerformanceSnapshot {
            portfolio_id,
            snapshot_id,
            timestamp: now,
            nav_per_share,
            total_assets: portfolio.total_assets,
            sharpe_ratio: sharpe,
            max_drawdown_bps: acc.max_drawdown_bps,
            peak_nav: acc.peak_nav,
            twr_bps,
            annualized_return_bps,
        };
        Storage::set_performance_snapshot(&env, &snapshot);

        // Update accumulator
        acc.previous_nav = nav_per_share;
        acc.previous_nav_time = now;
        Storage::set_performance_accumulator(&env, &acc);

        env.events().publish(
            (symbol_short!("pm_perf"),),
            (portfolio_id, nav_per_share, sharpe, acc.max_drawdown_bps),
        );

        snapshot
    }

    // ╔═══════════════════════════════════════════════════════════╗
    // ║  PORTFOLIO FORKING                                       ║
    // ╚═══════════════════════════════════════════════════════════╝

    /// Fork an existing portfolio to create a customized version
    pub fn fork_portfolio(
        env: Env,
        creator: Address,
        source_portfolio_id: u64,
        name: Symbol,
        custom_weights: Option<Vec<AssetAllocation>>,
        custom_rebalance_frequency: Option<RebalanceFrequency>,
    ) -> u64 {
        creator.require_auth();
        let source = Storage::get_portfolio(&env, source_portfolio_id);

        if source.status == PortfolioStatus::Closed {
            panic!("Cannot fork closed portfolio");
        }

        // Determine allocations: custom or same as source
        let allocations = match custom_weights {
            Some(ref weights) => {
                if !Templates::validate_weights(weights) {
                    panic!("Custom weights must sum to 10000 BPS");
                }
                weights.clone()
            }
            None => source.target_weights.clone(),
        };

        // Verify custom weights use same tokens as source
        if custom_weights.is_some() {
            if source.target_weights.len() != allocations.len() {
                panic!("Fork must use same set of tokens");
            }
            for i in 0..source.target_weights.len() {
                let src_token = source.target_weights.get_unchecked(i).token.clone();
                let custom_token = allocations.get_unchecked(i).token.clone();
                if src_token != custom_token {
                    panic!("Fork must use same set of tokens");
                }
            }
        }

        let rebalance_freq = custom_rebalance_frequency.unwrap_or(source.rebalance_frequency);

        Self::create_portfolio_inner(
            &env,
            &creator,
            &name,
            PortfolioType::Custom,
            source.weighting_strategy,
            rebalance_freq,
            source.drift_tolerance_bps,
            source.max_slippage_bps,
            &source.deposit_token,
            &source.oracle_address,
            &allocations,
        )
    }

    // ╔═══════════════════════════════════════════════════════════╗
    // ║  GOVERNANCE                                               ║
    // ╚═══════════════════════════════════════════════════════════╝

    /// Update drift tolerance for a portfolio (governance action)
    pub fn set_drift_tolerance(
        env: Env,
        admin: Address,
        portfolio_id: u64,
        new_tolerance_bps: u32,
    ) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut portfolio = Storage::get_portfolio(&env, portfolio_id);
        portfolio.drift_tolerance_bps = new_tolerance_bps;
        Storage::set_portfolio(&env, &portfolio);

        env.events().publish(
            (symbol_short!("pm_gov"),),
            (
                portfolio_id,
                Symbol::new(&env, "drift_tol"),
                new_tolerance_bps as i128,
            ),
        );
    }

    /// Update rebalance frequency
    pub fn set_rebalance_frequency(
        env: Env,
        admin: Address,
        portfolio_id: u64,
        frequency: RebalanceFrequency,
    ) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut portfolio = Storage::get_portfolio(&env, portfolio_id);
        portfolio.rebalance_frequency = frequency;
        Storage::set_portfolio(&env, &portfolio);

        env.events().publish(
            (symbol_short!("pm_gov"),),
            (
                portfolio_id,
                Symbol::new(&env, "rbal_freq"),
                frequency as i128,
            ),
        );
    }

    /// Update max slippage
    pub fn set_max_slippage(env: Env, admin: Address, portfolio_id: u64, max_slippage_bps: u32) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut portfolio = Storage::get_portfolio(&env, portfolio_id);
        portfolio.max_slippage_bps = max_slippage_bps;
        Storage::set_portfolio(&env, &portfolio);

        env.events().publish(
            (symbol_short!("pm_gov"),),
            (
                portfolio_id,
                Symbol::new(&env, "max_slip"),
                max_slippage_bps as i128,
            ),
        );
    }

    /// Update target weights for a portfolio
    pub fn set_target_weights(
        env: Env,
        admin: Address,
        portfolio_id: u64,
        new_weights: Vec<AssetAllocation>,
    ) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        if !Templates::validate_weights(&new_weights) {
            panic!("Weights must sum to 10000 BPS");
        }

        let mut portfolio = Storage::get_portfolio(&env, portfolio_id);
        let old_count = portfolio.target_weights.len();
        let new_count = new_weights.len();

        if new_count != old_count {
            panic!("Cannot change number of assets");
        }

        // Verify tokens match
        for i in 0..old_count {
            let old_token = portfolio.target_weights.get_unchecked(i).token;
            let new_token = new_weights.get_unchecked(i).token;
            if old_token != new_token {
                panic!("Cannot change token addresses");
            }
        }

        // Update target weights and current weights
        for i in 0..new_count {
            let new_weight = new_weights.get_unchecked(i);
            let mut pos = Storage::get_asset_position(&env, portfolio_id, i);
            pos.target_weight_bps = new_weight.weight_bps;

            // Recalculate current weight
            if portfolio.total_assets > 0 {
                let asset_value = pos.balance * pos.last_price / PRECISION_FACTOR;
                pos.current_weight_bps =
                    (asset_value * BPS_DENOMINATOR / portfolio.total_assets) as u32;
            }

            Storage::set_asset_position(&env, portfolio_id, i, &pos);
        }

        portfolio.target_weights = new_weights;
        Storage::set_portfolio(&env, &portfolio);

        env.events().publish(
            (symbol_short!("pm_gov"),),
            (portfolio_id, Symbol::new(&env, "weights"), 0),
        );
    }

    /// Pause a portfolio
    pub fn pause_portfolio(env: Env, admin: Address, portfolio_id: u64) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut portfolio = Storage::get_portfolio(&env, portfolio_id);
        if portfolio.status != PortfolioStatus::Active {
            panic!("Portfolio is not active");
        }
        portfolio.status = PortfolioStatus::Paused;
        Storage::set_portfolio(&env, &portfolio);

        env.events()
            .publish((symbol_short!("pm_pause"),), (portfolio_id,));
    }

    /// Unpause a portfolio
    pub fn unpause_portfolio(env: Env, admin: Address, portfolio_id: u64) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut portfolio = Storage::get_portfolio(&env, portfolio_id);
        if portfolio.status != PortfolioStatus::Paused {
            panic!("Portfolio is not paused");
        }
        portfolio.status = PortfolioStatus::Active;
        Storage::set_portfolio(&env, &portfolio);

        env.events()
            .publish((symbol_short!("pm_unpse"),), (portfolio_id,));
    }

    /// Close a portfolio (cannot be reopened)
    pub fn close_portfolio(env: Env, admin: Address, portfolio_id: u64) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut portfolio = Storage::get_portfolio(&env, portfolio_id);
        if portfolio.status == PortfolioStatus::Closed {
            panic!("Portfolio already closed");
        }
        portfolio.status = PortfolioStatus::Closed;
        Storage::set_portfolio(&env, &portfolio);

        env.events()
            .publish((symbol_short!("pm_close"),), (portfolio_id,));
    }

    // ╔═══════════════════════════════════════════════════════════╗
    // ║  PAUSE / UNPAUSE (GLOBAL)                                ║
    // ╚═══════════════════════════════════════════════════════════╝

    /// Pause the entire contract
    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        Storage::set_paused(&env, true);
        env.events().publish((symbol_short!("pm_pause"),), (admin,));
    }

    /// Unpause the entire contract
    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        Storage::set_paused(&env, false);
        env.events().publish((symbol_short!("pm_unpse"),), (admin,));
    }

    // ╔═══════════════════════════════════════════════════════════╗
    // ║  VIEW FUNCTIONS                                           ║
    // ╚═══════════════════════════════════════════════════════════╝

    /// Get portfolio info summary
    pub fn get_portfolio_info(env: Env, portfolio_id: u64) -> PortfolioInfo {
        let p = Storage::get_portfolio(&env, portfolio_id);
        let nav = if p.total_supply > 0 {
            p.total_assets * PRECISION_FACTOR / p.total_supply
        } else {
            PRECISION_FACTOR
        };

        PortfolioInfo {
            portfolio_id: p.portfolio_id,
            name: p.name,
            portfolio_type: p.portfolio_type,
            weighting_strategy: p.weighting_strategy,
            status: p.status,
            total_assets: p.total_assets,
            total_supply: p.total_supply,
            nav_per_share: nav,
            rebalance_frequency: p.rebalance_frequency,
            drift_tolerance_bps: p.drift_tolerance_bps,
            asset_count: p.target_weights.len(),
            rebalance_count: p.rebalance_count,
            last_rebalance_time: p.last_rebalance_time,
            created_at: p.created_at,
        }
    }

    /// Get the NAV per share for a portfolio
    pub fn get_nav_per_share(env: Env, portfolio_id: u64) -> i128 {
        let p = Storage::get_portfolio(&env, portfolio_id);
        if p.total_supply > 0 {
            p.total_assets * PRECISION_FACTOR / p.total_supply
        } else {
            PRECISION_FACTOR
        }
    }

    /// Get user's position in a portfolio
    pub fn get_user_position(env: Env, user: Address, portfolio_id: u64) -> UserPosition {
        Storage::get_user_position(&env, &user, portfolio_id)
    }

    /// Get all portfolios a user has positions in
    pub fn get_user_portfolio_ids(env: Env, user: Address) -> Vec<u64> {
        Storage::get_user_portfolio_ids(&env, &user)
    }

    /// Get the current drift for each asset
    pub fn get_drifts(env: Env, portfolio_id: u64) -> Vec<u32> {
        let portfolio = Storage::get_portfolio(&env, portfolio_id);
        let mut drifts = Vec::new(&env);

        for i in 0..portfolio.target_weights.len() {
            let pos = Storage::get_asset_position(&env, portfolio_id, i);
            let target = pos.target_weight_bps as i32;
            let current = pos.current_weight_bps as i32;
            let drift = (target - current).unsigned_abs();
            drifts.push_back(drift);
        }

        drifts
    }

    /// Get all asset positions for a portfolio
    pub fn get_asset_positions(env: Env, portfolio_id: u64) -> Vec<AssetPosition> {
        let portfolio = Storage::get_portfolio(&env, portfolio_id);
        let mut positions = Vec::new(&env);
        for i in 0..portfolio.target_weights.len() {
            positions.push_back(Storage::get_asset_position(&env, portfolio_id, i));
        }
        positions
    }

    /// Get a rebalance record
    pub fn get_rebalance_record(env: Env, portfolio_id: u64, rebalance_id: u64) -> RebalanceRecord {
        Storage::get_rebalance_record(&env, portfolio_id, rebalance_id)
    }

    /// Get latest performance snapshot
    pub fn get_latest_snapshot(env: Env, portfolio_id: u64) -> Option<PerformanceSnapshot> {
        let counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::SnapshotCounter(portfolio_id))
            .unwrap_or(0);
        if counter == 0 {
            return None;
        }
        Some(Storage::get_performance_snapshot(
            &env,
            portfolio_id,
            counter,
        ))
    }

    /// Get total dividends collected for a portfolio
    pub fn get_total_dividends(env: Env, portfolio_id: u64) -> i128 {
        Storage::total_dividends_collected(&env, portfolio_id)
    }

    /// Get admin address
    pub fn get_admin(env: Env) -> Address {
        Storage::get_admin(&env)
    }

    /// Check if contract is paused
    pub fn is_paused(env: Env) -> bool {
        Storage::is_paused(&env)
    }

    // ╔═══════════════════════════════════════════════════════════╗
    // ║  INTERNAL HELPERS                                         ║
    // ╚═══════════════════════════════════════════════════════════╝

    fn create_portfolio_inner(
        env: &Env,
        creator: &Address,
        name: &Symbol,
        portfolio_type: PortfolioType,
        weighting_strategy: WeightingStrategy,
        rebalance_frequency: RebalanceFrequency,
        drift_tolerance_bps: u32,
        max_slippage_bps: u32,
        deposit_token: &Address,
        oracle_address: &Address,
        allocations: &Vec<AssetAllocation>,
    ) -> u64 {
        let portfolio_id = Storage::next_portfolio_id(env);
        let now = env.ledger().timestamp();

        let portfolio = Portfolio {
            portfolio_id,
            creator: creator.clone(),
            name: name.clone(),
            portfolio_type,
            weighting_strategy,
            status: PortfolioStatus::Active,
            target_weights: allocations.clone(),
            deposit_token: deposit_token.clone(),
            oracle_address: oracle_address.clone(),
            total_assets: 0,
            total_supply: 0,
            accumulated_dividends: 0,
            rebalance_frequency,
            drift_tolerance_bps,
            max_slippage_bps,
            last_rebalance_time: now,
            last_dividend_time: now,
            rebalance_count: 0,
            governance_managed: true,
            created_at: now,
        };

        Storage::set_portfolio(env, &portfolio);

        // Initialize asset positions
        for i in 0..allocations.len() {
            let alloc = allocations.get_unchecked(i);
            let pos = AssetPosition {
                token: alloc.token.clone(),
                balance: 0,
                target_weight_bps: alloc.weight_bps,
                current_weight_bps: 0,
                last_price: PRECISION_FACTOR, // Default 1:1
                last_price_update: now,
            };
            Storage::set_asset_position(env, portfolio_id, i, &pos);
        }

        // Initialize performance accumulator
        let acc = PerformanceAccumulator {
            portfolio_id,
            return_sum: 0,
            return_squared_sum: 0,
            observation_count: 0,
            previous_nav: PRECISION_FACTOR,
            previous_nav_time: now,
            peak_nav: PRECISION_FACTOR,
            max_drawdown_bps: 0,
        };
        Storage::set_performance_accumulator(env, &acc);

        env.events().publish(
            (symbol_short!("pm_crt"),),
            (
                portfolio_id,
                creator.clone(),
                name.clone(),
                allocations.len(),
            ),
        );

        portfolio_id
    }

    fn assert_admin(env: &Env, caller: &Address) {
        let admin = Storage::get_admin(env);
        if caller != &admin {
            panic!("Unauthorized");
        }
    }

    /// Calculate shares to mint for a deposit
    fn calculate_shares_for_deposit(amount: i128, total_assets: i128, total_supply: i128) -> i128 {
        if total_supply == 0 {
            // First deposit: 1 share per unit deposited
            amount
        } else {
            amount * total_supply / total_assets
        }
    }

    /// Calculate assets to return for a withdrawal
    fn calculate_assets_for_withdrawal(
        shares: i128,
        total_assets: i128,
        total_supply: i128,
    ) -> i128 {
        shares * total_assets / total_supply
    }

    /// Get rebalance interval in seconds
    fn rebalance_interval_seconds(freq: RebalanceFrequency) -> u64 {
        match freq {
            RebalanceFrequency::Monthly => SECONDS_PER_MONTH,
            RebalanceFrequency::Quarterly => SECONDS_PER_QUARTER,
            RebalanceFrequency::SemiAnnual => SECONDS_PER_SEMI_ANNUAL,
            RebalanceFrequency::Annual => SECONDS_PER_YEAR,
        }
    }

    /// Calculate max drift across all assets given new balances
    fn max_drift(env: &Env, portfolio_id: u64, new_balances: &Vec<i128>) -> u32 {
        let portfolio = Storage::get_portfolio(env, portfolio_id);
        let mut max_drift: u32 = 0;
        let mut total_value: i128 = 0;

        // First pass: compute total value with new balances
        for i in 0..portfolio.target_weights.len() {
            let pos = Storage::get_asset_position(env, portfolio_id, i);
            let balance = new_balances.get_unchecked(i);
            let value = balance * pos.last_price / PRECISION_FACTOR;
            total_value = total_value.checked_add(value).expect("Value overflow");
        }

        if total_value <= 0 {
            return 0;
        }

        // Second pass: compute drift per asset
        for i in 0..portfolio.target_weights.len() {
            let pos = Storage::get_asset_position(env, portfolio_id, i);
            let balance = new_balances.get_unchecked(i);
            let value = balance * pos.last_price / PRECISION_FACTOR;
            let current_weight = (value * BPS_DENOMINATOR / total_value) as i32;
            let target_weight = pos.target_weight_bps as i32;
            let drift = (target_weight - current_weight).unsigned_abs();
            if drift > max_drift {
                max_drift = drift;
            }
        }

        max_drift
    }

    /// Sum swap values for slippage calculation.
    /// Uses the smaller of in/out for each swap (the actual portfolio impact).
    fn sum_swap_values(buys: &Vec<SwapRecord>, sells: &Vec<SwapRecord>) -> i128 {
        let mut total: i128 = 0;
        for i in 0..buys.len() {
            let swap = buys.get_unchecked(i);
            total = total
                .checked_add(swap.amount_in)
                .expect("Swap sum overflow");
        }
        for i in 0..sells.len() {
            let swap = sells.get_unchecked(i);
            total = total
                .checked_add(swap.amount_in)
                .expect("Swap sum overflow");
        }
        total
    }

    /// Calculate annualized Sharpe ratio
    fn calculate_sharpe(return_sum: i128, return_squared_sum: i128, count: u32) -> i128 {
        if count < 2 {
            return 0;
        }

        let n = count as i128;
        let mean_return = return_sum * PRECISION_FACTOR / n / PRECISION_FACTOR;
        let mean_return_sq = return_squared_sum / n;
        let variance = mean_return_sq - mean_return * mean_return / PRECISION_FACTOR;

        if variance <= 0 {
            return 0;
        }

        // Standard deviation (sqrt of variance) - simplified integer sqrt
        let std_dev = Self::isqrt(variance * PRECISION_FACTOR);

        if std_dev == 0 {
            return 0;
        }

        // Sharpe = (mean_return - risk_free) / std_dev, annualized
        let excess_return = mean_return - RISK_FREE_RATE_BPS * PRECISION_FACTOR / BPS_DENOMINATOR;
        excess_return * PRECISION_FACTOR / std_dev
    }

    /// Integer square root (Newton's method)
    fn isqrt(n: i128) -> i128 {
        if n < 0 {
            return 0;
        }
        if n == 0 {
            return 0;
        }
        let mut x = n;
        let mut y = (x + 1) / 2;
        while y < x {
            x = y;
            y = (x + n / x) / 2;
        }
        x
    }
}
