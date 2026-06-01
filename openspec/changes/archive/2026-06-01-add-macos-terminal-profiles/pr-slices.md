## PR Slices

Target base: latest `origin/main`, or the merged branch that already contains
any required macOS shell restore work. Do not include unrelated local
`stabilize-macos-quick-terminal-peak` commits in these slices.

### 1. Model, Store, And Manifest References

Branch: `macos-terminal-profiles-model-store`

Includes:

- Terminal Profile value types, validation, channel-scoped store, corrupt-store
  fallback, and login-shell default.
- Optional `terminal_profile_id` fields on Space, terminal content, restore
  records, and workspace snapshots.
- Manifest compatibility and missing-profile preservation tests.

Primary files:

- `clients/apple/alan-macos/Models/Shell/ShellValueTypes.swift`
- `clients/apple/alan-macos/Models/Shell/ShellSnapshots.swift`
- `clients/apple/alan-macos/Models/Shell/ShellWorkspaceManifest.swift`
- `clients/apple/scripts/test-shell-runtime-metadata.swift`

Required verification:

- `bash clients/apple/scripts/test-shell-runtime-metadata.sh`
- `openspec validate add-macos-terminal-profiles --strict`

### 2. Launch Resolution And Workspace Interactions

Branch: `macos-terminal-profiles-launch-interactions`

Depends on: slice 1.

Includes:

- Profile-aware terminal boot command resolution through the existing Ghostty
  surface path.
- Structured launch commands for `login_shell`, `sudo_user`, `sudo_root`, and
  `custom_command`.
- Space default profile binding, tab inheritance, split inheritance, explicit
  overrides, and non-retroactive binding changes.
- Control-plane, action-routing, local command executor, and App Intent fields
  for terminal-profile overrides.

Primary files:

- `clients/apple/alan-macos/TerminalHostRuntime.swift`
- `clients/apple/alan-macos/Models/Shell/ShellStateMutations.swift`
- `clients/apple/alan-macos/Models/Shell/ShellAutomationCommand.swift`
- `clients/apple/alan-macos/Models/Shell/ShellAutomationIntents.swift`
- `clients/apple/alan-macos/Models/Shell/ShellControlPlaneDTOs.swift`
- `clients/apple/alan-macos/Controllers/Shell/ShellHostControlCommandHandling.swift`
- `clients/apple/alan-macos/Services/Shell/ShellLocalCommandExecutor.swift`
- `clients/apple/alan-macos/ShellHostController.swift`

Required verification:

- `bash clients/apple/scripts/test-shell-runtime-metadata.sh`
- `bash clients/apple/scripts/check-shell-contracts.sh`
- `openspec validate add-macos-terminal-profiles --strict`

### 3. Settings, Sidebar, And Runtime Metadata

Branch: `macos-terminal-profiles-ui`

Depends on: slice 2.

Includes:

- Terminal Profiles Settings model rows and structured edit/validation
  affordances.
- Compact Space profile binding selector and quiet identity/missing/root/custom
  command hints in shell UI.
- Non-secret profile metadata projection into pane context and published state
  merges.
- Redaction behavior for normal shell chrome.

Primary files:

- `clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift`
- `clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift`
- `clients/apple/alan-macos/Services/Shell/ShellPaneProjectionService.swift`
- `clients/apple/alan-macos/Services/Shell/ShellPublishedStateMerger.swift`
- `clients/apple/scripts/test-shell-settings-surface.sh`
- `clients/apple/scripts/test-shell-settings-surface.swift`

Required verification:

- `bash clients/apple/scripts/test-shell-settings-surface.sh`
- `bash clients/apple/scripts/test-shell-runtime-metadata.sh`
- `openspec validate add-macos-terminal-profiles --strict`

### 4. Smoke And Contract Gate

Branch: `macos-terminal-profiles-smoke-contract`

Depends on: slice 3.

Includes:

- Dev-channel fresh relaunch smoke evidence for persisted profile references
  and missing-profile fallback.
- Focused test aggregator updates and OpenSpec task status.

Primary files:

- `justfile`
- `openspec/changes/add-macos-terminal-profiles/tasks.md`

Required verification:

- Dev-channel fresh relaunch smoke path for profile restore and missing fallback.
- `just apple-shell-focused-tests`
- `bash clients/apple/scripts/check-shell-contracts.sh`
- `openspec validate add-macos-terminal-profiles --strict`
- `git diff --check`

