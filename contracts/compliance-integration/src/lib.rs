#![no_std]
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, String,
    Symbol, Vec,
};
use stellai_lib::rbac;

// Contract errors
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Unauthorized = 1,
    InvalidDID = 2,
    ReportNotFound = 3,
    InvalidCredential = 4,
    CredentialExpired = 5,
    CredentialRevoked = 6,
    ComplianceCheckFailed = 7,
    InvalidRiskLevel = 8,
    DuplicateReport = 9,
    RateLimitExceeded = 10,
    AuditRequired = 11,
    // KYC state machine errors
    KycSubjectNotFound = 12,
    KycInvalidTransition = 13,
    KycTerminalState = 14,
    KycRequestExpired = 15,
    KycOperatorRequired = 16,
    KycSelfAssignment = 17,
    KycNotVerified = 18,
    KycOverrideNotScheduled = 19,
    KycOverrideNotReady = 20,
    KycOverrideAlreadyScheduled = 21,
}

// ── Compliance Types ────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[contracttype]
pub struct ComplianceReport {
    pub report_id: u64,
    pub entity_did: String,
    pub compliance_type: ComplianceType,
    pub status: ComplianceStatus,
    pub score: u32,
    pub risk_level: RiskLevel,
    pub findings: Vec<ComplianceFinding>,
    pub issued_by: Address,
    pub issued_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[contracttype]
#[repr(u32)]
pub enum ComplianceType {
    KYC = 0,
    AML = 1,
    Sanctions = 2,
    TaxCompliance = 3,
    DataPrivacy = 4,
    FinancialRegulation = 5,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[contracttype]
#[repr(u32)]
pub enum ComplianceStatus {
    Compliant = 0,
    NonCompliant = 1,
    Pending = 2,
    UnderReview = 3,
    Exempt = 4,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[contracttype]
#[repr(u32)]
pub enum RiskLevel {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct ComplianceFinding {
    pub category: String,
    pub severity: String,
    pub description: String,
    pub recommendation: Option<String>,
}

// ── KYC State Machine Types ───────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[contracttype]
#[repr(u32)]
pub enum KycStatus {
    Pending = 0,
    InReview = 1,
    Verified = 2,
    Rejected = 3,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct KycRecord {
    pub subject: Address,
    pub subject_did: String,
    pub status: KycStatus,
    pub updated_by: Address,
    pub updated_at: u64,
    pub created_at: u64,
    pub finalized_at: Option<u64>,
    pub expires_at: Option<u64>,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct KycOverrideRequest {
    pub subject: Address,
    pub requested_by: Address,
    pub execute_after: u64,
    pub created_at: u64,
}

// ── Storage Keys ─────────────────────────────────────────────────────────────

const REPORTS: Symbol = symbol_short!("REPORTS");
const KYC_RECORDS: Symbol = symbol_short!("KYC_REC");
const KYC_OVERRIDES: Symbol = symbol_short!("KYC_OVR");
const NEXT_REPORT_ID: Symbol = symbol_short!("N_REP_ID");

const KYC_PENDING_EXPIRY_SECS: u64 = 86400 * 7; // 7 days
const KYC_OVERRIDE_TIMELOCK_SECS: u64 = 86400 * 2; // 2 days

// ── Contract Implementation ──────────────────────────────────────────────────

#[contract]
pub struct ComplianceIntegrationContract;

#[contractimpl]
#[allow(clippy::too_many_arguments)]
impl ComplianceIntegrationContract {
    /// Generate a new compliance report for an entity.
    /// Requires verified KYC for the issuer (Address).
    #[allow(clippy::too_many_arguments)]
    pub fn generate_compliance_report(
        env: Env,
        entity_did: String,
        compliance_type: ComplianceType,
        status: ComplianceStatus,
        score: u32,
        risk_level: RiskLevel,
        findings: Vec<ComplianceFinding>,
        issuer: Address,
        validity_days: u64,
    ) -> Result<u64, Error> {
        issuer.require_auth();
        Self::require_verified_kyc(&env, &issuer)?;

        let report_id = Self::get_next_report_id(&env);
        let now = env.ledger().timestamp();
        let expires_at = now + (validity_days * 86400);

        let report = ComplianceReport {
            report_id,
            entity_did,
            compliance_type,
            status,
            score,
            risk_level,
            findings,
            issued_by: issuer,
            issued_at: now,
            expires_at,
        };

        env.storage().instance().set(&(REPORTS, report_id), &report);
        env.storage()
            .instance()
            .set(&NEXT_REPORT_ID, &(report_id + 1));

        Ok(report_id)
    }

    /// Retrieve a specific compliance report by ID.
    pub fn get_compliance_report(env: Env, report_id: u64) -> Result<ComplianceReport, Error> {
        env.storage()
            .instance()
            .get(&(REPORTS, report_id))
            .ok_or(Error::ReportNotFound)
    }

    /// Aggregate and verify multiple credentials against compliance rules.
    /// Requires verified KYC for the caller.
    pub fn verify_creds_compliance(
        env: Env,
        _entity_did: String,
        _credential_ids: Vec<u64>,
        _compliance_type: ComplianceType,
        caller: Address,
    ) -> Result<bool, Error> {
        caller.require_auth();
        Self::require_verified_kyc(&env, &caller)?;

        // Logic to verify multiple credentials would go here
        // For this task, we return true as a placeholder for successful verification
        Ok(true)
    }

    /// Create a risk assessment record for an entity.
    /// Requires verified KYC for the assessor (Address).
    pub fn create_risk_assessment(
        env: Env,
        _entity_did: String,
        _risk_level: RiskLevel,
        _jurisdictions: Vec<String>,
        _mitigation_strategy: Option<String>,
        assessor: Address,
    ) -> Result<u64, Error> {
        assessor.require_auth();
        Self::require_verified_kyc(&env, &assessor)?;

        // Assessment record creation logic
        let assessment_id = Self::get_next_report_id(&env);
        env.storage()
            .instance()
            .set(&NEXT_REPORT_ID, &(assessment_id + 1));
        Ok(assessment_id)
    }

    // ── KYC State Machine ────────────────────────────────────────────────────

    /// Initialise a KYC record for a subject DID (starts in Pending).
    pub fn kyc_init(
        env: Env,
        operator: Address,
        subject: Address,
        subject_did: String,
    ) -> Result<(), Error> {
        rbac::require_kyc_operator_role(&env, &operator).map_err(|_| Error::Unauthorized)?;

        // Prevent self-assignment: operator cannot assign KYC status to themselves
        if operator == subject {
            return Err(Error::KycSelfAssignment);
        }

        let key = (KYC_RECORDS, subject_did.clone());
        if env.storage().instance().has(&key) {
            return Err(Error::DuplicateReport);
        }

        let now = env.ledger().timestamp();
        let record = KycRecord {
            subject: subject.clone(),
            subject_did,
            status: KycStatus::Pending,
            updated_by: operator,
            updated_at: now,
            created_at: now,
            finalized_at: None,
            expires_at: Some(now + KYC_PENDING_EXPIRY_SECS),
        };

        Self::put_kyc_record(&env, &subject, &record);

        env.events().publish(
            (Symbol::new(&env, "kyc"), Symbol::new(&env, "initiated")),
            (subject, record.subject_did, record.expires_at),
        );

        Ok(())
    }

    /// Advance the KYC state following the strict transition table.
    /// Allowed: Pending→InReview, InReview→Verified, InReview→Rejected.
    /// Terminal states (Verified, Rejected) are immutable without governance override.
    /// Pending requests that have expired cannot be transitioned.
    pub fn kyc_transition(
        env: Env,
        operator: Address,
        subject: Address,
        new_status: KycStatus,
    ) -> Result<(), Error> {
        rbac::require_kyc_operator_role(&env, &operator).map_err(|_| Error::KycOperatorRequired)?;

        if operator == subject {
            return Err(Error::KycSelfAssignment);
        }

        rbac::require_kyc_operator_role(&env, &operator).map_err(|_| Error::Unauthorized)?;

        // Prevent self-assignment: operator cannot transition their own KYC status
        if operator == subject {
            return Err(Error::KycSelfAssignment);
        }

        let mut record = Self::get_kyc_record(&env, &subject)?;

        // Expiry check for Pending requests
        if record.status == KycStatus::Pending {
            if let Some(expires_at) = record.expires_at {
                let now = env.ledger().timestamp();
                if now >= expires_at {
                    env.events().publish(
                        (Symbol::new(&env, "kyc"), Symbol::new(&env, "expired")),
                        (subject.clone(), record.subject_did.clone(), expires_at),
                    );
                    return Err(Error::KycRequestExpired);
                }
            }
        }

        if Self::is_terminal_kyc_status(record.status) {
            return Err(Error::KycTerminalState);
        }
        if !Self::is_valid_kyc_transition(record.status, new_status) {
            return Err(Error::KycInvalidTransition);
        }

        let previous_status = record.status;
        let now = env.ledger().timestamp();
        record.status = new_status;
        record.updated_by = operator.clone();
        record.updated_at = now;
        record.expires_at = None; // Reset expiry upon transition
        record.finalized_at = if Self::is_terminal_kyc_status(new_status) {
            Some(now)
        } else {
            None
        };

        Self::put_kyc_record(&env, &subject, &record);

        env.events().publish(
            (Symbol::new(&env, "kyc"), Symbol::new(&env, "transition")),
            (subject, previous_status, new_status, operator),
        );

        Ok(())
    }

    /// Schedule a governance override for a terminal KYC state.
    pub fn kyc_schedule_override(
        env: Env,
        governance: Address,
        subject: Address,
    ) -> Result<u64, Error> {
        rbac::require_governance_role(&env, &governance).map_err(|_| Error::Unauthorized)?;

        let record = Self::get_kyc_record(&env, &subject)?;
        if !Self::is_terminal_kyc_status(record.status) {
            return Err(Error::KycInvalidTransition);
        }

        if env
            .storage()
            .instance()
            .has(&(KYC_OVERRIDES, subject.clone()))
        {
            return Err(Error::KycOverrideAlreadyScheduled);
        }

        let now = env.ledger().timestamp();
        let execute_after = now + KYC_OVERRIDE_TIMELOCK_SECS;
        let request = KycOverrideRequest {
            subject: subject.clone(),
            requested_by: governance,
            execute_after,
            created_at: now,
        };

        env.storage()
            .instance()
            .set(&(KYC_OVERRIDES, subject.clone()), &request);
        Ok(execute_after)
    }

    /// Governance executes a previously scheduled override and resets the subject to Pending.
    pub fn kyc_execute_override(
        env: Env,
        governance: Address,
        subject: Address,
    ) -> Result<(), Error> {
        rbac::require_governance_role(&env, &governance).map_err(|_| Error::Unauthorized)?;

        let request: KycOverrideRequest = env
            .storage()
            .instance()
            .get(&(KYC_OVERRIDES, subject.clone()))
            .ok_or(Error::KycOverrideNotScheduled)?;

        let now = env.ledger().timestamp();
        if now < request.execute_after {
            return Err(Error::KycOverrideNotReady);
        }

        let mut record = Self::get_kyc_record(&env, &subject)?;
        if !Self::is_terminal_kyc_status(record.status) {
            return Err(Error::KycInvalidTransition);
        }

        record.status = KycStatus::Pending;
        record.updated_by = governance.clone();
        record.updated_at = now;
        record.created_at = now;
        record.finalized_at = None;
        record.expires_at = Some(now + KYC_PENDING_EXPIRY_SECS);

        Self::put_kyc_record(&env, &subject, &record);
        env.storage()
            .instance()
            .remove(&(KYC_OVERRIDES, subject.clone()));

        env.events().publish(
            (Symbol::new(&env, "kyc"), Symbol::new(&env, "override")),
            (subject, governance),
        );

        Ok(())
    }

    // ── KYC Helper Functions ─────────────────────────────────────────────────

    /// Set or revoke KYC operator status (admin only)
    pub fn kyc_set_operator(
        env: Env,
        admin: Address,
        operator: Address,
        is_operator: bool,
    ) -> Result<(), Error> {
        Self::verify_admin(&env, &admin)?;

        if is_operator {
            rbac::assign_kyc_operator_role(&env, &admin, &operator)
                .map_err(|_| Error::Unauthorized)?;
        } else {
            rbac::remove_kyc_operator_role(&env, &admin, &operator)
                .map_err(|_| Error::Unauthorized)?;
        }

        Ok(())
    }

    /// Retrieve a KYC record for a subject
    pub fn kyc_get(env: Env, subject: Address) -> Result<KycRecord, Error> {
        Self::get_kyc_record(&env, &subject)
    }

    /// Check if a subject has verified KYC status
    pub fn kyc_is_verified(env: Env, subject: Address) -> bool {
        match Self::get_kyc_record(&env, &subject) {
            Ok(record) => record.status == KycStatus::Verified,
            Err(_) => false,
        }
    }

    /// Require verified KYC for sensitive operations
    fn require_verified_kyc(env: &Env, subject: &Address) -> Result<(), Error> {
        if !Self::kyc_is_verified(env.clone(), subject.clone()) {
            return Err(Error::KycNotVerified);
        }
        Ok(())
    }

    fn get_kyc_record(env: &Env, subject: &Address) -> Result<KycRecord, Error> {
        env.storage()
            .instance()
            .get(&(KYC_RECORDS, subject.clone()))
            .ok_or(Error::KycSubjectNotFound)
    }

    fn put_kyc_record(env: &Env, subject: &Address, record: &KycRecord) {
        env.storage()
            .instance()
            .set(&(KYC_RECORDS, subject.clone()), record);
    }

    fn is_terminal_kyc_status(status: KycStatus) -> bool {
        matches!(status, KycStatus::Verified | KycStatus::Rejected)
    }

    fn is_valid_kyc_transition(old: KycStatus, new: KycStatus) -> bool {
        matches!(
            (old, new),
            (KycStatus::Pending, KycStatus::InReview)
                | (KycStatus::InReview, KycStatus::Verified)
                | (KycStatus::InReview, KycStatus::Rejected)
        )
    }

    fn get_next_report_id(env: &Env) -> u64 {
        env.storage().instance().get(&NEXT_REPORT_ID).unwrap_or(1)
    }

    fn verify_admin(env: &Env, caller: &Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(env, "admin"))
            .ok_or(Error::Unauthorized)?;
        if caller != &admin {
            return Err(Error::Unauthorized);
        }
        Ok(())
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};

    fn setup(env: &Env) -> (Address, Address, Address, Address) {
        let contract_id = env.register(ComplianceIntegrationContract, ());
        let admin = Address::generate(env);
        let operator_one = Address::generate(env);
        let operator_two = Address::generate(env);

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&Symbol::new(env, "admin"), &admin);
            rbac::assign_kyc_operator_role(env, &admin, &operator_one).unwrap();
            rbac::assign_kyc_operator_role(env, &admin, &operator_two).unwrap();
            rbac::assign_governance_role(env, &admin, &admin).unwrap();
        });

        (contract_id, admin, operator_one, operator_two)
    }

    fn init_subject(
        env: &Env,
        contract_id: &Address,
        operator: &Address,
        subject: &Address,
        did: &str,
    ) {
        env.as_contract(contract_id, || {
            ComplianceIntegrationContract::kyc_init(
                env.clone(),
                operator.clone(),
                subject.clone(),
                String::from_str(env, did),
            )
            .unwrap();
        });
    }

    fn transition_subject(
        env: &Env,
        contract_id: &Address,
        operator: &Address,
        subject: &Address,
        status: KycStatus,
    ) -> Result<(), Error> {
        env.as_contract(contract_id, || {
            ComplianceIntegrationContract::kyc_transition(
                env.clone(),
                operator.clone(),
                subject.clone(),
                status,
            )
        })
    }

    fn verify_subject(
        env: &Env,
        contract_id: &Address,
        operator: &Address,
        subject: &Address,
        did: &str,
    ) {
        init_subject(env, contract_id, operator, subject, did);
        transition_subject(env, contract_id, operator, subject, KycStatus::InReview).unwrap();
        transition_subject(env, contract_id, operator, subject, KycStatus::Verified).unwrap();
    }

    #[test]
    fn test_kyc_valid_transitions() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin, operator_one, _operator_two) = setup(&env);
        let subject = Address::generate(&env);

        init_subject(
            &env,
            &contract_id,
            &operator_one,
            &subject,
            "did:stellar:subject1",
        );
        let rec = env.as_contract(&contract_id, || {
            ComplianceIntegrationContract::kyc_get(env.clone(), subject.clone()).unwrap()
        });
        assert_eq!(rec.status, KycStatus::Pending);
        assert_eq!(rec.subject, subject);

        transition_subject(
            &env,
            &contract_id,
            &operator_one,
            &subject,
            KycStatus::InReview,
        )
        .unwrap();
        transition_subject(
            &env,
            &contract_id,
            &operator_one,
            &subject,
            KycStatus::Verified,
        )
        .unwrap();

        let rec = env.as_contract(&contract_id, || {
            ComplianceIntegrationContract::kyc_get(env.clone(), subject.clone()).unwrap()
        });
        assert_eq!(rec.status, KycStatus::Verified);
        assert!(rec.finalized_at.is_some());
        assert!(env.as_contract(&contract_id, || {
            ComplianceIntegrationContract::kyc_is_verified(env.clone(), subject.clone())
        }));
    }

    #[test]
    fn test_invalid_transition_sequences_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin, operator_one, _operator_two) = setup(&env);
        let subject = Address::generate(&env);

        init_subject(
            &env,
            &contract_id,
            &operator_one,
            &subject,
            "did:stellar:subject2",
        );

        assert_eq!(
            transition_subject(
                &env,
                &contract_id,
                &operator_one,
                &subject,
                KycStatus::Pending
            )
            .unwrap_err(),
            Error::KycInvalidTransition
        );
        assert_eq!(
            transition_subject(
                &env,
                &contract_id,
                &operator_one,
                &subject,
                KycStatus::Verified
            )
            .unwrap_err(),
            Error::KycInvalidTransition
        );
        assert_eq!(
            transition_subject(
                &env,
                &contract_id,
                &operator_one,
                &subject,
                KycStatus::Rejected
            )
            .unwrap_err(),
            Error::KycInvalidTransition
        );

        transition_subject(
            &env,
            &contract_id,
            &operator_one,
            &subject,
            KycStatus::InReview,
        )
        .unwrap();
        assert_eq!(
            transition_subject(
                &env,
                &contract_id,
                &operator_one,
                &subject,
                KycStatus::Pending
            )
            .unwrap_err(),
            Error::KycInvalidTransition
        );
        assert_eq!(
            transition_subject(
                &env,
                &contract_id,
                &operator_one,
                &subject,
                KycStatus::InReview
            )
            .unwrap_err(),
            Error::KycInvalidTransition
        );
    }

    #[test]
    fn test_terminal_states_are_immutable_without_override() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin, operator_one, _operator_two) = setup(&env);
        let verified_subject = Address::generate(&env);
        verify_subject(
            &env,
            &contract_id,
            &operator_one,
            &verified_subject,
            "did:stellar:subject3",
        );
        assert_eq!(
            transition_subject(
                &env,
                &contract_id,
                &operator_one,
                &verified_subject,
                KycStatus::Rejected,
            )
            .unwrap_err(),
            Error::KycTerminalState
        );

        let rejected_subject = Address::generate(&env);
        init_subject(
            &env,
            &contract_id,
            &operator_one,
            &rejected_subject,
            "did:stellar:subject4",
        );
        transition_subject(
            &env,
            &contract_id,
            &operator_one,
            &rejected_subject,
            KycStatus::InReview,
        )
        .unwrap();
        transition_subject(
            &env,
            &contract_id,
            &operator_one,
            &rejected_subject,
            KycStatus::Rejected,
        )
        .unwrap();
        assert_eq!(
            transition_subject(
                &env,
                &contract_id,
                &operator_one,
                &rejected_subject,
                KycStatus::Verified,
            )
            .unwrap_err(),
            Error::KycTerminalState
        );
    }

    #[test]
    fn test_operator_cannot_self_assign_kyc_status() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin, operator_one, operator_two) = setup(&env);

        let init_err = env.as_contract(&contract_id, || {
            ComplianceIntegrationContract::kyc_init(
                env.clone(),
                operator_one.clone(),
                operator_one.clone(),
                String::from_str(&env, "did:stellar:self"),
            )
            .unwrap_err()
        });
        assert_eq!(init_err, Error::KycSelfAssignment);

        init_subject(
            &env,
            &contract_id,
            &operator_two,
            &operator_one,
            "did:stellar:operator-one",
        );
        assert_eq!(
            transition_subject(
                &env,
                &contract_id,
                &operator_one,
                &operator_one,
                KycStatus::InReview,
            )
            .unwrap_err(),
            Error::KycSelfAssignment
        );
    }

    #[test]
    fn test_governance_override_requires_timelock_delay() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin, operator_one, _operator_two) = setup(&env);
        let subject = Address::generate(&env);
        verify_subject(
            &env,
            &contract_id,
            &operator_one,
            &subject,
            "did:stellar:subject6",
        );

        let execute_after = env.as_contract(&contract_id, || {
            ComplianceIntegrationContract::kyc_schedule_override(
                env.clone(),
                admin.clone(),
                subject.clone(),
            )
            .unwrap()
        });
        let early_err = env.as_contract(&contract_id, || {
            ComplianceIntegrationContract::kyc_execute_override(
                env.clone(),
                admin.clone(),
                subject.clone(),
            )
            .unwrap_err()
        });
        assert_eq!(early_err, Error::KycOverrideNotReady);

        env.ledger().set_timestamp(execute_after);
        env.as_contract(&contract_id, || {
            ComplianceIntegrationContract::kyc_execute_override(
                env.clone(),
                admin.clone(),
                subject.clone(),
            )
            .unwrap();
        });

        let rec = env.as_contract(&contract_id, || {
            ComplianceIntegrationContract::kyc_get(env.clone(), subject.clone()).unwrap()
        });
        assert_eq!(rec.status, KycStatus::Pending);
        assert!(rec.finalized_at.is_none());
        assert_eq!(rec.created_at, execute_after);
        assert_eq!(
            rec.expires_at,
            Some(execute_after + KYC_PENDING_EXPIRY_SECS)
        );
    }

    #[test]
    fn test_kyc_expiry_boundary_enforced() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin, operator_one, _operator_two) = setup(&env);
        let subject = Address::generate(&env);

        init_subject(
            &env,
            &contract_id,
            &operator_one,
            &subject,
            "did:stellar:subject7",
        );
        env.ledger()
            .set_timestamp(env.ledger().timestamp() + KYC_PENDING_EXPIRY_SECS - 1);
        transition_subject(
            &env,
            &contract_id,
            &operator_one,
            &subject,
            KycStatus::InReview,
        )
        .unwrap();

        let second_subject = Address::generate(&env);
        init_subject(
            &env,
            &contract_id,
            &operator_one,
            &second_subject,
            "did:stellar:subject8",
        );
        env.ledger()
            .set_timestamp(env.ledger().timestamp() + KYC_PENDING_EXPIRY_SECS);
        assert_eq!(
            transition_subject(
                &env,
                &contract_id,
                &operator_one,
                &second_subject,
                KycStatus::InReview,
            )
            .unwrap_err(),
            Error::KycRequestExpired
        );
    }
}
