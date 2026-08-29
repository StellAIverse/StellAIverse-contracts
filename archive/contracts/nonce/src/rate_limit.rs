use anchor_lang::prelude::*;

/// Error codes for rate limiting
#[error_code]
pub enum RateLimitError {
    #[msg("Rate limit exceeded — too many requests in the current window")]
    RateLimitExceeded,
    #[msg("Rate limit account does not belong to the signer")]
    OwnerMismatch,
    #[msg("Max actions per window must be at least 1")]
    InvalidConfig,
}

/// Configurable rate-limit policy.
/// Embed this inside your program's config account or pass as a separate PDA.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default)]
pub struct RateLimitConfig {
    /// Duration of each rolling window in seconds
    pub window_seconds: i64,
    /// Maximum number of allowed actions inside one window
    pub max_actions: u32,
}

impl RateLimitConfig {
    /// Production default: 10 actions per 60 seconds
    pub const DEFAULT: RateLimitConfig = RateLimitConfig {
        window_seconds: 60,
        max_actions: 10,
    };
}

/// Per-user rate-limit state — one PDA per user per operation type.
///
/// Seed pattern: `[b"rate", operation_tag, user_pubkey]`
/// Using distinct `operation_tag` values lets you apply separate limits
/// to different sensitive operations (e.g. b"withdraw" vs b"vote").
#[account]
#[derive(Default)]
pub struct RateLimitState {
    pub owner: Pubkey,
    /// Unix timestamp (seconds) when the current window started
    pub window_start: i64,
    /// Number of actions taken in the current window
    pub action_count: u32,
}

impl RateLimitState {
    pub const LEN: usize = 8   // discriminator
        + 32                   // owner
        + 8                    // window_start
        + 4;                   // action_count

    /// Returns true if one more action is allowed right now.
    /// Deterministic: uses `block_time`, NOT `Clock::get()`, so callers
    /// control the timestamp — making it fully testable.
    pub fn check_and_record(
        &mut self,
        owner: &Pubkey,
        config: &RateLimitConfig,
        block_time: i64,
    ) -> Result<()> {
        require!(config.max_actions >= 1, RateLimitError::InvalidConfig);

        // Initialise owner on first use
        if self.owner == Pubkey::default() {
            self.owner = *owner;
            self.window_start = block_time;
        }

        require!(self.owner == *owner, RateLimitError::OwnerMismatch);

        // Roll the window forward if needed
        if block_time >= self.window_start + config.window_seconds {
            self.window_start = block_time;
            self.action_count = 0;
        }

        require!(
            self.action_count < config.max_actions,
            RateLimitError::RateLimitExceeded
        );

        self.action_count = self.action_count.saturating_add(1);
        Ok(())
    }

    /// Seconds remaining until the current window resets
    pub fn seconds_until_reset(&self, config: &RateLimitConfig, block_time: i64) -> i64 {
        let window_end = self.window_start + config.window_seconds;
        (window_end - block_time).max(0)
    }
}

/// Convenience macro — injects rate-limit enforcement into an instruction.
///
/// ```rust
/// pub fn sensitive_op(ctx: Context<SensitiveOp>) -> Result<()> {
///     enforce_rate_limit!(ctx, rate_limit_state, WITHDRAW_LIMIT);
///     // ... protected logic
/// }
/// ```
#[macro_export]
macro_rules! enforce_rate_limit {
    ($ctx:expr, $state_field:ident, $config:expr) => {{
        let clock = Clock::get()?;
        $ctx.accounts
            .$state_field
            .check_and_record(&$ctx.accounts.signer.key(), &$config, clock.unix_timestamp)?;
    }};
}

/// Example accounts struct for a rate-limited instruction.
/// Duplicate this (with a different `operation_tag` seed literal) per
/// sensitive operation that needs its own independent limit.
#[derive(Accounts)]
pub struct WithRateLimit<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        init_if_needed,
        payer  = signer,
        space  = RateLimitState::LEN,
        seeds  = [b"rate", b"default", signer.key().as_ref()],
        bump,
    )]
    pub rate_limit_state: Account<'info, RateLimitState>,

    pub system_program: Program<'info, System>,
}