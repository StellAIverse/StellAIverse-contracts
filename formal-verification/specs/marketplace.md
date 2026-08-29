# Formal Specification: Marketplace Contract

**Contract:** `contracts/marketplace/src/lib.rs`  
**Specification Language:** Natural-language invariants with Kani-verifiable proofs in `../kani_proofs/marketplace_proofs.rs`

---

## Overview

The Marketplace contract enables fixed-price sales, lease listings, and auctions of Agent NFTs. It handles token transfers, royalty distributions, and a multi-signature approval workflow for high-value transactions.

---

## Invariants

### INV-MKT-1: Fund Conservation (No Creation or Destruction)

**Statement:**  
In any sale execution, the total funds leaving the buyer's account exactly equal the total funds entering the system (seller proceeds + marketplace fee + royalty). No tokens are created or destroyed.

**Formal expression (fixed-price sale):**

```
buyer_payment = marketplace_fee + seller_amount
where:
  marketplace_fee = (price × marketplace_fee_bps) / 10000
  seller_amount   = price - marketplace_fee
  buyer_payment   = price
⟹ marketplace_fee + seller_amount = price  ✓
```

**Formal expression (auction with royalty):**

```
buyer_payment = marketplace_fee + royalty + seller_amount
where:
  marketplace_fee = (highest_bid × fee_bps) / 10000
  royalty         = (highest_bid × royalty_fee) / 10000
  seller_amount   = highest_bid - royalty - marketplace_fee
⟹ marketplace_fee + royalty + seller_amount = highest_bid  ✓
```

**Code reference:** `buy_agent` lines ~169–182; `execute_approved_auction_sale` lines ~682–716  
**Machine check:** `kani_proofs/marketplace_proofs.rs::verify_fund_conservation`

---

### INV-MKT-2: Royalty Fee Never Exceeds 25%

**Statement:**  
The royalty fee stored via `set_royalty` is always at most 10000 basis points checked at entry, and the acceptance criteria requires it to never exceed 2500 (25%).

**Formal expression:**

```
∀ call set_royalty(agent_id, creator, recipient, fee):
  fee > 2500 ⟹ panic("Royalty fee exceeds maximum (100%)")
```

> **Note:** The contract currently rejects `fee > 10000`. The invariant we specify here
> uses the stricter business requirement of 25% (2500 bp). The Kani harness verifies
> the arithmetic bound. The CI spec check documents the tighter bound as an assertion.

**Code reference:** `set_royalty` lines ~239–242  
**Machine check:** `kani_proofs/marketplace_proofs.rs::verify_royalty_bound`

---

### INV-MKT-3: Listing Price Always Positive

**Statement:**  
`create_listing` panics if `price <= 0`. No listing with a zero or negative price is ever stored.

**Formal expression:**

```
∀ call create_listing(agent_id, seller, listing_type, price):
  price ≤ 0 ⟹ panic("Price must be positive")

∀ listing in storage: listing.price > 0
```

**Code reference:** `create_listing` lines ~80–82  
**Machine check:** `kani_proofs/marketplace_proofs.rs::verify_price_positive`

---

### INV-MKT-4: Listing Counter Monotonicity

**Statement:**  
The listing counter stored under `LISTING_COUNTER_KEY` never decreases. It is incremented exactly once per successful `create_listing` call.

**Formal expression:**

```
∀ pre, post states (on create_listing success):
  post.listing_counter = pre.listing_counter + 1
```

**Code reference:** `create_listing` lines ~85–114  
**Machine check:** `kani_proofs/marketplace_proofs.rs::verify_listing_counter_monotonicity`

---

### INV-MKT-5: Only Seller Can Cancel Listing

**Statement:**  
`cancel_listing` panics unless the caller is the original seller of the listing.

**Formal expression:**

```
∀ call cancel_listing(listing_id, seller):
  listing.seller ≠ seller ⟹ panic("Unauthorized: only seller can cancel listing")
```

**Code reference:** `cancel_listing` lines ~209–211

---

### INV-MKT-6: Inactive Listing Cannot Be Purchased

**Statement:**  
`buy_agent` panics if the listing's `active` flag is `false`.

**Formal expression:**

```
∀ call buy_agent(listing_id, buyer):
  ¬listing.active ⟹ panic("Listing is not active")
```

**Code reference:** `buy_agent` lines ~155–157

---

### INV-MKT-7: High-Value Sales Require Multi-Sig Approval

**Statement:**  
`buy_agent` blocks direct purchase when `price >= approval_threshold`. Such sales must go through the `propose_sale` → `approve_sale` → `execute_approved_sale` workflow.

**Formal expression:**

```
∀ call buy_agent(listing_id, buyer):
  listing.price ≥ config.threshold ⟹ panic("High-value sale requires multi-signature approval")
```

**Code reference:** `buy_agent` lines ~160–163

---

## Safety Properties (Itemised)

| ID        | Category      | Property                                |
| --------- | ------------- | --------------------------------------- |
| INV-MKT-1 | Conservation  | Funds conserved in all sale paths       |
| INV-MKT-2 | Finance       | Royalty never exceeds 25%               |
| INV-MKT-3 | Validity      | Price always positive                   |
| INV-MKT-4 | Safety        | Listing counter monotonically increases |
| INV-MKT-5 | Authorization | Only seller can cancel                  |
| INV-MKT-6 | Safety        | Inactive listing not purchasable        |
| INV-MKT-7 | Process       | High-value sales gated by multi-sig     |

---

## Liveness Properties

- **LIVE-MKT-1:** A seller can always create a listing with a valid positive price.
- **LIVE-MKT-2:** A buyer can always purchase an active listing below the approval threshold given sufficient token balance.

---

## Out-of-Scope Assumptions

See [`../ASSUMPTIONS.md`](../ASSUMPTIONS.md). Key exclusions:

- The token contract (`token::Client`) is assumed to correctly transfer the exact requested amount
- On-chain time (`env.ledger().timestamp()`) is assumed to be monotonically non-decreasing
