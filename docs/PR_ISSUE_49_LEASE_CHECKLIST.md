# PR Checklist: Issue #49 – Lease lifecycle management

## Acceptance criteria

| Criterion | Status | Implementation |
|-----------|--------|----------------|
| Create `LeaseData` struct with duration, renewal terms, and termination conditions | ✅ Met | `lib/src/lib.rs`: `LeaseData` has `duration_seconds`, `deposit_amount`, `total_value`, `auto_renew`, `lessee_consent_for_renewal`, `status`, `pending_extension_id`. `LeaseState`: Active, ExtensionRequested, Terminated, Renewed. |
| Implement `request_lease_extension(lease_id, additional_duration)` | ✅ Met | `lib.rs:971` – lessee-only, creates `LeaseExtensionRequest`, sets lease to `ExtensionRequested`, appends history, emits `LeaseExtensionRequested`. |
| Implement `approve_lease_extension(lease_id, extension_id)` by lessor | ✅ Met | `lib.rs:1004` – lessor-only, checks TTL, extends `end_time`, clears pending extension, history + `LeaseExtended` event. |
| Implement `early_termination(lease_id, termination_fee_paid)` with penalties | ✅ Met | `lib.rs:1036` – prorated remaining value, penalty = `remaining_value * early_termination_penalty_bps / 10_000`, deposit refund = deposit − penalty (capped ≥ 0), lessor gets penalty + fee, emits `LeaseTerminated`. |
| Implement automatic renewal if configured (with lessee consent) | ✅ Met | `set_lease_auto_renew` (lessor), `set_lease_renewal_consent` (lessee), `process_lease_renewal(lease_id)` – requires both flags, creates new lease, marks old as Renewed, emits `LeaseRenewed`. |
| Track lease deposit and handle refunds on completion | ✅ Met | Deposit in `LeaseData.deposit_amount`. `settle_lease_expiry` refunds full deposit to lessee. `early_termination` refunds `deposit − penalty` to lessee. |
| Emit events: LeaseExtensionRequested, LeaseExtended, LeaseTerminated, LeaseRenewed | ✅ Met | All four emitted. `LeaseInitiated` and `LeaseExpired` also emitted. |
| Calculate prorated fees for early termination | ✅ Met | `remaining_value = (total_value * remaining_seconds) / total_seconds`, `penalty = remaining_value * early_termination_penalty_bps / 10_000`. |
| Store lease history for lessees and lessors | ✅ Met | `add_lease_history` / `get_lease_history`; entries for initiated, extension_requested, extended, terminated, expired, renewed. |
| Implement `get_active_leases(address) → Vec<Lease>` | ✅ Met | `lib.rs:1174` – returns leases where `status == Active` for lessee and lessor index. |

## Implementation notes (from issue)

| Note | Status |
|------|--------|
| Lease states: Active, ExtensionRequested, Terminated, Renewed | ✅ `LeaseState` enum in `lib`. |
| Penalties: 20% of remaining lease value (configurable) | ✅ `DEFAULT_EARLY_TERMINATION_PENALTY_BPS = 2000`, `set_lease_config(..., early_termination_penalty_bps)`. |
| Automatic renewal: optional, requires explicit renewal agreement | ✅ `auto_renew` (lessor) and `lessee_consent_for_renewal` (lessee) both required in `process_lease_renewal`. |
| Deposit: 10% of lease total value (configurable) | ✅ `DEFAULT_LEASE_DEPOSIT_BPS = 1000`, `set_lease_config(..., deposit_bps)`. |

## Testing requirements

| Requirement | Status | Notes |
|-------------|--------|--------|
| Unit tests for lease calculations | ⚠️ Partial | Config defaults (deposit_bps, early_termination_penalty_bps) tested; no explicit test for penalty/deposit math. |
| Integration tests for full lease lifecycle | ⚠️ Partial | Extension flow (request + approve) and history covered; no test that runs initiate → extend → settle/terminate/renew in one flow. |
| Tests for extension scenarios | ✅ Met | `test_lease_extension_request_and_approve`, `test_lease_history` (extension_requested). |
| Tests for early termination penalties | ❌ Missing | No test calls `early_termination` or asserts penalty/refund amounts. |
| Automatic renewal tests | ❌ Missing | No tests for `set_lease_auto_renew`, `set_lease_renewal_consent`, or `process_lease_renewal`. |
| Edge cases: zero-duration, single-day leases | ⚠️ Partial | `initiate_lease` asserts `duration_seconds > 0` (zero-duration rejected). No single-day or min-duration edge test. |

## Optional improvements (not required for PR)

1. **`get_active_leases` and ExtensionRequested**  
   Currently only `status == Active` is returned. If “active” should include leases pending extension, consider also including `ExtensionRequested` (and document the chosen semantics).

2. **Extra tests (if you want to strengthen the PR)**  
   - Early termination: one test that calls `early_termination` and checks penalty and refund (e.g. with a mock token or storage-only setup similar to `test_lease`).  
   - Auto-renewal: one test that sets `auto_renew` and `lessee_consent_for_renewal`, advances time past `end_time`, calls `process_lease_renewal`, and asserts new lease and `LeaseRenewed` event.

## Summary

- **Acceptance criteria:** All 10 items are met.
- **Implementation notes:** All 4 are met.
- **Testing:** Extension and history are well covered; early termination and automatic renewal have no tests; full lifecycle and edge cases are only partially covered.

You can submit the PR as-is; adding tests for early termination and automatic renewal would make the submission stronger.
