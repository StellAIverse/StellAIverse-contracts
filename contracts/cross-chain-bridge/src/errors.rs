use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum BridgeError {
    // Generic errors
    Unauthorized = 1,
    AlreadyInitialized = 2,
    NotInitialized = 3,
    InvalidArgument = 4,
    BridgePaused = 5,

    // Token related
    TokenNotSupported = 100,
    InsufficientBalance = 101,
    TokenTransferFailed = 102,
    MintFailed = 103,
    BurnFailed = 104,
    LockFailed = 105,
    UnlockFailed = 106,

    // Transaction related
    TransferNotFound = 200,
    InvalidTransferStatus = 201,
    TransferAlreadyProcessed = 202,
    InvalidChainPair = 203,
    InvalidAmount = 204,

    // Nonce related
    NonceAlreadyUsed = 300,
    InvalidNonce = 301,

    // Rate limiting
    RateLimitExceeded = 400,
    DailyLimitExceeded = 401,
    MonthlyLimitExceeded = 402,
    PerTransactionLimitExceeded = 403,
    TransactionBelowMinimum = 404,

    // Validator related
    ValidatorNotFound = 500,
    ValidatorAlreadyExists = 501,
    ValidatorAlreadyRemoved = 502,
    InsufficientValidators = 503,
    DuplicateSignature = 504,
    InvalidSignature = 505,
    InsufficientSignatures = 506,
    SignerNotValidator = 507,

    // Emergency controls
    AlreadyPaused = 600,
    AlreadyUnpaused = 601,

    // Fee related
    FeeCollectionFailed = 700,
    InvalidFeeConfiguration = 701,
}
