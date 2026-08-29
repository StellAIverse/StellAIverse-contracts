# Property-Based Testing — Specifications & Invariants

## Overview

Property-based tests (PBT) use [proptest](https://github.com/proptest-rs/proptest) to verify
contract invariants across thousands of randomly-generated inputs, catching edge cases that
hand-written unit tests miss.

**Files added:**
- `contracts/agent-nft/src/test.rs` — existing file, already contains PBT via `proptest!`
- `contracts/marketplace/src/prop_tests.rs` — new
- `contracts/execution-hub/src/prop_tests.rs` — new

**Run all PBT:**
```bash
cargo test -p agent-nft -p marketplace -p execution-hub --lib prop_tests
```

---

## Agent NFT — Invariants

| ID    | Invariant | Test |
|-------|-----------|------|
| AN-1  | `agent_counter` equals total successful legacy mints | `prop_agent_counter_always_increases_correctly` |
| AN-2  | Royalty fee > 10 000 bps is always rejected | `prop_royalty_fee_invariant` |
| AN-3  | Only the owner can initiate a transfer; stranger gets `NotOwner` | `prop_transfer_auth_invariant` |

---

## Marketplace — Invariants

| ID    | Invariant | Test |
|-------|-----------|------|
| MP-1  | Every stored listing has `price > 0` | `prop_listing_price_always_positive` |
| MP-1b | Zero or negative price is rejected at creation | `prop_zero_or_negative_price_rejected` |
| MP-2  | Valid royalty fee (0–2500 bps) is accepted and stored exactly | `prop_valid_royalty_fee_accepted` |
| MP-2b | Royalty fee > 2500 bps is always rejected | `prop_royalty_fee_above_max_rejected` |
| MP-3  | Platform fee ≤ 5000 bps is accepted and stored exactly | `prop_platform_fee_within_bounds` |
| MP-3b | Platform fee > 5000 bps is always rejected | `prop_platform_fee_above_max_rejected` |
| MP-4  | Listing counter is strictly monotonically increasing | `prop_listing_counter_monotonically_increases` |
| MP-5  | A cancelled listing is never active | `prop_cancelled_listing_is_inactive` |
| MP-6  | Stored royalty fee is always ≤ 10 000 bps (never exceeds 100%) | `prop_stored_royalty_never_exceeds_10000` |

---

## Execution Hub — Invariants

| ID    | Invariant | Test |
|-------|-----------|------|
| EH-1  | `execution_id` is strictly monotonically increasing | `prop_execution_id_strictly_increases` |
| EH-2  | Replaying the same nonce is always rejected | `prop_replay_nonce_rejected` |
| EH-3  | A non-owner (with no operator grant) cannot execute | `prop_non_owner_cannot_execute` |
| EH-4  | The 101st execution within the default 60 s window is blocked | `prop_rate_limit_101st_blocked` |
| EH-5  | `get_action_count` equals the number of successful executions | `prop_action_count_matches_executions` |
| EH-6  | An execution receipt is immutable after creation | `prop_receipt_immutable` |

---

## Test Configuration

| Contract      | Cases (default) | Max input size | Slow test flag |
|---------------|-----------------|----------------|----------------|
| agent-nft     | 1 000           | —              | —              |
| marketplace   | 200             | n ≤ 20 listings | —             |
| execution-hub | 100             | n ≤ 10 actions  | `slow-tests` feature for EH-4 full sweep |

The `prop_rate_limit_default_blocks_over_100` test (100 calls × N cases) is gated behind
`#[cfg_attr(not(feature = "slow-tests"), ignore)]` to keep CI under 5 minutes.

---

## CI Integration

PBT runs as a dedicated step in `.github/workflows/ci.yml`:

```yaml
- name: Property-Based Tests
  run: cargo test -p agent-nft -p marketplace -p execution-hub --lib prop_tests
  timeout-minutes: 10
```

Failures are surfaced as CI failures with proptest's minimal-failing-input shrinking output,
making root-cause analysis straightforward.

---

## Adding New Properties

1. Add a `proptest! { }` block to the relevant `prop_tests.rs`.
2. Document the invariant in this file under the appropriate contract section.
3. Keep per-case work O(1) or O(small-n); gate expensive tests with `#[ignore]` or a feature flag.
