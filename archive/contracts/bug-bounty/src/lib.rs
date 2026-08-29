#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Symbol};
use stellai_lib::{admin, errors::ContractError, ADMIN_KEY};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BugBountySubmission {
    pub submitter: Address,
    pub bug_id: u64,
    pub description: String,
    pub severity: Severity,
    pub status: BountyStatus,
    pub submitted_at: u64,
    pub reviewed_at: Option<u64>,
    pub reviewer: Option<Address>,
    pub reward_amount: i128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
#[repr(u32)]
pub enum Severity {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
#[repr(u32)]
pub enum BountyStatus {
    Submitted = 0,
    UnderReview = 1,
    Approved = 2,
    Rejected = 3,
    Paid = 4,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BountyConfig {
    pub low_reward: i128,
    pub medium_reward: i128,
    pub high_reward: i128,
    pub critical_reward: i128,
    pub review_period_days: u64,
}

#[contract]
pub struct BugBounty;

#[contractimpl]
impl BugBounty {
    /// Initialize the bug bounty contract with admin and bounty configuration.
    pub fn init_contract(
        env: Env,
        admin_addr: Address,
        config: BountyConfig,
    ) -> Result<(), ContractError> {
        if env.storage().instance().has(&Symbol::new(&env, ADMIN_KEY)) {
            return Err(ContractError::AlreadyInitialized);
        }
        admin_addr.require_auth();

        // Validate configuration
        if config.low_reward <= 0
            || config.medium_reward <= config.low_reward
            || config.high_reward <= config.medium_reward
            || config.critical_reward <= config.high_reward
        {
            return Err(ContractError::InvalidAgentId); // Using existing error for invalid amounts
        }

        env.storage()
            .instance()
            .set(&Symbol::new(&env, ADMIN_KEY), &admin_addr);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "config"), &config);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "bug_counter"), &0u64);

        Ok(())
    }

    /// Submit a bug bounty report.
    pub fn submit_bug(
        env: Env,
        submitter: Address,
        description: String,
        severity: Severity,
    ) -> Result<u64, ContractError> {
        submitter.require_auth();

        // Validate description length
        if description.len() > 1000 {
            return Err(ContractError::InvalidAgentId); // Using existing error for invalid input
        }

        let bug_id: u64 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "bug_counter"))
            .unwrap_or(0);
        let new_bug_id = bug_id + 1;

        let submission = BugBountySubmission {
            submitter: submitter.clone(),
            bug_id: new_bug_id,
            description: description.clone(),
            severity,
            status: BountyStatus::Submitted,
            submitted_at: env.ledger().timestamp(),
            reviewed_at: None,
            reviewer: None,
            reward_amount: 0,
        };

        let key = (Symbol::new(&env, "bug"), new_bug_id);
        env.storage().instance().set(&key, &submission);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "bug_counter"), &new_bug_id);

        env.events().publish(
            (Symbol::new(&env, "bug"), Symbol::new(&env, "submitted")),
            (new_bug_id, submitter, severity),
        );

        Ok(new_bug_id)
    }

    /// Review a bug submission (admin only).
    pub fn review_bug(
        env: Env,
        admin: Address,
        bug_id: u64,
        approved: bool,
        reviewer_notes: String,
    ) -> Result<(), ContractError> {
        admin::verify_admin(&env, &admin)?;

        let key = (Symbol::new(&env, "bug"), bug_id);
        let mut submission: BugBountySubmission = env
            .storage()
            .instance()
            .get(&key)
            .ok_or(ContractError::InvalidAgentId)?; // Using existing error for not found

        if submission.status != BountyStatus::Submitted
            && submission.status != BountyStatus::UnderReview
        {
            return Err(ContractError::InvalidAgentId); // Using existing error for invalid state
        }

        let config: BountyConfig = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "config"))
            .ok_or(ContractError::NotInitialized)?;

        submission.status = if approved {
            BountyStatus::Approved
        } else {
            BountyStatus::Rejected
        };
        submission.reviewed_at = Some(env.ledger().timestamp());
        submission.reviewer = Some(admin.clone());

        if approved {
            submission.reward_amount = match submission.severity {
                Severity::Low => config.low_reward,
                Severity::Medium => config.medium_reward,
                Severity::High => config.high_reward,
                Severity::Critical => config.critical_reward,
            };
        }

        env.storage().instance().set(&key, &submission);

        env.events().publish(
            (Symbol::new(&env, "bug"), Symbol::new(&env, "reviewed")),
            (bug_id, approved, submission.reward_amount),
        );

        Ok(())
    }

    /// Pay bounty for an approved bug (admin only).
    pub fn pay_bounty(env: Env, admin: Address, bug_id: u64) -> Result<(), ContractError> {
        admin::verify_admin(&env, &admin)?;

        let key = (Symbol::new(&env, "bug"), bug_id);
        let mut submission: BugBountySubmission = env
            .storage()
            .instance()
            .get(&key)
            .ok_or(ContractError::InvalidAgentId)?; // Using existing error for not found

        if submission.status != BountyStatus::Approved {
            return Err(ContractError::InvalidAgentId); // Using existing error for invalid state
        }

        submission.status = BountyStatus::Paid;
        env.storage().instance().set(&key, &submission);

        // Update total paid amount
        let total_key = Symbol::new(&env, "total_paid");
        let mut total_paid: i128 = env.storage().instance().get(&total_key).unwrap_or(0);
        total_paid += submission.reward_amount;
        env.storage().instance().set(&total_key, &total_paid);

        env.events().publish(
            (Symbol::new(&env, "bug"), Symbol::new(&env, "paid")),
            (bug_id, submission.submitter, submission.reward_amount),
        );

        Ok(())
    }

    /// Get bug submission details.
    pub fn get_bug_submission(env: Env, bug_id: u64) -> Result<BugBountySubmission, ContractError> {
        let key = (Symbol::new(&env, "bug"), bug_id);
        env.storage()
            .instance()
            .get(&key)
            .ok_or(ContractError::InvalidAgentId) // Using existing error for not found
    }

    /// Get bounty configuration.
    pub fn get_bounty_config(env: Env) -> Result<BountyConfig, ContractError> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "config"))
            .ok_or(ContractError::NotInitialized)
    }

    /// Get total amount paid in bounties.
    pub fn get_total_paid(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "total_paid"))
            .unwrap_or(0)
    }

    /// Get bug submissions by status.
    pub fn get_bugs_by_status(
        env: Env,
        status: BountyStatus,
        limit: u32,
    ) -> Vec<BugBountySubmission> {
        let bug_counter: u64 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "bug_counter"))
            .unwrap_or(0);
        let mut results = Vec::new(&env);

        for bug_id in 1..=bug_counter {
            if results.len() >= limit as u32 {
                break;
            }

            let key = (Symbol::new(&env, "bug"), bug_id);
            if let Ok(submission) = env.storage().instance().get::<_, BugBountySubmission>(&key) {
                if submission.status == status {
                    results.push_back(submission);
                }
            }
        }

        results
    }

    /// Get bug submissions by submitter.
    pub fn get_bugs_by_submitter(
        env: Env,
        submitter: Address,
        limit: u32,
    ) -> Vec<BugBountySubmission> {
        let bug_counter: u64 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "bug_counter"))
            .unwrap_or(0);
        let mut results = Vec::new(&env);

        for bug_id in 1..=bug_counter {
            if results.len() >= limit as u32 {
                break;
            }

            let key = (Symbol::new(&env, "bug"), bug_id);
            if let Ok(submission) = env.storage().instance().get::<_, BugBountySubmission>(&key) {
                if submission.submitter == submitter {
                    results.push_back(submission);
                }
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_bug_bounty_flow() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let submitter = Address::generate(&env);

        let config = BountyConfig {
            low_reward: 100,
            medium_reward: 500,
            high_reward: 1000,
            critical_reward: 5000,
            review_period_days: 30,
        };

        let contract_id = env.register_contract(None, BugBounty);
        let client = BugBountyClient::new(&env, &contract_id);

        client.init_contract(&admin, &config);

        // Submit a bug
        let bug_id = client.submit_bug(
            &submitter,
            &String::from_str(&env, "Test bug"),
            &Severity::High,
        );
        assert!(bug_id > 0);

        // Get submission
        let submission = client.get_bug_submission(&bug_id);
        assert_eq!(submission.submitter, submitter);
        assert_eq!(submission.severity, Severity::High);
        assert_eq!(submission.status, BountyStatus::Submitted);

        // Review and approve
        client.review_bug(
            &admin,
            &bug_id,
            &true,
            &String::from_str(&env, "Good catch!"),
        );
        let reviewed = client.get_bug_submission(&bug_id);
        assert_eq!(reviewed.status, BountyStatus::Approved);
        assert_eq!(reviewed.reward_amount, 1000);

        // Pay bounty
        client.pay_bounty(&admin, &bug_id);
        let paid = client.get_bug_submission(&bug_id);
        assert_eq!(paid.status, BountyStatus::Paid);
        assert_eq!(client.get_total_paid(), 1000);
    }
}
