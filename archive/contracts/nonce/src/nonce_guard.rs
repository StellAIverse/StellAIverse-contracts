use anchor_lang::prelude::*;

/// Error codes for nonce-based replay protection
#[error_code]
pub enum NonceError {
    #[msg("Nonce has already been used — replay attack detected")]
    NonceAlreadyUsed,
    #[msg("Nonce account does not belong to the signer")]
    NonceOwnerMismatch,
    #[msg("Nonce value must be non-zero")]
    InvalidNonce,
}

/// Per-user nonce registry stored on-chain.
/// Each user gets one `NonceAccount` PDA seeded by their public key.
#[account]
#[derive(Default)]
pub struct NonceAccount {
    /// The wallet that owns this nonce registry
    pub owner: Pubkey,
    /// Monotonically-increasing counter — we reject any nonce ≤ last_nonce
    pub last_nonce: u64,
    /// Flat bitmap of 256 nonces already consumed in the current window
    /// (slots [last_nonce..last_nonce+256]).  Cleared when the window
    /// advances.  This allows *any* ordering inside the window.
    pub bitmap: [u8; 32], // 32 × 8 = 256 bits
}

impl NonceAccount {
    pub const LEN: usize = 8   // discriminator
        + 32                   // owner
        + 8                    // last_nonce
        + 32;                  // bitmap

    const WINDOW: u64 = 256;

    /// Returns true if `nonce` has been consumed.
    fn is_used(&self, nonce: u64) -> bool {
        let offset = nonce.wrapping_sub(self.last_nonce) as usize;
        if offset >= Self::WINDOW as usize {
            return false; // outside current window → not yet consumed
        }
        let byte = offset / 8;
        let bit = offset % 8;
        (self.bitmap[byte] >> bit) & 1 == 1
    }

    /// Mark `nonce` as consumed.  Advances the window if needed.
    fn consume(&mut self, nonce: u64) {
        // If the nonce is ahead of the window, slide it forward.
        if nonce >= self.last_nonce + Self::WINDOW {
            let advance = nonce - self.last_nonce - Self::WINDOW + 1;
            self.last_nonce += advance;
            // Shift the bitmap (clear slots that fell off the back)
            let byte_shift = (advance / 8) as usize;
            if byte_shift >= 32 {
                self.bitmap = [0u8; 32];
            } else {
                self.bitmap.rotate_left(byte_shift);
                for b in &mut self.bitmap[(32 - byte_shift)..] {
                    *b = 0;
                }
                let bit_shift = (advance % 8) as usize;
                if bit_shift > 0 {
                    let mut carry = 0u8;
                    for b in self.bitmap.iter_mut().rev() {
                        let new_carry = *b << (8 - bit_shift);
                        *b = (*b >> bit_shift) | carry;
                        carry = new_carry;
                    }
                }
            }
        }

        let offset = nonce.wrapping_sub(self.last_nonce) as usize;
        let byte = offset / 8;
        let bit = offset % 8;
        self.bitmap[byte] |= 1 << bit;
    }
}

/// Context for any instruction that requires replay protection.
///
/// Usage:
/// ```rust
/// pub fn my_instruction(ctx: Context<MyCtx>, nonce: u64, ...) -> Result<()> {
///     validate_and_consume_nonce(&mut ctx.accounts.nonce_account,
///                                &ctx.accounts.signer, nonce)?;
///     // ... rest of logic
/// }
/// ```
#[derive(Accounts)]
#[instruction(nonce: u64)]
pub struct WithNonce<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        init_if_needed,
        payer  = signer,
        space  = NonceAccount::LEN,
        seeds  = [b"nonce", signer.key().as_ref()],
        bump,
    )]
    pub nonce_account: Account<'info, NonceAccount>,

    pub system_program: Program<'info, System>,
}

/// Standalone helper — call this at the top of any instruction handler.
pub fn validate_and_consume_nonce(
    nonce_account: &mut Account<NonceAccount>,
    signer: &Signer,
    nonce: u64,
) -> Result<()> {
    require!(nonce != 0, NonceError::InvalidNonce);
    require!(
        nonce_account.owner == Pubkey::default()
            || nonce_account.owner == *signer.key,
        NonceError::NonceOwnerMismatch
    );

    // Initialise owner on first use
    if nonce_account.owner == Pubkey::default() {
        nonce_account.owner = *signer.key;
    }

    require!(!nonce_account.is_used(nonce), NonceError::NonceAlreadyUsed);

    nonce_account.consume(nonce);
    Ok(())
}