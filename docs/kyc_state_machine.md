# KYC State Machine

## Roles

- `KYC_OPERATOR`: may initialize KYC records and advance non-terminal states.
- `governance` (`admin` in `compliance-integration`): may grant or revoke `KYC_OPERATOR` and may schedule or execute terminal-state overrides after a timelock.

## Subject Model

KYC records are keyed by account `Address` and carry the linked DID string for auditability.

## State Diagram

```text
Pending --(operator)--> InReview --(operator)--> Verified
                               \
                                --(operator)--> Rejected

Verified --(governance + timelock)--> Pending
Rejected --(governance + timelock)--> Pending
```

## Invariants

- `Pending -> InReview` is the only valid first transition.
- `InReview -> Verified` and `InReview -> Rejected` are the only valid terminal transitions.
- `Verified` and `Rejected` are immutable through the normal operator flow.
- `finalized_at` is set when a record reaches `Verified` or `Rejected`.
- Operators cannot assign or update their own KYC record.
- Pending requests expire after `90 days`.
- Governance overrides require a scheduled request plus a `24 hour` timelock before execution.

## Sensitive Entry Points Guarded By Verified KYC

The contract now uses a shared verified-KYC guard for:

- `generate_compliance_report`
- `update_compliance_report`
- `verify_creds_compliance`
- `add_reputation_review`
- `create_risk_assessment`
