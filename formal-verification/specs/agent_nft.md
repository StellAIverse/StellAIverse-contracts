# Formal Specification: Agent NFT Contract

**Contract:** `contracts/agent-nft/src/lib.rs`  
**Specification Language:** Natural-language invariants with Kani-verifiable proofs in `../kani_proofs/agent_nft_proofs.rs`

---

## Overview

The Agent NFT contract manages the creation, ownership, and lifecycle of AI Agent tokens on the Stellar/Soroban network. Each agent is identified by a unique `u64` ID and has a single owner at any point in time.

---

## Invariants

### INV-ANFT-1: Counter Monotonicity

**Statement:**  
The `agent_counter` (stored under `AGENT_COUNTER_KEY`) never decreases. After every successful mint operation, `agent_counter` is strictly greater than its value before the call.

**Formal expression:**

```
∀ pre, post states:
  post.agent_counter >= pre.agent_counter
```

**Code reference:** `mint_agent_legacy` (line ~412), `batch_mint` (line ~685)  
**Why important:** Ensures agent IDs form a strictly increasing sequence with no reuse.  
**Machine check:** `kani_proofs/agent_nft_proofs.rs::verify_counter_monotonicity`

---

### INV-ANFT-2: Counter Bounds All Agent IDs

**Statement:**  
For every agent that exists in storage, its ID is less than or equal to `agent_counter`.

**Formal expression:**

```
∀ id: u64. agent_exists(id) ⟹ id ≤ agent_counter
```

**Code reference:** Counter is incremented before each agent is persisted.  
**Machine check:** `kani_proofs/agent_nft_proofs.rs::verify_id_within_counter`

---

### INV-ANFT-3: Owner-Only Transfer

**Statement:**  
`transfer_agent` panics / returns `ContractError::NotOwner` unless `from == agent.owner`.

**Formal expression:**

```
∀ call transfer_agent(agent_id, from, to):
  agent.owner ≠ from ⟹ result = Err(NotOwner)
```

**Code reference:** `transfer_agent` lines ~559–560  
**Machine check:** `kani_proofs/agent_nft_proofs.rs::verify_owner_only_transfer`

---

### INV-ANFT-4: Owner-Only Update

**Statement:**  
`update_agent` panics / returns `ContractError::NotOwner` unless the caller is the current agent owner.

**Formal expression:**

```
∀ call update_agent(agent_id, owner, ...):
  agent.owner ≠ owner ⟹ result = Err(NotOwner)
```

**Code reference:** `update_agent` lines ~457–477

---

### INV-ANFT-5: Royalty Fee Bounds

**Statement:**  
The royalty fee stored for any agent never exceeds `MAX_ROYALTY_FEE` (2500 basis points = 25%).

**Formal expression:**

```
∀ agent_id: u64. royalty_exists(agent_id) ⟹ royalty(agent_id).fee ≤ MAX_ROYALTY_FEE
```

**Code reference:** `validate_royalty_fee` helper; `MAX_ROYALTY_FEE` defined in `stellai_lib`  
**Machine check:** `kani_proofs/agent_nft_proofs.rs::verify_royalty_fee_bound`

---

### INV-ANFT-6: No Transfer While Leased

**Statement:**  
`transfer_agent` returns `ContractError::AgentLeased` if the agent's lease flag is `true`.

**Formal expression:**

```
∀ call transfer_agent(agent_id, from, to):
  is_leased(agent_id) = true ⟹ result = Err(AgentLeased)
```

**Code reference:** `transfer_agent` lines ~563–565

---

### INV-ANFT-7: No Duplicate Agent IDs

**Statement:**  
Calling `mint_agent` with an `agent_id` that already exists returns `ContractError::DuplicateAgentId` without mutating state.

**Formal expression:**

```
∀ call mint_agent(agent_id, ...):
  agent_exists(agent_id) ⟹ result = Err(DuplicateAgentId)
```

**Code reference:** `mint_agent` lines ~256–259

---

### INV-ANFT-8: Overflow Safety

**Statement:**  
The `safe_add` helper never returns a value less than either operand (integer overflow is caught).

**Formal expression:**

```
∀ a, b: u64. safe_add(a, b) = Ok(v) ⟹ v = a + b ∧ v ≥ a
```

**Machine check:** `kani_proofs/agent_nft_proofs.rs::verify_safe_add`

---

## Safety Properties (Itemised)

| ID         | Category      | Property                      |
| ---------- | ------------- | ----------------------------- |
| INV-ANFT-1 | Safety        | Counter never decreases       |
| INV-ANFT-2 | Safety        | All IDs within counter bounds |
| INV-ANFT-3 | Authorization | Only owner can transfer       |
| INV-ANFT-4 | Authorization | Only owner can update         |
| INV-ANFT-5 | Conservation  | Royalty ≤ 25% always          |
| INV-ANFT-6 | Safety        | No transfer while leased      |
| INV-ANFT-7 | Safety        | No ID reuse                   |
| INV-ANFT-8 | Safety        | No integer overflow           |

---

## Liveness Properties

- **LIVE-ANFT-1:** Any address that is admin or approved minter can always successfully mint a new agent (assuming counter has not overflowed u64).
- **LIVE-ANFT-2:** An agent that is not leased can always be transferred by its owner.

---

## Out-of-Scope Assumptions

See [`../ASSUMPTIONS.md`](../ASSUMPTIONS.md) for full details. Key exclusions:

- Cross-contract calls to external token contracts are assumed correct
- Soroban ledger storage is assumed tamper-proof
