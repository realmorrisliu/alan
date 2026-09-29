## Why

ADR-0054 retires Alan for macOS, but the repository still ships its Ghostty
submodule, desktop control client and Skill, and unused signing instructions.
The desktop server and its canonical contracts were already removed.

## What Changes

- Remove the Ghostty gitlink and sole `.gitmodules` registration.
- Remove the `alan shell` desktop IPC command, adapter, tests and bundled
  `alan-shell-control` Skill, including package registration and socket names.
- Remove the unused Rust Host executable descriptor field. Keep script-side
  executable names required to safely retire previously installed Host binaries.
- Retire the previously seeded desktop Skill through Package Service during
  boot, preserving live leases and operator-owned packages.
- Remove `.env.example`, whose signing/notarization steps no longer exist.
- Remove historical desktop/daemon/JS-TUI absence guards and their wiring.
  Trim workspace checks to current implicit Host-source safety boundaries.
  Do not add tests whose only assertion is that a deleted feature stays deleted.
- Correct the brand contract's surviving claim that desktop control is current.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `repository-quality-gate`: stop maintaining retired-feature blacklists; retain
  current Rust, Host-source safety, OpenSpec and distribution checks.
- `package-management-contract`: trusted retirement of obsolete preinstalled
  packages reuses the ordinary removal transaction and lease lifecycle.
- `product-brand-identity`: treat desktop `alan shell` syntax as historical;
  the file-native Alan Shell remains supported.

## Impact

No replacement desktop protocol or compatibility command is introduced. Kernel,
AgentFS, terminal rendering, credentials, sandbox adapters and stores are unaffected.
See `design.md` for the audited retention boundaries. Canonical spec sync follows
merge; archived history is unchanged.
