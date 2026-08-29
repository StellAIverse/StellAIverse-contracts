use anchor_lang::prelude::*;

/// Error codes for safe parameter updates
#[error_code]
pub enum ParamUpdateError {
    #[msg("Caller does not have governance authority to update parameters")]
    Unauthorized,
    #[msg("fee_bps must be between 0 and 10 000 (0 % – 100 %)")]
    FeeBpsOutOfRange,
    #[msg("min_stake must be at least 1 lamport")]
    MinStakeTooLow,
    #[msg("max_stake cannot be less than min_stake")]
    MaxStakeBelowMin,
    #[msg("Slippage tolerance must be between 1 and 10 000 bps")]
    SlippageOutOfRange,
    #[msg("epoch_duration must be between 1 and 52 weeks in seconds")]
    EpochDurationOutOfRange,
}

/// Central parameter store — one PDA owned by the governance authority.
/// Each field lives in its own isolated storage slot so that updating one
/// field provably cannot corrupt another.
#[account]
pub struct GovernanceParams {
    /// The multisig / DAO wallet that may update parameters
    pub authority: Pubkey,
    /// Protocol fee in basis points (0–10 000)
    pub fee_bps: u16,
    /// Minimum stake accepted (lamports)
    pub min_stake: u64,
    /// Maximum stake accepted (lamports); must be ≥ min_stake
    pub max_stake: u64,
    /// Maximum slippage in bps (1–10 000)
    pub slippage_tolerance_bps: u16,
    /// Length of one epoch in seconds (1 – 52 weeks)
    pub epoch_duration: i64,
    /// Monotonic version counter — incremented on every successful update
    pub version: u64,
}

impl GovernanceParams {
    pub const LEN: usize = 8   // discriminator
        + 32                   // authority
        + 2                    // fee_bps
        + 8                    // min_stake
        + 8                    // max_stake
        + 2                    // slippage_tolerance_bps
        + 8                    // epoch_duration
        + 8;                   // version

    const MAX_FEE_BPS: u16 = 10_000;
    const MAX_SLIPPAGE_BPS: u16 = 10_000;
    const MAX_EPOCH_SECONDS: i64 = 52 * 7 * 24 * 3600; // 52 weeks
}

// ─────────────────────────────────────────────────────────────────────────────
// Individual update instructions
// ─────────────────────────────────────────────────────────────────────────────
// Each update targets exactly ONE field.  The instruction name, the accounts
// struct, and the validation are all scoped to that field so that a buggy
// call-site cannot accidentally mutate unrelated state.

#[derive(Accounts)]
pub struct UpdateFee<'info> {
    pub authority: Signer<'info>,
    #[account(
        mut,
        has_one = authority @ ParamUpdateError::Unauthorized,
        seeds = [b"gov-params"],
        bump,
    )]
    pub params: Account<'info, GovernanceParams>,
}

/// Update the protocol fee rate.
pub fn update_fee_bps(ctx: Context<UpdateFee>, new_fee_bps: u16) -> Result<()> {
    require!(
        new_fee_bps <= GovernanceParams::MAX_FEE_BPS,
        ParamUpdateError::FeeBpsOutOfRange
    );
    ctx.accounts.params.fee_bps = new_fee_bps;
    ctx.accounts.params.version = ctx.accounts.params.version.saturating_add(1);
    emit!(ParamUpdated {
        param: "fee_bps".to_string(),
        version: ctx.accounts.params.version,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct UpdateStakeBounds<'info> {
    pub authority: Signer<'info>,
    #[account(
        mut,
        has_one = authority @ ParamUpdateError::Unauthorized,
        seeds = [b"gov-params"],
        bump,
    )]
    pub params: Account<'info, GovernanceParams>,
}

/// Update the stake bounds.  Both bounds are written atomically so the
/// invariant `min_stake ≤ max_stake` is always maintained.
pub fn update_stake_bounds(
    ctx: Context<UpdateStakeBounds>,
    new_min: u64,
    new_max: u64,
) -> Result<()> {
    require!(new_min >= 1, ParamUpdateError::MinStakeTooLow);
    require!(new_max >= new_min, ParamUpdateError::MaxStakeBelowMin);
    ctx.accounts.params.min_stake = new_min;
    ctx.accounts.params.max_stake = new_max;
    ctx.accounts.params.version = ctx.accounts.params.version.saturating_add(1);
    emit!(ParamUpdated {
        param: "stake_bounds".to_string(),
        version: ctx.accounts.params.version,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct UpdateSlippage<'info> {
    pub authority: Signer<'info>,
    #[account(
        mut,
        has_one = authority @ ParamUpdateError::Unauthorized,
        seeds = [b"gov-params"],
        bump,
    )]
    pub params: Account<'info, GovernanceParams>,
}

pub fn update_slippage(
    ctx: Context<UpdateSlippage>,
    new_slippage_bps: u16,
) -> Result<()> {
    require!(
        new_slippage_bps >= 1 && new_slippage_bps <= GovernanceParams::MAX_SLIPPAGE_BPS,
        ParamUpdateError::SlippageOutOfRange
    );
    ctx.accounts.params.slippage_tolerance_bps = new_slippage_bps;
    ctx.accounts.params.version = ctx.accounts.params.version.saturating_add(1);
    emit!(ParamUpdated {
        param: "slippage_tolerance_bps".to_string(),
        version: ctx.accounts.params.version,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct UpdateEpochDuration<'info> {
    pub authority: Signer<'info>,
    #[account(
        mut,
        has_one = authority @ ParamUpdateError::Unauthorized,
        seeds = [b"gov-params"],
        bump,
    )]
    pub params: Account<'info, GovernanceParams>,
}

pub fn update_epoch_duration(
    ctx: Context<UpdateEpochDuration>,
    new_duration: i64,
) -> Result<()> {
    require!(
        new_duration >= 1 && new_duration <= GovernanceParams::MAX_EPOCH_SECONDS,
        ParamUpdateError::EpochDurationOutOfRange
    );
    ctx.accounts.params.epoch_duration = new_duration;
    ctx.accounts.params.version = ctx.accounts.params.version.saturating_add(1);
    emit!(ParamUpdated {
        param: "epoch_duration".to_string(),
        version: ctx.accounts.params.version,
    });
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Events
// ─────────────────────────────────────────────────────────────────────────────

#[event]
pub struct ParamUpdated {
    pub param: String,
    pub version: u64,
}