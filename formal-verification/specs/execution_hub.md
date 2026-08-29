# Formal Specification: Execution Hub Contract

**Contract:** `contracts/execution-hub/src/lib.rs`  
**Specification Language:** Natural-language invariants with Kani-verifiable proofs in `../kani_proofs/execution_hub_proofs.rs`

---

## Overview

The Execution Hub manages on-chain execution of AI Agent actions with replay protection (nonces), rate limiting, operator delegation, and immutable execution receipts for off-chain proof.

---

## Invariants

### INV-HUB-1: Rate Limit Enforcement

**Statement:**  
For any agent without an active bypass, the number of successful `execute_action` calls within any time window of `window_seconds` duration never exceeds `operations` (the configured limit).

**Formal expression:**

```
∀ agent_id, time_window [t, t + window_seconds]:
  count_executions(agent_id, t, t + window_seconds) ≤ config(agent_id).operations
  OR has_active_bypass(agent_id)
```

**Code reference:** `check_rate_limit` lines ~695–731  
**Machine check:** `kani_proofs/execution_hub_proofs.rs::verify_rate_limit_enforcement`

---

### INV-HUB-2: Only Authorized Executors Can Execute

**Statement:**  
`execute_action` succeeds only when the executor is either:

1. The current owner of the agent (fetched via cross-contract call to AgentNFT), **or**
2. A registered operator for the agent whose authorization has not yet expired.

Any other caller causes a panic.

**Formal expression:**

```
∀ call execute_action(agent_id, executor, ...):
  executor ≠ owner(agent_id)
  ∧ (operator(agent_id) = None ∨ operator(agent_id).operator ≠ executor ∨ now > operator(agent_id).expires_at)
  ⟹ panic("Unauthorized: executor is not owner or operator")
```

**Code reference:** `execute_action` lines ~260–282

---

### INV-HUB-3: Nonce Strictly Increases Per Agent

**Statement:**  
For any agent, each successful `execute_action` call stores a nonce strictly greater than the previously stored nonce. Replaying a call with an equal or lower nonce is rejected.

**Formal expression:**

```
∀ call execute_action(agent_id, ..., nonce, ...):
  nonce ≤ stored_nonce(agent_id) ⟹ panic("Invalid nonce: replay protection triggered")

∀ pre, post states (on success):
  post.stored_nonce(agent_id) = nonce > pre.stored_nonce(agent_id)
```

**Code reference:** `execute_action` lines ~287–291; `set_action_nonce` line ~617  
**Machine check:** `kani_proofs/execution_hub_proofs.rs::verify_nonce_monotonicity`

---

### INV-HUB-4: Execution Counter Monotonicity

**Statement:**  
The `EXEC_CTR_KEY` counter strictly increases with every successful `execute_action` call and never wraps (overflow causes a panic).

**Formal expression:**

```
∀ pre, post states (on execute_action success):
  post.execution_counter = pre.execution_counter + 1
  ∧ post.execution_counter > pre.execution_counter
```

**Code reference:** `next_execution_id` lines ~120–128  
**Machine check:** `kani_proofs/execution_hub_proofs.rs::verify_execution_counter_monotonicity`

---

### INV-HUB-5: Execution Receipts Are Immutable

**Statement:**  
Once an execution receipt is stored for a given `execution_id`, it is never overwritten.  
`store_execution_receipt` is called exactly once per execution ID.

**Formal expression:**

```
∀ execution_id:
  receipt_stored(execution_id) ⟹
    no subsequent call overwrites receipt(execution_id)
```

**Code reference:** `store_execution_receipt` lines ~660–692  
**Why important:** Immutable receipts are the on-chain proof of execution for off-chain verification.

---

### INV-HUB-6: Rate Limit Config Always Positive

**Statement:**  
`set_global_rate_limit` and `set_agent_rate_limit` both panic if `ops == 0` or `window_secs == 0`. No zero-limit config is ever stored.

**Formal expression:**

```
∀ call set_global_rate_limit(admin, ops, window_secs):
  ops = 0 ∨ window_secs = 0 ⟹ panic

∀ stored RateLimitConfig c:
  c.operations > 0 ∧ c.window_seconds > 0
```

**Code reference:** `validate_rate_limit_config` lines ~552–558

---

### INV-HUB-7: Bypass Requires Future Expiry

**Statement:**  
`set_rate_limit_bypass` panics if `valid_until <= now`. A bypass can only be set for a timestamp strictly in the future.

**Formal expression:**

```
∀ call set_rate_limit_bypass(admin, agent_id, reason, valid_until):
  valid_until ≤ now ⟹ panic("valid_until must be in the future")
```

**Code reference:** `set_rate_limit_bypass` lines ~509–512

---

### INV-HUB-8: Admin-Only Privileged Operations

**Statement:**  
The following functions panic unless the caller is the current admin:

- `set_global_rate_limit`
- `set_agent_rate_limit`
- `reset_agent_rate_limit`
- `set_rate_limit_bypass`
- `clear_rate_limit_bypass`
- `transfer_admin`

**Formal expression:**

```
∀ admin-only call f(caller, ...):
  caller ≠ admin ⟹ panic("Unauthorized: caller is not admin")
```

**Code reference:** `verify_admin` lines ~544–549

---

## Safety Properties (Itemised)

| ID        | Category          | Property                                  |
| --------- | ----------------- | ----------------------------------------- |
| INV-HUB-1 | Rate Control      | Executions capped per time window         |
| INV-HUB-2 | Authorization     | Only owner/operator can execute           |
| INV-HUB-3 | Replay Protection | Nonce strictly increases                  |
| INV-HUB-4 | Safety            | Execution counter monotonically increases |
| INV-HUB-5 | Integrity         | Receipts immutable after creation         |
| INV-HUB-6 | Validity          | Rate limit config always positive         |
| INV-HUB-7 | Validity          | Bypass only set for future timestamps     |
| INV-HUB-8 | Authorization     | Admin-only operations gated               |

---

## Liveness Properties

- **LIVE-HUB-1:** An authorized executor can always successfully call `execute_action` if within rate limit bounds.
- **LIVE-HUB-2:** An admin can always update rate limit configurations.

---

## Out-of-Scope Assumptions

See [`../ASSUMPTIONS.md`](../ASSUMPTIONS.md). Key exclusions:

- The cross-contract call to `AgentNFT.get_agent_owner` is assumed to return the correct current owner
- Soroban's ledger timestamp is assumed to be monotonically non-decreasing
