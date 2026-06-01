## Why

Alan for macOS is becoming a terminal-first workspace for distinct work
identities, but every new terminal currently launches as the GUI user's login
shell. Users who maintain separate Unix accounts for Alan, Univer, personal, or
lab work need a first-class way to bind Spaces to terminal startup identities
without mixing that concern with Alan connection profiles.

## What Changes

- Add machine-local **Terminal Profiles** for terminal startup identity:
  login shell, sudo Unix user, sudo root, and advanced custom command.
- Keep Terminal Profiles separate from provider `connection_profile` and future
  broader identity profiles.
- Store Terminal Profile definitions in Alan for macOS local app support, while
  workspace manifests only store profile references.
- Let Spaces bind to a default Terminal Profile; new tabs inherit the Space
  profile and new splits inherit the current pane profile unless explicitly
  overridden.
- Persist each terminal content's creation-time `terminal_profile_id` for
  restore, diagnostics, and clear user-facing identity.
- Add Settings and sidebar affordances for creating, editing, selecting, and
  identifying Terminal Profiles without turning the shell into a dashboard.
- Preserve safe fallback behavior when a profile is missing or invalid: the app
  remains usable and falls back to the login shell with a visible missing-profile
  state.

## Capabilities

### New Capabilities

- `macos-terminal-profiles`: Defines machine-local Terminal Profile storage,
  profile kinds, resolution, safety boundaries, and separation from Alan
  provider connection profiles.

### Modified Capabilities

- `macos-shell-workspace-persistence`: Persist Space and terminal-content
  profile references in the workspace manifest without embedding machine-local
  profile definitions.
- `macos-shell-workspace-interactions`: Define inheritance and explicit
  override behavior for new Spaces, tabs, and splits.
- `macos-shell-terminal-lifecycle`: Define terminal startup and restore behavior
  when a terminal content has a Terminal Profile reference.
- `macos-shell-ui-ux-conformance`: Add calm Settings and sidebar affordances for
  Terminal Profiles and missing-profile states.
- `macos-shell-build-test-contract`: Require focused verification for profile
  store, manifest compatibility, launch resolution, inheritance, and UI smoke.

## Impact

- Apple client models need Terminal Profile value types, a local profile store,
  and profile references on Space and terminal content restore payloads.
- `AlanShellBootProfile` / `AlanCommandResolution` need profile-aware command
  resolution while preserving the existing login-shell fallback and Ghostty
  surface path.
- Shell mutations, action routing, control-plane DTOs, and App Intents need to
  carry optional terminal-profile overrides for Space, tab, and split creation.
- Settings and sidebar UI need profile management and Space binding controls.
- Focused Apple shell tests need coverage for local profile persistence,
  workspace manifest compatibility, command generation, inheritance rules, and
  dev-channel relaunch restore.
