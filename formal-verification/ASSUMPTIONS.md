# Formal Verification Assumptions and Limitations

This document records all assumptions made in the formal specifications and Kani proof harnesses for the stellAIverse critical contracts. The goal is full transparency: every assumption narrows the scope of what has been formally verified, and must be independently justified.

---

## General Assumptions

### A-GEN-1: Soroban Ledger Storage Is Tamper-Proof

**Assumption:** The Soroban host environment correctly enforces that contract storage cannot be read or written by any party other than the contract itself.

**Justification:** This is a guaranteed property of the Soroban execution environment, upheld by the Stellar Core host. It is outside the scope of smart contract verification.

**Impact:** Our proofs do not model adversarial storage writes from outside the contract.

---

### A-GEN-2: Soroban Ledger Timestamp Is Monotonically Non-Decreasing

**Assumption:** `env.ledger().timestamp()` returns a value that never decreases between successive ledger closes.

**Justification:** Enforced by Stellar Core consensus rules. Soroban ledger time is derived from consensus and is strictly non-decreasing per the Stellar protocol.

**Impact:** Rate limit window calculations and bypass expiry checks rely on this assumption. If violated, a bypass might appear expired when it is not (or vice versa).

---

### A-GEN-3: On-Chain Authorization (`require_auth`) Is Correctly Enforced

**Assumption:** Soroban's `require_auth()` correctly verifies that the authorizing address (account or contract) has signed the current invocation.

**Justification:** This is a host-level guarantee of the Soroban SDK. Our proofs model authorization as succeeding iff the correct address calls `require_auth`.

**Impact:** Proofs of owner-only operations assume the caller identity cannot be forged.

---

### A-GEN-4: Integer Arithmetic Is 64-Bit (No Silent Truncation)

**Assumption:** All arithmetic operations in the Kani harnesses use `u64` and `i128` types matching the corresponding contract types, with no silent truncation.

**Justification:** Rust's type system enforces this at compile time.

---

## Agent NFT Contract Assumptions

### A-NFT-1: Agent Owner Identity Is Stable Within a Transaction

**Assumption:** The owner of an agent does not change between the authorization check and the ownership enforcement check within a single invocation.

**Justification:** Soroban contracts are single-threaded and atomic within a transaction. No reentrancy is possible.

---

### A-NFT-2: `AGENT_COUNTER_KEY` Initial Value Is Zero

**Assumption:** On first mint after contract initialization, `agent_counter` starts at 0 (as set in `init_contract`), so the first minted agent receives ID 1.

**Justification:** Enforced by `init_contract` which explicitly stores `0u64`.

---

## Marketplace Contract Assumptions

### A-MKT-1: Token Contract Transfers Exact Requested Amount

**Assumption:** The token contract called via `token::Client::new(&env, &get_payment_token(&env))` correctly transfers exactly the amount requested — no more, no less.

**Justification:** The payment token is assumed to be a compliant Soroban token contract (e.g., XLM or a verified SAC). The fund conservation invariants hold only if this assumption holds.

**Impact if violated:** A malicious or buggy token contract could violate fund conservation. Mitigation: restrict the payment token to audited contracts.

---

### A-MKT-2: Marketplace Fee Basis Points Never Exceed 10000

**Assumption:** `get_current_marketplace_fee` always returns a value in `[0, 10000]`.

**Justification:** The `set_approval_config` and fee transition functions enforce this, but there is no explicit runtime check on the returned value in `buy_agent`. This is documented as a gap.

**Impact:** If `marketplace_fee_bps > 10000`, `seller_amount` would become negative. Recommended: add an assertion in `buy_agent`.

---

### A-MKT-3: Royalty Exists for All Auctioned Agents

**Assumption:** `execute_approved_auction_sale` assumes a royalty entry exists (`.expect("Royalty info not found")`). This means every auctioned agent must have royalty info set.

**Justification:** Enforced off-chain by the marketplace workflow (sellers must set royalty before auctioning). A missing royalty entry causes a panic.

**Impact:** If royalty is not set, the auction settlement panics. This is a known limitation.

---

## Execution Hub Contract Assumptions

### A-HUB-1: Cross-Contract Call to `AgentNFT.get_agent_owner` Returns Correct Data

**Assumption:** The `AgentNFT` contract address stored at initialization is a legitimate, unmodified AgentNFT contract, and `get_agent_owner(agent_id)` returns the current owner.

**Justification:** The `agent_nft` address is set once at initialization and cannot be changed. The owner data is assumed to be non-stale within the same ledger.

**Impact:** A compromised AgentNFT contract could authorize unauthorized executors.

---

### A-HUB-2: Execution Counter Will Not Reach `u64::MAX`

**Assumption:** In practice, the execution counter will never reach `u64::MAX` (18.4 quintillion operations).

**Justification:** At 1 million executions per second, it would take ~584,000 years to exhaust a `u64` counter. The contract protects against this with a `saturating_add` + panic-if-zero check.

---

## Kani-Specific Limitations

### A-KANI-1: Harnesses Model Contract Logic, Not the Full Soroban SDK

**Limitation:** The Kani harnesses in `kani_proofs/` extract and re-implement the arithmetic and control-flow logic from the contracts. They do not import or test the actual Soroban SDK types (`Env`, `Address`, `Symbol`) because those types are `no_std`/WASM-specific and would not compile in the Kani environment.

**Implication:** The harnesses provide mathematical proof of the stated arithmetic invariants. They do NOT verify Soroban-specific behaviors (storage, events, cross-contract calls). For those, the existing Soroban test suite (using `soroban-sdk` test utilities) provides coverage.

---

### A-KANI-2: Bounded Model Checking Does Not Prove Termination

**Limitation:** Kani performs bounded model checking. It proves properties hold for all inputs satisfying the `kani::assume` preconditions, but it does not prove termination (liveness) of the contracts.

**Mitigation:** Liveness properties are documented in the spec files and are argued by inspection (no unbounded loops exist in the critical paths).
