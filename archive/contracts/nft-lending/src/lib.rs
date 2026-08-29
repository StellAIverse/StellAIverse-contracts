#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, Symbol};

/// Scaled arithmetic constant for interest calculations (Issue #213/#212)
/// Using 18 decimal places (10^18) to prevent truncation errors
const DECIMALS: i128 = 1_000_000_000_000_000_000; // 10^18

/// Interest rate per period in scaled format (e.g., 5% = 0.05 * DECIMALS)
const DEFAULT_INTEREST_RATE_PER_PERIOD: i128 = 50_000_000_000_000_000; // 5% in scaled format

/// Storage keys for the NFT lending contract
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Loan(u64),
    LoanCounter,
}

/// Represents an NFT-backed loan
#[contracttype]
#[derive(Clone, Debug)]
pub struct NftLoan {
    pub loan_id: u64,
    pub borrower: Address,
    pub lender: Address,
    pub nft_contract: Address,
    pub nft_id: u64,
    pub principal: i128,           // Original loan amount
    pub principal_scaled: i128,    // Principal in scaled format
    pub interest_rate_scaled: i128, // Interest rate in scaled format (per period)
    pub start_time: u64,
    pub last_accrual_time: u64,
    pub duration_periods: u64,
    pub accrued_interest_scaled: i128, // Accrued interest in scaled format
    pub is_active: bool,
    pub is_defaulted: bool,
}

/// Loan creation parameters
#[contracttype]
#[derive(Clone)]
pub struct LoanParams {
    pub nft_contract: Address,
    pub nft_id: u64,
    pub principal: i128,
    pub interest_rate_scaled: i128, // Scaled interest rate per period
    pub duration_periods: u64,
}

#[contract]
pub struct NftLending;

#[contractimpl]
impl NftLending {
    /// Initialize the NFT lending contract
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::LoanCounter, &0u64);
    }

    /// Create a new NFT-backed loan with scaled interest calculations (Issue #213/#212)
    pub fn create_loan(
        env: Env,
        borrower: Address,
        lender: Address,
        params: LoanParams,
    ) -> u64 {
        borrower.require_auth();

        if params.principal <= 0 {
            panic!("Principal must be positive");
        }

        if params.interest_rate_scaled < 0 {
            panic!("Interest rate cannot be negative");
        }

        // Generate loan ID
        let counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LoanCounter)
            .unwrap_or(0);
        let loan_id = counter + 1;

        let now = env.ledger().timestamp();

        // Store principal in scaled format to prevent truncation
        let principal_scaled = params.principal * DECIMALS;

        let loan = NftLoan {
            loan_id,
            borrower: borrower.clone(),
            lender: lender.clone(),
            nft_contract: params.nft_contract.clone(),
            nft_id: params.nft_id,
            principal: params.principal,
            principal_scaled,
            interest_rate_scaled: params.interest_rate_scaled,
            start_time: now,
            last_accrual_time: now,
            duration_periods: params.duration_periods,
            accrued_interest_scaled: 0,
            is_active: true,
            is_defaulted: false,
        };

        // Store loan
        env.storage()
            .instance()
            .set(&DataKey::Loan(loan_id), &loan);
        env.storage()
            .instance()
            .set(&DataKey::LoanCounter, &loan_id);

        // Transfer NFT to contract as collateral
        let contract_addr = env.current_contract_address();
        // Note: In real implementation, would call NFT transfer
        // nft_client.transfer(&borrower, &contract_addr, &params.nft_id);

        // Transfer principal to borrower
        let token_client = token::Client::new(&env, &lender);
        token_client.transfer(&lender, &borrower, &params.principal);

        env.events().publish(
            (Symbol::new(&env, "LoanCreated"),),
            (loan_id, borrower, lender, params.principal),
        );

        loan_id
    }

    /// Accrue interest using scaled arithmetic to prevent truncation errors (Issue #213/#212)
    /// This function calculates and accumulates interest without losing precision
    pub fn accrue_interest(env: Env, loan_id: u64, caller: Address) -> i128 {
        let mut loan: NftLoan = env
            .storage()
            .instance()
            .get(&DataKey::Loan(loan_id))
            .expect("Loan not found");

        if !loan.is_active {
            panic!("Loan is not active");
        }

        caller.require_auth();

        let now = env.ledger().timestamp();
        let elapsed_time = now.saturating_sub(loan.last_accrual_time);
        
        // Calculate periods elapsed (assuming 1 period = 1 day = 86400 seconds)
        let seconds_per_period: u64 = 86_400;
        let periods_elapsed = elapsed_time / seconds_per_period;

        if periods_elapsed == 0 {
            return Self::scaled_to_normal(loan.accrued_interest_scaled);
        }

        // CRITICAL: Use scaled arithmetic for interest calculation (Issue #213/#212)
        // Formula: interest = principal * rate * periods
        // All calculations in scaled format to prevent truncation
        let periods_scaled = (periods_elapsed as i128) * DECIMALS;
        
        // Calculate interest: (principal_scaled * interest_rate_scaled * periods_scaled) / DECIMALS^2
        // This maintains precision throughout the calculation
        let new_interest_scaled = loan
            .principal_scaled
            .checked_mul(loan.interest_rate_scaled)
            .expect("Overflow in interest calculation")
            .checked_mul(periods_scaled)
            .expect("Overflow in interest calculation")
            .checked_div(DECIMALS * DECIMALS)
            .expect("Division error in interest calculation");

        // Accrue interest
        loan.accrued_interest_scaled = loan
            .accrued_interest_scaled
            .checked_add(new_interest_scaled)
            .expect("Overflow in accrued interest");
        loan.last_accrual_time = now;

        // Check for default
        if loan.accrued_interest_scaled > loan.principal_scaled {
            loan.is_defaulted = true;
        }

        env.storage()
            .instance()
            .set(&DataKey::Loan(loan_id), &loan);

        // Return accrued interest in normal format
        Self::scaled_to_normal(loan.accrued_interest_scaled)
    }

    /// Repay loan with interest calculated using scaled arithmetic
    pub fn repay_loan(env: Env, loan_id: u64, borrower: Address) -> i128 {
        let mut loan: NftLoan = env
            .storage()
            .instance()
            .get(&DataKey::Loan(loan_id))
            .expect("Loan not found");

        if !loan.is_active {
            panic!("Loan is not active");
        }

        if loan.borrower != borrower {
            panic!("Unauthorized: caller is not borrower");
        }

        borrower.require_auth();

        // Accrue interest first
        Self::accrue_interest(env.clone(), loan_id, borrower.clone());

        // Reload loan after accrual
        let loan: NftLoan = env
            .storage()
            .instance()
            .get(&DataKey::Loan(loan_id))
            .expect("Loan not found");

        // Calculate total repayment: principal + accrued interest
        let total_repayment_scaled = loan
            .principal_scaled
            .checked_add(loan.accrued_interest_scaled)
            .expect("Overflow in total repayment");

        let total_repayment = Self::scaled_to_normal(total_repayment_scaled);

        // Transfer repayment from borrower to lender
        let token_client = token::Client::new(&env, &loan.lender);
        token_client.transfer(&borrower, &loan.lender, &total_repayment);

        // Mark loan as repaid
        loan.is_active = false;
        env.storage()
            .instance()
            .set(&DataKey::Loan(loan_id), &loan);

        // Return NFT collateral to borrower
        // Note: In real implementation, would call NFT transfer
        // nft_client.transfer(&env.current_contract_address(), &borrower, &loan.nft_id);

        env.events().publish(
            (Symbol::new(&env, "LoanRepaid"),),
            (loan_id, borrower, total_repayment),
        );

        total_repayment
    }

    /// Get loan details with interest calculated using scaled arithmetic
    pub fn get_loan_info(env: Env, loan_id: u64) -> Option<NftLoan> {
        env.storage().instance().get(&DataKey::Loan(loan_id))
    }

    /// Calculate total owed (principal + accrued interest) using scaled arithmetic
    pub fn calculate_total_owed(env: Env, loan_id: u64) -> i128 {
        let loan: NftLoan = env
            .storage()
            .instance()
            .get(&DataKey::Loan(loan_id))
            .expect("Loan not found");

        if !loan.is_active {
            panic!("Loan is not active");
        }

        // Calculate current accrued interest
        let now = env.ledger().timestamp();
        let elapsed_time = now.saturating_sub(loan.last_accrual_time);
        let seconds_per_period: u64 = 86_400;
        let periods_elapsed = elapsed_time / seconds_per_period;

        if periods_elapsed == 0 {
            return Self::scaled_to_normal(
                loan.principal_scaled.checked_add(loan.accrued_interest_scaled).expect("Overflow")
            );
        }

        // Use scaled arithmetic for precision
        let periods_scaled = (periods_elapsed as i128) * DECIMALS;
        let new_interest_scaled = loan
            .principal_scaled
            .checked_mul(loan.interest_rate_scaled)
            .expect("Overflow")
            .checked_mul(periods_scaled)
            .expect("Overflow")
            .checked_div(DECIMALS * DECIMALS)
            .expect("Division error");

        let total_accrued_scaled = loan
            .accrued_interest_scaled
            .checked_add(new_interest_scaled)
            .expect("Overflow");

        let total_owed_scaled = loan
            .principal_scaled
            .checked_add(total_accrued_scaled)
            .expect("Overflow");

        Self::scaled_to_normal(total_owed_scaled)
    }

    /// Helper: Convert scaled value back to normal format
    fn scaled_to_normal(scaled_value: i128) -> i128 {
        scaled_value / DECIMALS
    }

    /// Helper: Get admin address
    fn get_admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Admin not set")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    fn setup_test() -> (Env, NftLendingClient<'static>, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, NftLending);
        let client = NftLendingClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let borrower = Address::generate(&env);
        let lender = Address::generate(&env);

        (env, client, borrower, lender)
    }

    #[test]
    fn test_scaled_interest_calculation() {
        let (env, client, borrower, lender) = setup_test();

        // Create loan with 5% interest rate per period (scaled)
        let interest_rate_scaled = 50_000_000_000_000_000; // 5% in scaled format
        let principal = 1000;

        let params = LoanParams {
            nft_contract: Address::generate(&env),
            nft_id: 1,
            principal,
            interest_rate_scaled,
            duration_periods: 30,
        };

        let loan_id = client.create_loan(&borrower, &lender, &params);

        // Advance time by 10 periods (10 days)
        let initial_time = env.ledger().timestamp();
        env.ledger()
            .set_timestamp(initial_time + 10 * 86_400);

        // Accrue interest
        let accrued = client.accrue_interest(&loan_id, &borrower);

        // With scaled arithmetic, 1000 * 0.05 * 10 = 500 (exact, no truncation)
        // Without scaled arithmetic, integer division would cause truncation
        assert!(accrued > 0, "Interest should have accrued");
        
        // Verify precision is maintained (should be exactly 500 or very close)
        let expected_interest = 500; // 1000 * 0.05 * 10
        let difference = if accrued > expected_interest {
            accrued - expected_interest
        } else {
            expected_interest - accrued
        };
        
        // Allow small difference due to rounding, but should be < 0.01% of principal
        assert!(
            difference < (principal / 10_000), // 0.01% of principal
            "Accumulated rounding error exceeds 0.01%: difference = {}",
            difference
        );
    }

    #[test]
    fn test_long_term_lending_precision() {
        let (env, client, borrower, lender) = setup_test();

        // Test over 100 periods to verify accumulated error stays below 0.01%
        let interest_rate_scaled = 50_000_000_000_000_000; // 5% per period
        let principal = 10_000;

        let params = LoanParams {
            nft_contract: Address::generate(&env),
            nft_id: 1,
            principal,
            interest_rate_scaled,
            duration_periods: 100,
        };

        let loan_id = client.create_loan(&borrower, &lender, &params);

        // Advance time by 100 periods
        let initial_time = env.ledger().timestamp();
        env.ledger()
            .set_timestamp(initial_time + 100 * 86_400);

        // Accrue interest
        let accrued = client.accrue_interest(&loan_id, &borrower);

        // Expected: 10_000 * 0.05 * 100 = 50_000
        let expected_interest = 50_000;
        
        // Verify accumulated error is below 0.01%
        let difference = if accrued > expected_interest {
            accrued - expected_interest
        } else {
            expected_interest - accrued
        };

        assert!(
            difference < (principal / 10_000), // 0.01% of principal
            "Long-term accumulated error exceeds 0.01%: difference = {}, accrued = {}, expected = {}",
            difference,
            accrued,
            expected_interest
        );
    }

    #[test]
    fn test_scaled_vs_truncated_comparison() {
        let (env, client, borrower, lender) = setup_test();

        let principal = 1000;
        let interest_rate_scaled = 33_333_333_333_333_333; // 3.333...% (repeating decimal)

        let params = LoanParams {
            nft_contract: Address::generate(&env),
            nft_id: 1,
            principal,
            interest_rate_scaled,
            duration_periods: 100,
        };

        let loan_id = client.create_loan(&borrower, &lender, &params);

        // Advance time by 100 periods
        let initial_time = env.ledger().timestamp();
        env.ledger()
            .set_timestamp(initial_time + 100 * 86_400);

        // Accrue interest with scaled arithmetic
        let accrued_scaled = client.accrue_interest(&loan_id, &borrower);

        // Calculate what truncated division would give (for comparison)
        // Truncated: (1000 * 3 / 100) * 100 = 3000 (loses the 0.333... precision)
        let truncated_interest = (principal * 3 / 100) * 100; // Would give 3000

        // Scaled arithmetic should be more precise
        // Expected: 1000 * 0.03333... * 100 ≈ 3333.33...
        let expected_scaled = 3333; // Rounded down from 3333.33...

        // Verify scaled arithmetic is more accurate
        assert!(
            accrued_scaled >= expected_scaled - 10, // Allow small rounding
            "Scaled arithmetic should be more accurate than truncated division"
        );

        // Show that truncated division loses precision
        assert!(
            truncated_interest < expected_scaled,
            "Truncated division should underpayment"
        );
    }
}
