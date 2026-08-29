use soroban_sdk::contracterror;

/// Option market error codes.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum OptionsError {
    // General
    AlreadyInitialized = 1,
    Unauthorized = 2,
    ContractPaused = 3,
    InvalidInput = 4,

    // Option creation
    StrikePriceInvalid = 10,
    ExpirationInPast = 11,
    ExpirationTooFar = 12,
    SeriesAlreadyExists = 13,
    SeriesNotFound = 14,
    OptionAlreadyExists = 15,
    OptionNotFound = 16,
    InvalidOptionType = 17,
    InvalidOptionStyle = 18,
    MaxOpenInterestExceeded = 19,

    // Pricing
    OraclePriceUnavailable = 20,
    InvalidVolatility = 21,
    PricingFailed = 22,
    StalePriceData = 23,
    InvalidPremium = 24,

    // Exercise & Settlement
    OptionExpired = 30,
    OptionNotExpired = 31,
    OptionNotExercisable = 32,
    OptionAlreadyExercised = 33,
    OptionAlreadySettled = 34,
    NotOptionHolder = 35,
    EuropeanOptionCannotBeExercisedEarly = 36,
    OutOfTheMoney = 37,
    InsufficientCollateral = 38,
    SettlementFailed = 39,

    // Trading
    ListingNotFound = 40,
    ListingNotActive = 41,
    InsufficientBalance = 42,
    SlippageExceeded = 43,
    CannotBuyOwnListing = 44,
    ListingExpired = 45,
    ListingSizeMismatch = 46,

    // Collateral & Risk
    InsufficientWriterCollateral = 50,
    PositionLimitExceeded = 51,
    ExposureLimitExceeded = 52,
    CircuitBreakerTriggered = 53,
    WithdrawalRequestNotFound = 54,
    InsufficientApprovals = 55,
    WithdrawalAlreadyExecuted = 56,

    // Oracle
    OracleNotAuthorized = 60,
    VolatilityDataStale = 61,
    PriceDeviationExceeded = 62,

    // Multi-sig
    ApprovalThresholdNotMet = 70,
    AlreadyApproved = 71,
    RequestAlreadyExecuted = 72,

    // Arithmetic
    Overflow = 80,
    DivisionByZero = 81,
}
