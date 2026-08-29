# Role-Based Access Control

The `AccessControl` Soroban contract defines five protocol roles:

- `Admin`: controls the role hierarchy and can grant or revoke every role.
- `Minter`: may be used by token modules for mint operations.
- `Burner`: may be used by token modules for burn operations.
- `Pauser`: may be used by emergency controls.
- `Governance`: may be used by governance-controlled operations.

Every role is administered by `Admin` after initialization. `set_role_admin` supports delegated role administration while preserving the current role administrator as the authority to change that relationship. `RoleGranted`, `RoleRevoked`, and `RoleAdminChanged` events contain the affected role, account, and caller for audit indexing.
