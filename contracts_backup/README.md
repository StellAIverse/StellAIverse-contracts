# contracts_backup — LEGACY: Do Not Use

> **Deprecated.** These contracts contain the original pre-refactoring implementations that defined types, errors, and admin logic locally in each contract.

All common code has been extracted to [`lib/`](../lib/src/) (Issue #88):

| Extracted module | Replaces |
|---|---|
| `lib/src/types.rs` | Per-contract `Agent`, `RoyaltyInfo`, `EvolutionStatus` structs |
| `lib/src/admin.rs` | Per-contract `verify_admin`, `get_admin`, `transfer_admin` |
| `lib/src/validation.rs` | Per-contract `validate_metadata`, `validate_capabilities` |
| `lib/src/storage_keys.rs` | Per-contract `const` key definitions |
| `lib/src/errors.rs` | Per-contract `ContractError` enum |

**Active contracts are in [`contracts/`](../contracts/).**  
Do not import from this directory in new code. These files are preserved only for historical reference and diff comparison.
