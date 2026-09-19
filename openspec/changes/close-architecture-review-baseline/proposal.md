## Why

The September architecture review and the user's decision to retire Alan for
macOS supersede the desktop-host and generation-only planning assumptions.
Close those decisions before starting new implementation from current main.

## What Changes

- Record terminal-host independence, Herdr as the preferred terminal host, and
  retirement of the native desktop product without deleting platform security
  adapters or user data in this documentation-only change.
- Record the mixed Agent Machine direction and replace the obsolete two-Process
  cognitive routing proposal; implementation remains pending.
- Give every pre-existing active change an explicit disposition and entry gate.
- Correct Package Service handle versus mounted-tree paths and existing delta
  validation omissions. Update current guides and ADR status pointers.
- Preserve cancelled work as incomplete history, never as shipped capability.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `documentation-governance`: Record support lifecycle and distinguish accepted
  direction, retained legacy maintenance, parked proposals and delivered code.
- `package-management-contract`: Correct mounted control paths to `/mnt/package`.
- `provider-connection-contract`: Separate surviving platform credential adapter
  ownership from the retained desktop implementation.
- `host-directory-mounts`: Separate surviving native mount adapter ownership
  from the retained desktop presenter and preserve service-mediated grants.
- `alan-app-service-integration`: Make future app integration consumer-neutral;
  the retired desktop client is not a prerequisite for surviving clients.

## Impact

Documentation, OpenSpec and planning metadata only. No provider integration,
runtime rewrite, client source deletion, account cleanup, app uninstall or
release-infrastructure mutation. Canonical runtime contracts remain the current
implementation baseline until their owning follow-up deltas are implemented.
