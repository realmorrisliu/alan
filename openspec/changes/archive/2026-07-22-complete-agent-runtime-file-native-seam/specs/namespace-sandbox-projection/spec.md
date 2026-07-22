## REMOVED Requirements

### Requirement: One mount declaration list projects into two enforcement mechanisms
**Reason**: A composition-owned declaration list is no longer an authority
source shared by the Alan OS namespace and native sandbox.

**Migration**: Host Mount Service owns namespace grant projection; Host adapters
independently map the same explicitly delegated grant handles to native
per-Tool-Process sandbox authority.

### Requirement: The projection preserves crate layering
**Reason**: Making a Process composition root the sole joint projection owner
would require it to retain or reconstruct native Host backing.

**Migration**: Keep namespace ownership in Host Mount Service and native
sandbox projection in Host adapters; neither owner imports the other's backing
model.

### Requirement: Mounts are authorized outside the agent's control
**Reason**: The authorization invariant is now owned by Host Mount Service and
the replacement Host-adapter Tool Process projection contract rather than a
shared declaration-list projection capability.

**Migration**: Delegate service-issued Host Mount handles explicitly at Process
launch and let Host adapters derive only the matching native authority.
