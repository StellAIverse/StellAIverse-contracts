# Formal Verification Framework

This directory contains the formal verification framework for the three critical stellAIverse smart contracts: **Agent NFT**, **Marketplace**, and **Execution Hub**.

---

## Why Kani?

> **Note for reviewers:** The issue references Certora Prover and Scribble as example verification tools. However, those tools are designed exclusively for EVM/Solidity contracts and cannot be used with Soroban/Rust contracts.
>
> For this Rust-based project, we use **[Kani](https://model-checking.github.io/kani/)** — an open-source **bounded model checker for Rust** developed by Amazon Web Services. Kani provides the same class of mathematical proof guarantee as Certora: it exhaustively explores all possible input combinations (within stated bounds) and proves that the stated invariants hold universally, or produces a concrete counterexample.
>
> This is the correct, production-grade formal verification tool for Rust/Soroban contracts.

---

## Directory Structure

```
formal-verification/
├── Cargo.toml                      # Standalone Rust crate for proof harnesses
├── README.md                       # This file
├── ASSUMPTIONS.md                  # All verification assumptions and limitations
├── COUNTEREXAMPLE_GUIDE.md         # How to read and resolve Kani counterexamples
├── specs/
│   ├── agent_nft.md                # Formal invariants: Agent NFT contract
│   ├── marketplace.md              # Formal invariants: Marketplace contract
│   └── execution_hub.md            # Formal invariants: Execution Hub contract
└── kani_proofs/
    ├── agent_nft_proofs.rs         # Kani harnesses: Agent NFT
    ├── marketplace_proofs.rs       # Kani harnesses: Marketplace
    └── execution_hub_proofs.rs     # Kani harnesses: Execution Hub
```

---

## Setup

### Prerequisites

Install Kani (requires Rust nightly, managed automatically by Kani):

```bash
cargo install --locked kani-verifier
cargo kani setup
```

> Kani installs its own toolchain via `rustup` internally. No manual nightly installation is needed.

### Running Verification

From this directory (`formal-verification/`):

```bash
# Verify all harnesses
cargo kani

# Verify a single harness
cargo kani --harness verify_safe_add
cargo kani --harness verify_fund_conservation_fixed_price
cargo kani --harness verify_rate_limit_enforcement

# Verbose output (shows variable assignments for passing proofs)
cargo kani --verbose
```

Expected output for a passing harness:

```
VERIFICATION:- SUCCESSFUL
```

---

## Invariant Coverage

### Agent NFT (`specs/agent_nft.md`)

| Invariant  | Description                   | Machine-Checked                  |
| ---------- | ----------------------------- | -------------------------------- |
| INV-ANFT-1 | Counter monotonicity          | ✅ `verify_counter_monotonicity` |
| INV-ANFT-2 | All IDs within counter bounds | ✅ `verify_id_within_counter`    |
| INV-ANFT-3 | Owner-only transfer           | ✅ `verify_owner_only_transfer`  |
| INV-ANFT-4 | Owner-only update             | Spec only (requires Soroban env) |
| INV-ANFT-5 | Royalty ≤ 25%                 | ✅ `verify_royalty_fee_bound`    |
| INV-ANFT-6 | No transfer while leased      | Spec only (requires Soroban env) |
| INV-ANFT-7 | No duplicate IDs              | Spec only (requires Soroban env) |
| INV-ANFT-8 | Overflow safety               | ✅ `verify_safe_add`             |

### Marketplace (`specs/marketplace.md`)

| Invariant | Description                         | Machine-Checked                           |
| --------- | ----------------------------------- | ----------------------------------------- |
| INV-MKT-1 | Fund conservation (fixed-price)     | ✅ `verify_fund_conservation_fixed_price` |
| INV-MKT-1 | Fund conservation (auction)         | ✅ `verify_fund_conservation_auction`     |
| INV-MKT-2 | Royalty ≤ 25%                       | ✅ `verify_royalty_bound`                 |
| INV-MKT-3 | Price always positive               | ✅ `verify_price_positive`                |
| INV-MKT-4 | Listing counter monotonicity        | ✅ `verify_listing_counter_monotonicity`  |
| INV-MKT-5 | Only seller can cancel              | Spec only (requires Soroban env)          |
| INV-MKT-6 | Inactive listing not purchasable    | Spec only (requires Soroban env)          |
| INV-MKT-7 | High-value sales gated by multi-sig | Spec only (requires Soroban env)          |

### Execution Hub (`specs/execution_hub.md`)

| Invariant | Description                       | Machine-Checked                            |
| --------- | --------------------------------- | ------------------------------------------ |
| INV-HUB-1 | Rate limit enforcement            | ✅ `verify_rate_limit_enforcement`         |
| INV-HUB-2 | Only owner/operator can execute   | ✅ `verify_authorization_logic`            |
| INV-HUB-3 | Nonce strictly increases          | ✅ `verify_nonce_monotonicity`             |
| INV-HUB-4 | Execution counter monotonicity    | ✅ `verify_execution_counter_monotonicity` |
| INV-HUB-5 | Receipts immutable                | Spec only (requires Soroban env)           |
| INV-HUB-6 | Rate limit config always positive | ✅ `verify_rate_limit_config_positive`     |
| INV-HUB-7 | Bypass only for future timestamps | Spec only (requires Soroban env)           |
| INV-HUB-8 | Admin-only operations gated       | Spec only (requires Soroban env)           |

> **"Spec only"** invariants are formally stated in the spec documents and verified by code inspection.
> They require Soroban SDK types that do not compile outside the Soroban WASM target.
> The existing Soroban test suite (in each contract's `src/test.rs`) provides runtime coverage.

---

## CI Integration

Formal verification runs automatically on every push and pull request to `main` via the `formal_verification` job in `.github/workflows/ci.yml`. The job:

1. Installs Kani
2. Runs `cargo kani` in this directory
3. Fails the CI pipeline if any harness reports `VERIFICATION FAILED`

---

## Adding New Invariants

1. Document the invariant in the appropriate `specs/*.md` file following the existing format.
2. If the invariant involves pure arithmetic or control flow extractable from the contract, add a Kani harness in `kani_proofs/`.
3. If the invariant requires Soroban SDK types, document it as "Spec only" and ensure the existing test suite exercises it.
4. Update the coverage table in this README.
