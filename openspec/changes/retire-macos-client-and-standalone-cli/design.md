# Design

## Context

The Rust CLI already owns connection, Host attachment, Skills, and shell
commands. `crates/os-host` owns the Unix socket, macOS credential loading,
Host Mount authority, sandbox projection, and dedicated Host executables. The
remaining app distribution path is separate: it assembles an `Alan.app`,
embeds the CLI, signs and notarizes the bundle, publishes Sparkle appcasts, and
uses app-specific install scripts and CI checks. The Apple source and
`alan-shell-core` crates still exist for maintenance, but they are not a
supported product delivery path after ADR-0054.

### Audited consumers

| Surface | Current consumer | Step 1 disposition |
| --- | --- | --- |
| `crates/alan` CLI | Host attach/start, connections, skills, shell commands | Retain and distribute directly |
| `crates/os-host` | Unix socket, Host lifecycle, credentials, Host Mounts, sandbox and stores | Retain; no installer-owned lifecycle |
| `InstallChannel` | CLI/Host names and channel-isolated store roots | Retain; remove app/bundle identity fields |
| `clients/apple`, Ghostty, `alan-shell-core*` | Retained legacy source and FFI maintenance | Keep isolated; audit again before source removal |
| App bundle/Sparkle/appcast/notary/cask scripts | Alan.app release and embedded CLI only | Remove from current build/release surfaces |
| `just` Apple recipes and CI shell-token job | App/UI/release checks only | Remove or replace with CLI/Host checks |
| System/Host Stores and credential paths | `os-host` and provider adapters | Preserve; no cleanup or migration |

## Goals / Non-Goals

**Goals:**

- Make the standalone Rust CLI plus dedicated Alan OS Host binaries the only
  supported local/release distribution boundary.
- Keep Host lifecycle, credentials, Host Mounts, sandboxing, channel stores,
  and direct file-native shell/connection behavior owned by their current
  Rust adapters.
- Remove app-bundle, Sparkle, appcast, notarization, and app-owned lifecycle
  obligations from `just`, CI, release scripts, and current operator docs.
- Leave a small, testable install/archive contract that does not write user
  data or register a launchd product service as part of installation.

**Non-Goals:**

- Deleting `clients/apple`, Ghostty, `alan-shell-core`, or its FFI facade in
  this slice; that requires a separate source-consumer audit.
- Rebuilding Herdr, adding a new shell UI, changing Agent Machine semantics,
  or adding a package manager/executable projection.
- Migrating or deleting installed apps, credentials, System/Host Stores,
  launchd entries, or external release feeds on a user's machine.

## Decisions

### 1. Keep distribution at the Cargo binary boundary

The supported artifact contains `alan`, `alan-os-host`, and
`alan-os-host-dev`; `alan-dev` is a channel alias of the CLI rather than a
second application. The existing sibling-executable lookup remains valid when
these binaries share an install directory. This reuses the current Cargo
workspace and Host attachment path instead of creating an app bundle or a new
runtime manager.

Alternative considered: retain an app wrapper and merely hide it from docs.
Rejected because it keeps two lifecycle and release owners and allows an app
to become an accidental prerequisite again.

### 2. Use one local installer and one archive assembler

`just install` invokes a repository-local CLI installer and `just release`
assembles a target archive. Both commands build the same Cargo binaries and
write only an explicit destination. Existing non-owned destination files cause
an actionable failure; no shell startup file, user store, credential, app
bundle, or launchd registration is changed.

Alternative considered: keep separate stable/dev app installers. Rejected
because channel isolation belongs to `InstallChannel` and Host stores, not to
two GUI products.

### 3. Make the quality gate distribution-aware, not Apple-app-aware

The canonical quality script continues to run formatting, Rust architecture,
absence, and OpenSpec checks. Apple app architecture/design-token checks and
appcast checks are removed from the current gate. A focused standalone
distribution check verifies the required binaries, CLI aliases, archive
manifest, and `--version` startup without starting a Host.

Alternative considered: leave app checks as required but mark them skipped in
CI. Rejected because a skipped app gate would still encode a retired product
obligation and would hide drift.

### 4. Preserve retired source as maintenance-only

The active specs and docs explicitly state that Apple source and shell-core are
not a supported product surface. Their source remains untouched so platform
security, credential, store, and historical maintenance consumers can be
audited in a later, independently reviewable removal. The current change only
removes delivery consumers that can be proven to have no surviving runtime
owner.

Alternative considered: delete all Apple and shell-core trees now. Rejected for
this slice because it would combine source ownership migration, test-contract
retirement, and distribution cleanup into one unreviewable change.

## Risks / Trade-offs

- **A stale app script remains discoverable** → remove it from `just` and CI,
  add a current-surface guard for appcast/bundle commands, and retain only
  historical OpenSpec archives.
- **An installer overwrites an unrelated binary** → refuse non-owned targets
  and require an explicit destination override.
- **Host startup is accidentally coupled to installation** → installer checks
  binary presence and `--version` only; Host lifecycle remains the existing
  `alan host` runtime path.
- **Future source removal assumes delivery retirement proved no consumers** →
  record the retained-source boundary and require a separate consumer audit.

## Migration Plan

1. Add the standalone distribution spec, design, tasks, installer/archive
   scripts, and focused checks.
2. Remove app-only recipes, release scripts, CI jobs, cask/appcast metadata,
   and current docs; replace them with CLI/Host commands.
3. Run the repository quality gate, standalone distribution checks, workspace
   tests, and OpenSpec validation.
4. Merge through the normal ready → Codex review → fix/resolve → merge flow.
   After merge, sync the delta into canonical specs and archive this change.
5. If desired later, create a new source-removal change after auditing every
   Apple/shell-core consumer; rollback of this change is a normal Git revert
   and does not touch user machines.
