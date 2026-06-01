## Completion Audit

Change: `add-macos-terminal-profiles`

Status: implementation-ready, pending merged-spec sync.

### Requirement Evidence

| Requirement area | Evidence |
| --- | --- |
| Local Terminal Profile model, validation, store, fallback, and corrupt-store recovery | `TerminalProfileDefinition`, `TerminalProfileDocument`, `TerminalProfileValidator`, and `TerminalProfileStore` in `clients/apple/alan-macos/Models/Shell/ShellValueTypes.swift`; focused coverage in `clients/apple/scripts/test-shell-runtime-metadata.swift` |
| Structured launch modes for `login_shell`, `sudo_user`, `sudo_root`, and `custom_command` | `TerminalProfileLaunchKind`, launch generation in `TerminalHostRuntime.swift`, and runtime metadata tests in `clients/apple/scripts/test-shell-runtime-metadata.swift` |
| Workspace manifests store only profile references | `terminal_profile_id` fields in `ShellSnapshots.swift` and `ShellWorkspaceManifest.swift`; manifest/reference tests in `clients/apple/scripts/test-shell-runtime-metadata.swift` |
| Profile references survive restore and missing profiles fall back safely | restore and missing-profile tests in `clients/apple/scripts/test-shell-runtime-metadata.swift`; dev-channel relaunch smoke evidence captured by task 6.3 |
| Space binding, tab inheritance, split inheritance, explicit override, and non-retroactive updates | mutation/action changes in `ShellStateMutations.swift`, `ShellAutomationCommand.swift`, `ShellAutomationIntents.swift`, `ShellControlPlaneDTOs.swift`, `ShellHostControlCommandHandling.swift`, `ShellLocalCommandExecutor.swift`, and `ShellHostController.swift`; focused tests in `clients/apple/scripts/test-shell-runtime-metadata.swift` |
| Terminal lifecycle uses existing Ghostty-backed surface path | profile-aware command resolution in `TerminalHostRuntime.swift` without replacing the terminal host path; covered by runtime tests |
| Non-secret profile metadata projection | `ShellPaneProjectionService.swift`, `ShellPublishedStateMerger.swift`, and `ShellContextSnapshot` fields in `ShellValueTypes.swift`; covered by runtime metadata tests |
| Settings and sidebar affordances with redaction | `ShellSettingsSurfaceModel.swift`, `ShellSidebarView.swift`, `test-shell-settings-surface.swift`, and `test-shell-runtime-metadata.swift` |
| Provider connection profiles remain separate | no `connections.toml`, provider credential, or `connection_profile` schema changes are part of this change; task 6.5 diff review recorded |

### Verification Gates

Required focused gates for acceptance:

- `bash clients/apple/scripts/test-shell-runtime-metadata.sh`
- `bash clients/apple/scripts/test-shell-settings-surface.sh`
- `bash clients/apple/scripts/check-shell-contracts.sh`
- `just apple-shell-focused-tests`
- `openspec validate add-macos-terminal-profiles --strict`
- `git diff --check`
- dev-channel fresh relaunch smoke for persisted profile references and
  missing-profile fallback

### Remaining Work

Task 6.7 remains open by design: accepted spec deltas can only be synced into
`openspec/specs/` after the implementation is merged. Do not archive this change
before that merge and sync happen.

