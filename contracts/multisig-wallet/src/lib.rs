#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token::TokenClient, Address, Env, String,
    Vec,
};

const MAX_SIGNERS: u32 = 20;
const MAX_HISTORY_LIMIT: u32 = 100;
const MAX_MEMO_LENGTH: u32 = 128;
const DAY_SECONDS: u64 = 86_400;

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Config,
    Signer(Address),
    SignerList,
    Whitelist(Address),
    Transaction(u64),
    Confirmations(u64),
    ExecutionHistory,
    DailySpent(Address, u64),
    ReentrancyLock,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct WalletConfig {
    pub admin: Address,
    pub required_confirmations: u32,
    pub signer_count: u32,
    pub daily_limit: i128,
    pub next_tx_id: u64,
    pub next_nonce: u64,
    pub next_executable_nonce: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct WalletTransaction {
    pub tx_id: u64,
    pub nonce: u64,
    pub token: Address,
    pub recipient: Address,
    pub amount: i128,
    pub submitted_by: Address,
    pub executed: bool,
    pub created_at: u64,
    pub executed_at: u64,
    pub memo: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ExecutionReceipt {
    pub tx_id: u64,
    pub nonce: u64,
    pub token: Address,
    pub recipient: Address,
    pub amount: i128,
    pub confirmation_count: u32,
    pub executed_at: u64,
}

#[contract]
pub struct MultiSigWallet;

#[contractimpl]
impl MultiSigWallet {
    pub fn initialize(
        env: Env,
        admin: Address,
        signers: Vec<Address>,
        required_confirmations: u32,
        daily_limit: i128,
    ) {
        if env.storage().instance().has(&DataKey::Config) {
            panic!("Already initialized");
        }
        admin.require_auth();
        Self::validate_daily_limit(daily_limit);
        Self::validate_signer_set(&env, &signers, required_confirmations);

        for idx in 0..signers.len() {
            let signer = signers.get_unchecked(idx);
            env.storage()
                .instance()
                .set(&DataKey::Signer(signer), &true);
        }
        env.storage().instance().set(&DataKey::SignerList, &signers);
        env.storage()
            .instance()
            .set(&DataKey::ExecutionHistory, &Vec::<u64>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &false);

        let config = WalletConfig {
            admin: admin.clone(),
            required_confirmations,
            signer_count: signers.len(),
            daily_limit,
            next_tx_id: 1,
            next_nonce: 1,
            next_executable_nonce: 1,
        };
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish(
            (symbol_short!("msig_init"),),
            (admin, required_confirmations, config.signer_count),
        );
    }

    pub fn deposit(env: Env, from: Address, token: Address, amount: i128) {
        from.require_auth();
        Self::validate_amount(amount);
        let wallet = env.current_contract_address();
        Self::transfer_token(&env, &token, &from, &wallet, amount);
        env.events()
            .publish((symbol_short!("msig_dep"),), (from, token, amount));
    }

    pub fn submit_transaction(
        env: Env,
        signer: Address,
        token: Address,
        recipient: Address,
        amount: i128,
        memo: String,
    ) -> u64 {
        signer.require_auth();
        Self::assert_signer(&env, &signer);
        Self::validate_amount(amount);
        Self::validate_memo(&memo);

        let mut config = Self::config(&env);
        let tx_id = config.next_tx_id;
        let nonce = config.next_nonce;
        config.next_tx_id = config.next_tx_id.checked_add(1).expect("Tx ID overflow");
        config.next_nonce = config.next_nonce.checked_add(1).expect("Nonce overflow");

        let transaction = WalletTransaction {
            tx_id,
            nonce,
            token: token.clone(),
            recipient: recipient.clone(),
            amount,
            submitted_by: signer.clone(),
            executed: false,
            created_at: env.ledger().timestamp(),
            executed_at: 0,
            memo,
        };

        let mut confirmations = Vec::new(&env);
        confirmations.push_back(signer.clone());

        env.storage()
            .instance()
            .set(&DataKey::Transaction(tx_id), &transaction);
        env.storage()
            .instance()
            .set(&DataKey::Confirmations(tx_id), &confirmations);
        env.storage().instance().set(&DataKey::Config, &config);

        env.events().publish(
            (symbol_short!("msig_sub"),),
            (tx_id, nonce, signer, token, recipient, amount),
        );

        tx_id
    }

    pub fn confirm_transaction(env: Env, signer: Address, tx_id: u64) -> u32 {
        signer.require_auth();
        Self::assert_signer(&env, &signer);

        let transaction = Self::load_transaction(&env, tx_id);
        if transaction.executed {
            panic!("Transaction already executed");
        }

        let mut confirmations = Self::confirmations(&env, tx_id);
        if Self::vec_contains_address(&confirmations, &signer) {
            panic!("Transaction already confirmed by signer");
        }
        confirmations.push_back(signer.clone());
        env.storage()
            .instance()
            .set(&DataKey::Confirmations(tx_id), &confirmations);

        let count = Self::active_confirmation_count(&env, &confirmations);
        env.events()
            .publish((symbol_short!("msig_con"),), (tx_id, signer, count));
        count
    }

    pub fn revoke_confirmation(env: Env, signer: Address, tx_id: u64) -> u32 {
        signer.require_auth();
        Self::assert_signer(&env, &signer);

        let transaction = Self::load_transaction(&env, tx_id);
        if transaction.executed {
            panic!("Transaction already executed");
        }

        let mut confirmations = Self::confirmations(&env, tx_id);
        let removed = Self::remove_address(&mut confirmations, &signer);
        if !removed {
            panic!("Signer has not confirmed transaction");
        }
        env.storage()
            .instance()
            .set(&DataKey::Confirmations(tx_id), &confirmations);

        let count = Self::active_confirmation_count(&env, &confirmations);
        env.events()
            .publish((symbol_short!("msig_rev"),), (tx_id, signer, count));
        count
    }

    pub fn execute_transaction(env: Env, signer: Address, tx_id: u64) -> ExecutionReceipt {
        signer.require_auth();
        Self::assert_signer(&env, &signer);

        let mut config = Self::config(&env);
        let mut transaction = Self::load_transaction(&env, tx_id);
        if transaction.executed {
            panic!("Transaction already executed");
        }
        if transaction.nonce != config.next_executable_nonce {
            panic!("Transaction nonce is not next");
        }

        let confirmations = Self::confirmations(&env, tx_id);
        let confirmation_count = Self::active_confirmation_count(&env, &confirmations);
        if confirmation_count < config.required_confirmations {
            panic!("Insufficient confirmations");
        }

        Self::apply_daily_limit(&env, &transaction);

        transaction.executed = true;
        transaction.executed_at = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&DataKey::Transaction(tx_id), &transaction);

        config.next_executable_nonce = config
            .next_executable_nonce
            .checked_add(1)
            .expect("Executable nonce overflow");
        env.storage().instance().set(&DataKey::Config, &config);

        let wallet = env.current_contract_address();
        Self::transfer_token(
            &env,
            &transaction.token,
            &wallet,
            &transaction.recipient,
            transaction.amount,
        );
        Self::append_history(&env, tx_id);

        let receipt = ExecutionReceipt {
            tx_id,
            nonce: transaction.nonce,
            token: transaction.token.clone(),
            recipient: transaction.recipient.clone(),
            amount: transaction.amount,
            confirmation_count,
            executed_at: transaction.executed_at,
        };

        env.events().publish(
            (symbol_short!("msig_exe"),),
            (
                tx_id,
                transaction.nonce,
                signer,
                transaction.recipient,
                transaction.amount,
            ),
        );

        receipt
    }

    pub fn add_signer(env: Env, admin: Address, new_signer: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if Self::is_signer_inner(&env, &new_signer) {
            panic!("Address is already signer");
        }

        let mut signers = Self::signer_list(&env);
        if signers.len() >= MAX_SIGNERS {
            panic!("Maximum signer count reached");
        }
        signers.push_back(new_signer.clone());
        env.storage()
            .instance()
            .set(&DataKey::Signer(new_signer.clone()), &true);
        env.storage().instance().set(&DataKey::SignerList, &signers);

        let mut config = Self::config(&env);
        config.signer_count = signers.len();
        env.storage().instance().set(&DataKey::Config, &config);

        env.events()
            .publish((symbol_short!("msig_add"),), (admin, new_signer));
    }

    pub fn remove_signer(env: Env, admin: Address, signer: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if !Self::is_signer_inner(&env, &signer) {
            panic!("Address is not signer");
        }

        let mut signers = Self::signer_list(&env);
        Self::remove_address(&mut signers, &signer);
        if signers.is_empty() {
            panic!("Cannot remove last signer");
        }

        let mut config = Self::config(&env);
        if config.required_confirmations > signers.len() {
            panic!("Required confirmations exceed signer count");
        }
        config.signer_count = signers.len();

        env.storage()
            .instance()
            .set(&DataKey::Signer(signer.clone()), &false);
        env.storage().instance().set(&DataKey::SignerList, &signers);
        env.storage().instance().set(&DataKey::Config, &config);

        env.events()
            .publish((symbol_short!("msig_rem"),), (admin, signer));
    }

    pub fn change_requirement(env: Env, admin: Address, required_confirmations: u32) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut config = Self::config(&env);
        if required_confirmations == 0 || required_confirmations > config.signer_count {
            panic!("Invalid confirmation requirement");
        }
        config.required_confirmations = required_confirmations;
        env.storage().instance().set(&DataKey::Config, &config);

        env.events().publish(
            (symbol_short!("msig_req"),),
            (admin, required_confirmations),
        );
    }

    pub fn set_daily_limit(env: Env, admin: Address, daily_limit: i128) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        Self::validate_daily_limit(daily_limit);

        let mut config = Self::config(&env);
        config.daily_limit = daily_limit;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((symbol_short!("msig_lim"),), (admin, daily_limit));
    }

    pub fn set_whitelist(env: Env, admin: Address, recipient: Address, allowed: bool) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Whitelist(recipient.clone()), &allowed);
        env.events()
            .publish((symbol_short!("msig_wht"),), (admin, recipient, allowed));
    }

    pub fn get_config(env: Env) -> WalletConfig {
        Self::config(&env)
    }

    pub fn get_signers(env: Env) -> Vec<Address> {
        Self::signer_list(&env)
    }

    pub fn is_signer(env: Env, address: Address) -> bool {
        Self::is_signer_inner(&env, &address)
    }

    pub fn is_whitelisted(env: Env, recipient: Address) -> bool {
        Self::is_whitelisted_inner(&env, &recipient)
    }

    pub fn get_transaction(env: Env, tx_id: u64) -> WalletTransaction {
        Self::load_transaction(&env, tx_id)
    }

    pub fn get_confirmations(env: Env, tx_id: u64) -> Vec<Address> {
        Self::confirmations(&env, tx_id)
    }

    pub fn confirmation_count(env: Env, tx_id: u64) -> u32 {
        let confirmations = Self::confirmations(&env, tx_id);
        Self::active_confirmation_count(&env, &confirmations)
    }

    pub fn get_transaction_history(env: Env, limit: u32) -> Vec<WalletTransaction> {
        if limit > MAX_HISTORY_LIMIT {
            panic!("History limit too large");
        }

        let history_ids = Self::history_ids(&env);
        let mut history = Vec::new(&env);
        let start = if history_ids.len() > limit {
            history_ids.len() - limit
        } else {
            0
        };
        for idx in start..history_ids.len() {
            let tx_id = history_ids.get_unchecked(idx);
            history.push_back(Self::load_transaction(&env, tx_id));
        }
        history
    }

    pub fn daily_spent(env: Env, token: Address, day_index: u64) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::DailySpent(token, day_index))
            .unwrap_or(0)
    }

    fn validate_signer_set(env: &Env, signers: &Vec<Address>, required_confirmations: u32) {
        if signers.is_empty() {
            panic!("At least one signer required");
        }
        if signers.len() > MAX_SIGNERS {
            panic!("Too many signers");
        }
        if required_confirmations == 0 || required_confirmations > signers.len() {
            panic!("Invalid confirmation requirement");
        }

        let mut seen = Vec::new(env);
        for idx in 0..signers.len() {
            let signer = signers.get_unchecked(idx);
            if Self::vec_contains_address(&seen, &signer) {
                panic!("Duplicate signer");
            }
            seen.push_back(signer);
        }
    }

    fn apply_daily_limit(env: &Env, transaction: &WalletTransaction) {
        if Self::is_whitelisted_inner(env, &transaction.recipient) {
            return;
        }

        let config = Self::config(env);
        let day_index = env.ledger().timestamp() / DAY_SECONDS;
        let key = DataKey::DailySpent(transaction.token.clone(), day_index);
        let spent: i128 = env.storage().instance().get(&key).unwrap_or(0);
        let new_spent = spent
            .checked_add(transaction.amount)
            .expect("Daily spent overflow");
        if new_spent > config.daily_limit {
            panic!("Daily spending limit exceeded");
        }
        env.storage().instance().set(&key, &new_spent);
    }

    fn append_history(env: &Env, tx_id: u64) {
        let mut history = Self::history_ids(env);
        history.push_back(tx_id);
        env.storage()
            .instance()
            .set(&DataKey::ExecutionHistory, &history);
    }

    fn transfer_token(env: &Env, token: &Address, from: &Address, to: &Address, amount: i128) {
        Self::enter_non_reentrant(env);
        let token_client = TokenClient::new(env, token);
        token_client.transfer(from, to, &amount);
        Self::exit_non_reentrant(env);
    }

    fn enter_non_reentrant(env: &Env) {
        let locked = env
            .storage()
            .instance()
            .get(&DataKey::ReentrancyLock)
            .unwrap_or(false);
        if locked {
            panic!("Reentrant call blocked");
        }
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &true);
    }

    fn exit_non_reentrant(env: &Env) {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &false);
    }

    fn validate_amount(amount: i128) {
        if amount <= 0 {
            panic!("Amount must be positive");
        }
    }

    fn validate_daily_limit(daily_limit: i128) {
        if daily_limit <= 0 {
            panic!("Daily limit must be positive");
        }
    }

    fn validate_memo(memo: &String) {
        if memo.len() > MAX_MEMO_LENGTH {
            panic!("Memo exceeds maximum length");
        }
    }

    fn config(env: &Env) -> WalletConfig {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .expect("Wallet not initialized")
    }

    fn signer_list(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::SignerList)
            .unwrap_or_else(|| Vec::new(env))
    }

    fn history_ids(env: &Env) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&DataKey::ExecutionHistory)
            .unwrap_or_else(|| Vec::new(env))
    }

    fn confirmations(env: &Env, tx_id: u64) -> Vec<Address> {
        Self::load_transaction(env, tx_id);
        env.storage()
            .instance()
            .get(&DataKey::Confirmations(tx_id))
            .unwrap_or_else(|| Vec::new(env))
    }

    fn load_transaction(env: &Env, tx_id: u64) -> WalletTransaction {
        if tx_id == 0 {
            panic!("Invalid transaction ID");
        }
        env.storage()
            .instance()
            .get(&DataKey::Transaction(tx_id))
            .expect("Transaction not found")
    }

    fn assert_admin(env: &Env, caller: &Address) {
        let config = Self::config(env);
        if caller != &config.admin {
            panic!("Unauthorized: caller is not admin");
        }
    }

    fn assert_signer(env: &Env, caller: &Address) {
        if !Self::is_signer_inner(env, caller) {
            panic!("Caller is not signer");
        }
    }

    fn is_signer_inner(env: &Env, address: &Address) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Signer(address.clone()))
            .unwrap_or(false)
    }

    fn is_whitelisted_inner(env: &Env, recipient: &Address) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Whitelist(recipient.clone()))
            .unwrap_or(false)
    }

    fn active_confirmation_count(env: &Env, confirmations: &Vec<Address>) -> u32 {
        let mut count = 0u32;
        for idx in 0..confirmations.len() {
            let signer = confirmations.get_unchecked(idx);
            if Self::is_signer_inner(env, &signer) {
                count = count.checked_add(1).expect("Confirmation count overflow");
            }
        }
        count
    }

    fn vec_contains_address(addresses: &Vec<Address>, needle: &Address) -> bool {
        for idx in 0..addresses.len() {
            if addresses.get_unchecked(idx) == *needle {
                return true;
            }
        }
        false
    }

    fn remove_address(addresses: &mut Vec<Address>, needle: &Address) -> bool {
        let mut idx = 0;
        while idx < addresses.len() {
            if addresses.get_unchecked(idx) == *needle {
                addresses.remove(idx);
                return true;
            }
            idx += 1;
        }
        false
    }
}

#[cfg(test)]
mod test;
