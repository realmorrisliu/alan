## Why

Alan for macOS still carries readers, migrators, installer cleanup, and privileged-helper
branches for local state produced by retired product eras. Keeping those paths alive makes
obsolete formats and identities part of the current product contract even though this project
has chosen a hard cut to the canonical Alan architecture.

## What Changes

- Add an explicit, one-time operator inventory and cleanup procedure for obsolete local state,
  including the historical `AlanNative` Application Support tree, lowercase `alan.app`, retired
  shell manifests and snapshots, and verified Alan-owned legacy sudoers entries.
- Require the cleanup procedure to classify ownership before deletion, refuse to remove
  unrecognized or user-owned files, and route privileged sudoers cleanup through the signed
  helper or another explicitly authorized administrative operation.
- **BREAKING** Remove best-effort migration and fallback reads from the historical `AlanNative`
  support path after the one-time cleanup procedure is available.
- **BREAKING** Remove terminal-only workspace-manifest migration, legacy `quick_terminal`
  tolerance, the persistent Application Support `shell-state-*.json` projection,
  `restorePrevious`, deprecated shell-state decoding, and restore-from-shell-state compatibility.
- Keep `ShellStateSnapshot` as the in-memory UI model and keep the temporary control-plane
  `state.json` mirror for current `alan shell`/IPC clients; neither is restart restore authority.
- **BREAKING** Remove stable-channel installer detection and deletion of lowercase `alan.app`;
  current installers manage only the canonical channel-owned bundles and links.
- **BREAKING** Remove legacy-sudoers diagnosis, repair, and rollback branches from the steady-state
  privileged-helper API after the one-time cleanup has run; current Managed User state must use
  the canonical helper-owned model.
- **BREAKING** Remove sudoers state, rendering, validation, ownership inference, non-interactive
  sudo verification, and `sudo_user` migration from Managed User provisioning. Helper-owned
  Managed User PTY launch and ownership markers are the only current managed-account path;
  manually authored `sudo_user` Terminal Profiles remain operator-owned and separate.
- Add repository checks proving that the retired paths, formats, identities, and compatibility
  branches cannot return outside archived history and the bounded cleanup artifact.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `product-brand-identity`: replace historical `AlanNative` migration/fallback behavior with a
  hard-cut absence contract after bounded cleanup.
- `alan-app-distribution`: stop current installers from recognizing or deleting the retired
  lowercase app bundle as part of normal install behavior.
- `macos-shell-workspace-persistence`: accept only the current workspace-manifest schema as restore
  authority, remove the persistent shell-state file, and distinguish current in-memory/transient
  control-plane projections from durable restoration.
- `macos-shell-content-containers`: restore ContentInstances only from the current workspace
  manifest and remove historical terminal-only and persistent shell-state conversion semantics.
- `shell-workspace-core-contract`: remove legacy manifest upgrades and `quick_terminal`
  load-tolerant discard semantics from the portable shell core.
- `macos-privileged-helper`: remove steady-state legacy-sudoers state and operations after a
  separately authorized one-time cleanup.
- `macos-terminal-account-provisioning`: remove sudoers-based readiness, ownership, repair, and
  rollback from the current helper-backed Managed User model.
- `macos-terminal-profiles`: stop migrating old managed `sudo_user` profiles and keep manually
  authored sudo profiles separate from helper-backed Managed Users.
- `macos-shell-ui-ux-conformance`: remove sudoers and legacy-cleanup states from the current
  Managed Users UI contract.
- `macos-shell-build-test-contract`: replace sudoers compatibility tests with current helper-owned
  account, ownership-marker, PTY, and absence checks, and replace historical content-state
  migration fixtures with current-schema restore and fail-closed rejection coverage.

## Impact

This affects Apple workspace persistence models and stores, shell-core manifest codecs and FFI
tests, macOS install scripts and command-line link repair, Managed User/helper request and state
types, focused Apple contract tests, and local operator cleanup documentation. The in-memory
`ShellStateSnapshot` model and temporary control-plane `state.json` remain current runtime
surfaces. Existing obsolete local files will not be migrated after the cleanup boundary;
operators must run the bounded cleanup before adopting the hard-cut build if they want Alan-owned
legacy state removed. This change begins after `clean-canonical-spec-debt` and may proceed
independently of `remove-residual-compatibility-shims`; both must complete before
`finish-namespace-native-engine-boundary` begins.
