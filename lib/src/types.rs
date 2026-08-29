use soroban_sdk::{contracttype, Address, Bytes, Map, String, Symbol, Val, Vec};

/// Oracle data entry
#[derive(Clone, Debug)]
#[contracttype]
pub struct OracleData {
    pub key: Symbol,
    pub value: i128,
    pub timestamp: u64,
    pub provider: Address,
    pub signature: Option<String>,
    pub source: Option<String>,
}

/// Represents an agent's metadata and state
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[contracttype]
pub struct Agent {
    pub id: u64,
    pub owner: Address,
    pub name: String,
    pub model_hash: String,
    pub metadata_cid: String,
    pub capabilities: Vec<String>,
    pub evolution_level: u32,
    pub created_at: u64,
    pub updated_at: u64,
    pub nonce: u64,
    pub escrow_locked: bool,
    pub escrow_holder: Option<Address>,
}

/// Rate limiting window for security protection
#[derive(Clone, Copy)]
#[contracttype]
pub struct RateLimit {
    pub window_seconds: u64,
    pub max_operations: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BehaviorProfile {
    pub agent_id: u64,
    pub operations_per_hour: Vec<u32>, // last 24 hours
    pub avg_execution_cost: i128,
    pub action_type_distribution: Vec<(String, u32)>,
    pub last_updated: u64,
    pub learning_count: u32,
    pub profile_frozen: bool,
}

#[contracttype]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThresholdKeyShare {
    pub agent_id: u64,
    pub share_holder: Address,
    pub share_index: u32,
    pub x_coordinate: u32,
    pub y_coordinate_encrypted: Bytes,
    pub commitment: Bytes,
    pub created_at: u64,
}

#[contracttype]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalStatus {
    Pending,
    Executed,
    Cancelled,
}

#[contracttype]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThresholdProposal {
    pub proposal_id: u64,
    pub agent_id: u64,
    pub action_data: Bytes,
    pub proposer: Address,
    pub threshold_m: u32,
    pub signers: Vec<Address>,
    pub status: ProposalStatus,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum AnomalySeverity {
    Low = 0,
    Medium = 1,
    High = 2,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnomalyScore {
    pub score: i128, // basis points: 300 = 3.00
    pub anomaly_reason: String,
    pub severity: AnomalySeverity,
}

/// Represents a marketplace listing
#[derive(Clone)]
#[contracttype]
pub struct Listing {
    pub listing_id: u64,
    pub asset_id: u64,
    pub asset_type: AssetType,
    pub seller: Address,
    pub price: i128,
    pub listing_type: ListingType,
    pub active: bool,
    pub created_at: u64,
    pub expires_at: u64,
}

/// Listing types supported by the marketplace
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ListingType {
    Sale = 0,
    Lease = 1,
    Auction = 2,
}

/// Represents prediction market outcome
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PredictionOutcome {
    Yes = 0,
    No = 1,
    Invalid = 2,
}

/// Represents a prediction market
#[derive(Clone, Debug)]
#[contracttype]
pub struct PredictionMarket {
    pub market_id: u64,
    pub question: String,
    pub category: String,
    pub end_timestamp: u64,
    pub resolved: bool,
    pub outcome: PredictionOutcome, // Uses PredictionOutcome::Invalid for unresolved markets
    pub total_shares_yes: u128,
    pub total_shares_no: u128,
    pub oracle_address: Address,
    pub created_at: u64,
    pub creator: Address,
}

/// Represents a user's shares in a prediction market
#[derive(Clone, Debug)]
#[contracttype]
pub struct PredictionShares {
    pub shares_id: u64,
    pub market_id: u64,
    pub owner: Address,
    pub shares_yes: u128,
    pub shares_no: u128,
    pub created_at: u64,
    pub updated_at: u64,
    pub escrow_locked: bool,
    pub escrow_holder: Option<Address>,
}

/// Asset types supported by the marketplace
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum AssetType {
    Agent = 0,
    PredictionShares = 1,
}

/// Represents an evolution/upgrade request
#[derive(Clone)]
#[contracttype]
pub struct EvolutionRequest {
    pub request_id: u64,
    pub agent_id: u64,
    pub owner: Address,
    pub stake_amount: i128,
    pub status: EvolutionStatus,
    pub created_at: u64,
    pub completed_at: Option<u64>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[contracttype]
#[repr(u32)]
pub enum EvolutionStatus {
    Pending = 0,
    InProgress = 1,
    Completed = 2,
    Failed = 3,
}

/// Royalty information for marketplace transactions
#[derive(Clone, Debug)]
#[contracttype]
pub struct RoyaltyInfo {
    pub recipient: Address,
    pub fee: u32,
}

/// Individual royalty recipient with share percentage
#[derive(Clone, Debug)]
#[contracttype]
pub struct RoyaltyRecipient {
    pub recipient: Address,
    pub share_bps: u32, // Basis points (0-10000)
    pub role: String,   // "creator", "collaborator", "platform", etc.
}

/// Complex royalty configuration supporting multiple recipients
#[derive(Clone, Debug)]
#[contracttype]
pub struct RoyaltyConfig {
    pub recipients: Vec<RoyaltyRecipient>,
    pub total_bps: u32,        // Total basis points (should equal sum of shares)
    pub min_threshold: i128,   // Minimum sale price to trigger royalties
    pub max_cap: Option<i128>, // Optional maximum royalty amount
}

/// Asset class for royalty configuration
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
#[repr(u32)]
pub enum AssetClass {
    Agent = 0,
    Model = 1,
    Dataset = 2,
    Tool = 3,
    Other = 4,
}

/// Royalty settings per asset class
#[derive(Clone, Debug)]
#[contracttype]
pub struct AssetClassRoyaltySettings {
    pub asset_class: AssetClass,
    pub default_royalty_bps: u32,
    pub min_royalty_bps: u32,
    pub max_royalty_bps: u32,
    pub min_threshold: i128,
}

/// Royalty payment record for audit trail
#[derive(Clone, Debug)]
#[contracttype]
pub struct RoyaltyPaymentRecord {
    pub payment_id: u64,
    pub agent_id: u64,
    pub transaction_id: u64,
    pub sale_price: i128,
    pub total_royalty_paid: i128,
    pub recipients: Vec<(Address, i128, String)>, // (recipient, amount, role)
    pub timestamp: u64,
    pub asset_class: AssetClass,
}

/// A single record in an agent's ownership history (provenance chain)
#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct OwnerRecord {
    pub owner: Address,
    pub acquired_at: u64, // ledger timestamp when this owner acquired the agent
}

/// Wrapper enum so Option<RoyaltyInfo> works inside contracttype structs
#[derive(Clone, Debug)]
#[contracttype]
pub enum OptionalRoyaltyInfo {
    None,
    Some(RoyaltyInfo),
}

/// Wrapper enum so Option<RoyaltyConfig> works inside contracttype structs
#[derive(Clone, Debug)]
#[contracttype]
pub enum OptionalRoyaltyConfig {
    None,
    Some(RoyaltyConfig),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
#[repr(u32)]
pub enum AuctionType {
    English = 0,
    Dutch = 1,
    Sealed = 2,
}

/// Represents a dispute for a marketplace transaction
#[derive(Clone, Debug)]
#[contracttype]
pub struct Dispute {
    pub dispute_id: u64,
    pub listing_id: u64,
    pub asset_type: AssetType,
    pub initiator: Address,
    pub reason: String,
    pub evidence_cid: Option<String>,
    pub status: DisputeStatus,
    pub created_at: u64,
    pub resolved_at: Option<u64>,
}

/// Status of a dispute
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
#[repr(u32)]
pub enum DisputeStatus {
    Open = 0,
    Resolved = 1,
    Rejected = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
#[repr(u32)]
pub enum AuctionStatus {
    Created = 0,
    Active = 1,
    Ended = 2,
    Cancelled = 3,
    Won = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
#[repr(u32)]
pub enum PriceDecay {
    Linear = 0,
    Exponential = 1,
}

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DutchAuctionConfig {
    pub start_price: i128,
    pub reserve_price: i128,
    pub start_time: u64,
    pub end_time: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Auction {
    pub auction_id: u64,
    pub agent_id: u64,
    pub seller: Address,
    pub auction_type: AuctionType,
    pub start_price: i128,
    pub reserve_price: i128,
    pub current_price: i128,
    pub highest_bidder: Option<Address>,
    pub highest_bid: i128,
    pub start_time: u64,
    pub end_time: u64,
    pub min_bid_increment_bps: u32,
    pub status: AuctionStatus,
    pub dutch_config: Option<Bytes>,
    pub sealed_commit_end: Option<u64>,
    pub sealed_reveal_end: Option<u64>,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedCommit {
    pub bidder: Address,
    pub commitment: Bytes,
    pub deposit: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedReveal {
    pub bidder: Address,
    pub amount: i128,
    pub nonce: String,
    pub deposit: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BidRecord {
    pub bidder: Address,
    pub amount: i128,
    pub timestamp: u64,
    /// Amount above the previous highest bid (0 for the first bid).
    pub bid_increment: i128,
    /// 1-based position of this bid in the auction sequence.
    pub sequence: u64,
}

/// Multi-signature approval configuration for high-value sales
#[derive(Clone)]
#[contracttype]
pub struct ApprovalConfig {
    pub threshold: i128,
    pub approvers_required: u32,
    pub total_approvers: u32,
    pub ttl_seconds: u64,
}

/// Approval status for high-value transactions
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[contracttype]
#[repr(u32)]
pub enum ApprovalStatus {
    Pending = 0,
    Approved = 1,
    Rejected = 2,
    Expired = 3,
    Executed = 4,
}

/// Multi-signature approval for high-value agent sales
#[derive(Clone)]
#[contracttype]
pub struct Approval {
    pub approval_id: u64,
    pub listing_id: Option<u64>,
    pub auction_id: Option<u64>,
    pub buyer: Address,
    pub price: i128,
    pub proposed_at: u64,
    pub expires_at: u64,
    pub status: ApprovalStatus,
    pub required_approvals: u32,
    pub approvers: Vec<Address>,
    pub approvals_received: Vec<Address>,
    pub rejections_received: Vec<Address>,
    pub rejection_reasons: Vec<String>,
}

/// Approval history entry for audit trail
#[derive(Clone)]
#[contracttype]
pub struct ApprovalHistory {
    pub approval_id: u64,
    pub action: String,
    pub actor: Address,
    pub timestamp: u64,
    pub reason: Option<String>,
}

pub struct EvolutionAttestation {
    pub request_id: u64,
    pub agent_id: u64,
    pub oracle_provider: Address,
    pub new_model_hash: String,
    pub attestation_data: Bytes,
    pub signature: Bytes,
    pub timestamp: u64,
    pub nonce: u64,
}

/// State of a lease in its lifecycle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[contracttype]
#[repr(u32)]
pub enum LeaseState {
    None = 0,
    Active = 1,
    ExtensionRequested = 2,
    Terminated = 3,
    Renewed = 4,
    Pending = 5,
    Overdue = 6,
    PendingRenewal = 7,
    Expired = 8,
}

/// Frequency of lease payments.
#[derive(Clone, Copy, PartialEq, Eq)]
#[contracttype]
#[repr(u32)]
pub enum PaymentFrequency {
    Daily = 0,
    Weekly = 1,
    Monthly = 2,
    Quarterly = 3,
    Yearly = 4,
}

/// Type of late fee policy.
#[derive(Clone, Copy, PartialEq, Eq)]
#[contracttype]
#[repr(u32)]
pub enum LateFeeType {
    None = 0,
    Fixed = 1,
    Percentage = 2,
    DailyAccumulation = 3,
}

/// Late fee policy configuration.
#[derive(Clone)]
#[contracttype]
pub struct LateFeePolicy {
    pub fee_type: LateFeeType,
    pub value: u128,
}

/// Renewal policy configuration.
#[derive(Clone)]
#[contracttype]
pub struct RenewalPolicy {
    pub auto_renew: bool,
    pub min_notice_period: u64,
    pub max_renewals: u32,
    pub current_renewal_count: u32,
}

/// Delivery channel for lease renewal and payment notifications.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[contracttype]
#[repr(u32)]
pub enum LeaseNotificationChannel {
    Email = 0,
    InApp = 1,
}

/// Transaction status in the two-phase commit protocol
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[contracttype]
#[repr(u32)]
pub enum TransactionStatus {
    Initiated = 0,
    Preparing = 1,
    Prepared = 2,
    Committing = 3,
    Committed = 4,
    RollingBack = 5,
    RolledBack = 6,
    Failed = 7,
    TimedOut = 8,
}

/// Individual step in an atomic transaction
#[derive(Clone)]
#[contracttype]
pub struct TransactionStep {
    pub step_id: u32,
    pub contract: Address,
    pub function: Symbol,
    pub args: Vec<Val>,
    pub depends_on: Option<u32>,
    pub rollback_contract: Option<Address>,
    pub rollback_function: Option<Symbol>,
    pub rollback_args: Option<Vec<Val>>,
    pub executed: bool,
    pub result: Option<String>,
}

/// Full lease record: duration, renewal terms, termination conditions, deposit.
#[derive(Clone, Debug)]
#[contracttype]
pub struct LeaseData {
    pub lease_id: u64,
    pub agent_id: u64,
    pub listing_id: u64,
    pub lessor: Address,
    pub lessee: Address,
    pub start_time: u64,
    pub end_time: u64,
    pub duration_seconds: u64,
    pub deposit_amount: i128,
    pub total_value: i128,
    pub auto_renew: bool,
    pub lessee_consent_for_renewal: bool,
    pub status: LeaseState,
    pub pending_extension_id: Option<u64>,
    // Advanced Lease Management Fields (Flat structure for robustness)
    pub payment_interval: u64,
    pub payment_amount: i128,
    pub next_payment_timestamp: u64,
    pub max_renewals: u32,
    pub current_renewal_count: u32,
    pub termination_penalty_bps: u32,
    pub late_fee_type: u32,
    pub late_fee_value: u128,
    pub outstanding_balance: i128,
    pub accrued_late_fees: i128,
    pub total_paid: i128,
    pub missed_payments: u32,
    pub renewal_notice_period: u64,
    pub last_notification_timestamp: u64,
    pub email_notifications_enabled: bool,
    pub in_app_notifications_enabled: bool,
    pub asset_class: AssetClass,
}

/// A request to extend an active lease by additional duration.
#[derive(Clone, Debug)]
#[contracttype]
pub struct LeaseExtensionRequest {
    pub extension_id: u64,
    pub lease_id: u64,
    pub additional_duration_seconds: u64,
    pub requested_at: u64,
    pub approved: bool,
}

/// Single entry in lease history (for lessee/lessor audit).
#[derive(Clone, Debug)]
#[contracttype]
pub struct LeaseHistoryEntry {
    pub lease_id: u64,
    pub action: String,
    pub actor: Address,
    pub timestamp: u64,
    pub details: Option<String>,
    pub reason: Option<String>,
    pub old_status: LeaseState,
    pub new_status: LeaseState,
}

/// Immutable notification record for off-chain delivery workers and in-app UX.
#[derive(Clone, Debug)]
#[contracttype]
pub struct LeaseNotification {
    pub notification_id: u64,
    pub lease_id: u64,
    pub channel: LeaseNotificationChannel,
    pub recipient: Address,
    pub message: String,
    pub created_at: u64,
    pub scheduled_for: u64,
    pub sent_at: Option<u64>,
}

/// Atomic transaction containing multiple coordinated steps
#[derive(Clone)]
#[contracttype]
pub struct AtomicTransaction {
    pub transaction_id: u64,
    pub initiator: Address,
    pub steps: Vec<TransactionStep>,
    pub status: TransactionStatus,
    pub created_at: u64,
    pub deadline: u64,
    pub prepared_steps: Vec<u32>,
    pub executed_steps: Vec<u32>,
    pub failure_reason: Option<String>,
}

/// Journal entry for transaction recovery and replay
#[derive(Clone)]
#[contracttype]
pub struct TransactionJournalEntry {
    pub transaction_id: u64,
    pub step_id: u32,
    pub action: String,
    pub timestamp: u64,
    pub success: bool,
    pub error_message: Option<String>,
    pub state_snapshot: Option<String>,
}

/// Transaction progress event for monitoring
#[derive(Clone)]
#[contracttype]
pub struct TransactionEvent {
    pub transaction_id: u64,
    pub event_type: String,
    pub step_id: Option<u32>,
    pub timestamp: u64,
    pub details: Option<String>,
}

/// DID Document structure following W3C DID specification
#[derive(Clone, Debug)]
#[contracttype]
pub struct DIDDocument {
    pub did: String,
    pub controller: Address,
    pub verification_methods: Vec<DIDVerificationMethod>,
    pub authentication: Vec<String>,
    pub assertion_method: Vec<String>,
    pub key_agreement: Vec<String>,
    pub capability_invocation: Vec<String>,
    pub capability_delegation: Vec<String>,
    pub service: Vec<DIDService>,
    pub created: u64,
    pub updated: u64,
    pub version_id: u64,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct DIDVerificationMethod {
    pub id: String,
    pub type_: String,
    pub controller: String,
    pub public_key: Bytes,
    pub created: u64,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct DIDService {
    pub id: String,
    pub type_: String,
    pub service_endpoint: String,
    pub created: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[contracttype]
#[repr(u32)]
pub enum DIDStatus {
    Active = 0,
    Suspended = 1,
    Revoked = 2,
}

#[derive(Clone)]
#[contracttype]
pub struct DIDRecord {
    pub document: DIDDocument,
    pub status: DIDStatus,
    pub nonce: u64,
    pub last_activity: u64,
}

/// Verifiable Credential structure following W3C VC specification
#[derive(Clone, Debug)]
#[contracttype]
pub struct VCProof {
    pub type_: String,
    pub created: u64,
    pub proof_purpose: String,
    pub verification_method: String,
    pub challenge: Option<String>,
    pub domain: Option<String>,
    pub jws: Option<String>,
}

/// Wrapper enum so Option<VCProof> works inside contracttype structs
#[derive(Clone, Debug)]
#[contracttype]
#[allow(clippy::large_enum_variant)]
pub enum OptionalVCProof {
    None,
    Some(VCProof),
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct CredentialStatus {
    pub id: String,
    pub type_: String,
    pub status: String,
    pub revoked: bool,
    pub suspended: bool,
    pub revocation_reason: Option<String>,
    pub suspension_reason: Option<String>,
    pub effective_date: u64,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct VerifiableCredential {
    pub id: String,
    pub credential_id: u64,
    pub issuer: Address,
    pub subject: String, // DID of the subject
    pub credential_type: Vec<String>,
    pub credential_schema: String,
    pub credential_status: CredentialStatus,
    pub issuance_date: u64,
    pub expiration_date: Option<u64>,
    pub credential_subject: Map<String, String>,
    pub proof: OptionalVCProof,
    pub non_revoked: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct CredentialSchema {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Address,
    pub fields: Vec<SchemaField>,
    pub created_at: u64,
    pub required_fields: Vec<String>,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct SchemaField {
    pub name: String,
    pub type_: String,
    pub required: bool,
    pub description: Option<String>,
    pub validation: Option<String>,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct SelectiveDisclosure {
    pub disclosure_id: u64,
    pub credential_id: u64,
    pub verifier: Address,
    pub subject: String,
    pub disclosed_fields: Vec<String>,
    pub nonce: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub presentation_hash: String,
    pub verified: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[contracttype]
#[repr(u32)]
pub enum CredentialType {
    KYC = 0,
    AML = 1,
    Accreditation = 2,
    Reputation = 3,
    License = 4,
    Education = 5,
    Employment = 6,
    Certification = 7,
    AgeVerification = 8,
    AddressVerification = 9,
    IdentityVerification = 10,
}

// ── Workflow Orchestration Types ───────────────────────────────────────────

/// Overall status of an execution-hub workflow
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[contracttype]
#[repr(u32)]
pub enum WorkflowStatus {
    /// Workflow has been created but not yet started
    Pending = 0,
    /// One or more steps are currently executing
    Running = 1,
    /// All steps completed successfully
    Completed = 2,
    /// A step failed and rollback succeeded
    RolledBack = 3,
    /// A step failed and rollback itself also failed
    Failed = 4,
    /// Workflow was cancelled before completion
    Cancelled = 5,
}

/// Status of an individual workflow step
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[contracttype]
#[repr(u32)]
pub enum WorkflowStepStatus {
    Pending = 0,
    Executing = 1,
    Completed = 2,
    Failed = 3,
    RolledBack = 4,
    Skipped = 5,
}

/// A single step within a cross-contract workflow
#[derive(Clone, Debug)]
#[contracttype]
pub struct WorkflowStep {
    /// 0-based index of this step in the workflow
    pub step_index: u32,
    /// Human-readable name, e.g. "lock_escrow"
    pub name: String,
    /// Contract address to call
    pub target_contract: Address,
    /// Name of the function to invoke on `target_contract` (stored as String; converted to Symbol at call time)
    pub function_name: String,
    /// Serialised arguments (ABI-encoded by the caller)
    pub encoded_args: Bytes,
    /// If true this step must complete before any later step can run
    pub required: bool,
    /// Maximum number of automatic retries on transient failure (0 = no retry)
    pub max_retries: u32,
    /// How many retries have been attempted so far
    pub retry_count: u32,
    /// Step execution status
    pub status: WorkflowStepStatus,
    /// Serialised result returned by the target contract (may be empty)
    pub result: Option<Bytes>,
    /// Error message when status is Failed
    pub error: Option<String>,
    /// Ledger timestamp when this step was last updated
    pub updated_at: u64,
}

/// Callback registration so external contracts learn when a workflow finishes.
/// The hub always calls `wf_done(workflow_id, status)` on `callback_contract`.
#[derive(Clone, Debug)]
#[contracttype]
pub struct WorkflowCallback {
    /// Contract to notify on workflow completion or failure
    pub callback_contract: Address,
    /// Whether the callback has already been fired
    pub fired: bool,
}

/// Wrapper enum so Option<WorkflowCallback> works inside contracttype structs
#[derive(Clone, Debug)]
#[contracttype]
pub enum OptionalWorkflowCallback {
    None,
    Some(WorkflowCallback),
}

/// A complete workflow instance managed by the execution hub
#[derive(Clone, Debug)]
#[contracttype]
pub struct WorkflowInstance {
    pub workflow_id: u64,
    /// Address that initiated the workflow (e.g. the marketplace contract)
    pub initiator: Address,
    /// Human-readable workflow name
    pub name: String,
    pub steps: Vec<WorkflowStep>,
    pub status: WorkflowStatus,
    /// Index of the step currently being executed (or last attempted)
    pub current_step: u32,
    /// Number of steps that completed successfully
    pub completed_steps: u32,
    pub created_at: u64,
    pub updated_at: u64,
    /// Deadline after which the workflow is considered timed-out
    pub deadline: u64,
    /// Optional metadata tag for off-chain indexing (e.g. listing ID)
    pub context_tag: Option<String>,
    /// Registered callback (fired once on terminal status)
    pub callback: OptionalWorkflowCallback,
    /// Human-readable failure summary
    pub failure_reason: Option<String>,
    /// Number of steps that were rolled back after a failure
    pub rolled_back_steps: u32,
}

/// Compact summary stored in the per-initiator history index
#[derive(Clone, Debug)]
#[contracttype]
pub struct WorkflowSummary {
    pub workflow_id: u64,
    pub name: String,
    pub status: WorkflowStatus,
    pub created_at: u64,
    pub completed_at: Option<u64>,
}

/// Compliance integration types
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

/// Reputation integration types
#[derive(Clone, Debug)]
#[contracttype]
pub struct ReputationScore {
    pub entity_did: String,
    pub overall_score: u32,
    pub category_scores: Map<String, u32>,
    pub review_count: u32,
    pub last_updated: u64,
    pub calculation_method: String,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct ReputationReview {
    pub review_id: u64,
    pub reviewer_did: String,
    pub subject_did: String,
    pub rating: u32, // 1-5 stars
    pub category: String,
    pub comment: Option<String>,
    pub evidence: Vec<String>, // Credential IDs as evidence
    pub created_at: u64,
    pub verified: bool,
}
