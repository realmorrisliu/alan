## Context

Alan for macOS has current authorities for all affected areas: the
content-container workspace manifest restores shell workspaces, the temporary
control plane publishes live state, canonical app bundles are channel-scoped,
and the signed helper owns Managed User creation and PTY supervision. The code
nevertheless retains readers and migration branches for earlier local formats
and identities:

- Application Support `shell-state-*.json`, `restorePrevious`, old
  `ShellStateSnapshot` decoding, and the historical `AlanNative` support root;
- terminal-only manifests and `quick_terminal` compatibility;
- stable installer handling for lowercase `alan.app` and links targeting it;
- sudoers-based Managed User ownership, readiness, repair, rollback, and
  migration from `sudo_user` profiles.

The sudoers path is not the canonical helper-backed Managed User path. The
helper can create an owned account, persist its ownership marker, and supervise
the target user's PTY without passwordless sudo. Sudoers inspection exists to
recognize and remove the retired path, so it belongs to the bounded cleanup and
then disappears from steady state.

`clean-canonical-spec-debt` is a prerequisite. This change may proceed in
either order relative to `remove-residual-compatibility-shims`; both complete
before `finish-namespace-native-engine-boundary` begins.

## Goals / Non-Goals

**Goals:**

- Provide one explicit, operator-run inventory and cleanup boundary before the
  hard cut.
- Remove every steady-state reader, migrator, alias, and state variant for the
  named local formats and identities.
- Make the current content-container manifest the only restart authority.
- Delete redundant persistent shell-state files while retaining current live
  UI and IPC projections.
- Make signed-helper ownership markers and helper-supervised PTYs the sole
  Managed User authority and execution path.
- Prove current builds do not inspect or mutate obsolete paths.

**Non-Goals:**

- Design where Alan OS or Alan for macOS persistence will live long term.
- Design Alan for macOS attachment to Alan OS.
- Delete manually authored operator-owned `sudo_user` or `sudo_root` Terminal
  Profiles.
- Delete Unix accounts or home directories as part of ordinary cleanup.
- Preserve an upgrade path for machines that skip the one-time cleanup.
- Add a permanent `alan cleanup-legacy` command or background maintenance service.

## Decisions

### 1. Cleanup is a bounded operator step, not a shipped compatibility feature

The implementation change will include an explicit pre-cut runbook in its
OpenSpec artifacts. An operator runs the inventory against machines in scope
while the current signed helper can still verify and remove Alan-owned legacy
sudoers state. The runbook records only sanitized paths, ownership
classification, action, and result.

Unprivileged deletion is allowed only for recognized Alan-owned paths and
channel-owned links/bundles. Unrecognized files, non-Alan-owned symlinks,
accounts, and home directories are reported and left untouched. Privileged
sudoers deletion requires exact path/content or existing helper verification
and explicit administrator authorization.

No cleanup executable, reader, or migration branch remains in the final merged
tree. The archived OpenSpec change is the historical record of the one-time
operation.

Alternative considered: ship a permanent cleanup command for late upgrades.
Rejected because it would keep retired identities and formats in current code
and expand a one-owner development migration into a product contract.

### 2. Workspace manifest is the only durable restore authority

Alan for macOS will accept only the current content-container manifest schema.
Unsupported or malformed manifests follow the existing corrupt-evidence path
and produce a current default manifest; they are not decoded through a legacy
type. Terminal-only manifest structs, conversion FFI, and `quick_terminal`
fields are deleted.

`ShellStateSnapshot` remains the in-memory model used by Swift UI and adapters.
The temporary control-plane `state.json` remains a live IPC mirror for
`alan shell`. Neither is read as restart state. The Application Support
`shell-state-*.json` file, `ShellStatePersistenceStore`, `restorePrevious`, and
associated writer scheduling are deleted as redundant persistence.

Alternative considered: retain the current-format `shell-state-*.json` as a
diagnostic snapshot. Rejected because the control plane already publishes a
current state mirror and a second durable copy invites restore ambiguity.

### 3. Current installers manage only current channel-owned artifacts

Stable install and direct CLI-link repair know `Alan.app`; dev install knows
`Alan Dev.app`. They neither search for nor delete lowercase `alan.app`, and
they do not repair links based on that retired destination. The pre-cut runbook
may remove a verified obsolete bundle or link once; normal install behavior
does not retain that knowledge.

Alternative considered: keep deletion as harmless hygiene. Rejected because
path recognition is a compatibility contract and can delete an unrelated
lowercase bundle on case-sensitive storage.

### 4. Helper-backed Managed Users have no sudoers state

Managed User ownership is proven by the active channel's helper-owned marker,
not by a legacy sudoers file. Readiness is account/home/shell/ownership plus a
helper-managed PTY smoke test. Plans contain typed current account, ownership,
profile, verification, and destructive account/home actions; they do not
render, validate, diagnose, repair, or remove sudoers.

An old managed `sudo_user` profile is not upgraded at runtime. After the
pre-cut cleanup it is simply non-canonical persisted input and is not treated
as a helper-backed Managed User. Manually authored `sudo_user` and `sudo_root`
profiles remain valid operator-managed Terminal Profiles and are never claimed
or rewritten by Managed Users.

### 5. Absence checks cover code, specs, and current fixtures

Focused validation will reject obsolete path constants, legacy manifest and
shell-state types, `restorePrevious`, Managed User sudoers state and plan steps,
and normal-installer lowercase-bundle handling. It excludes immutable archived
changes and the exact bounded cleanup record while the change is active.
Positive tests continue to cover current manifest restore, transient IPC state,
channel-owned installation, helper ownership, and PTY launch.

## Risks / Trade-offs

- [Operator skips cleanup and obsolete files remain on disk] → Current builds
  ignore them; document that post-cut Alan will not discover or remove them.
- [Cleanup deletes user-owned state] → Require exact allowlists, ownership
  classification, dry-run inventory, explicit confirmation, and refusal on
  ambiguity.
- [Privileged residue is stranded] → Run verified helper cleanup before removing
  the helper's legacy operations and record sanitized success/failure evidence.
- [Current IPC state is mistaken for persistence] → Name and test the boundary:
  workspace manifest is durable; in-memory `ShellStateSnapshot` and temporary
  control-plane `state.json` are live projections.
- [Manual sudo profiles are accidentally removed] → Scope guards to
  Managed-User-owned compatibility code and retain operator-owned profile tests.
- [Unsupported manifest causes data loss] → Preserve corrupt evidence before
  creating a default; the hard cut intentionally does not convert old data.

## Migration Plan

1. Run a dry inventory for `AlanNative`, lowercase `alan.app`, obsolete shell
   state/manifests, affected links, legacy managed profiles, and verified
   Alan-owned sudoers entries.
2. Review ownership classification and explicitly authorize deletion. Use the
   current signed helper for privileged sudoers cleanup; do not delete accounts
   or homes.
3. Record sanitized cleanup results in change verification.
4. Remove old workspace codecs, persistent shell state, installer handling,
   Managed User sudoers/profile migration, tests, fixtures, and docs.
5. Add current-path positive tests and retired-path absence/rejection tests.
6. Relaunch Alan Dev from a clean current manifest and verify workspace restore,
   `alan shell` state, helper-owned Managed User PTY launch, and channel
   isolation.
7. Run Apple quality gates and strict OpenSpec validation.

Code rollback is a normal revert, but operator-deleted local files are not
recreated. The inventory must therefore be reviewed before cleanup, and any
operator-required backup must be made outside Alan before deletion.

## Open Questions

None.
