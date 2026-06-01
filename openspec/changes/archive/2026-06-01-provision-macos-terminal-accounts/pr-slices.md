## PR Slices

Target base: the merged Terminal Profile implementation, or a stacked branch on
top of the final `add-macos-terminal-profiles` PR. This change depends on the
Terminal Profile store and `sudo_user` launch kind.

### 1. Model, Discovery, And Planner

Branch: `macos-terminal-accounts-model-planner`

Includes:

- Managed Terminal Account request/state/result/verification/rollback value
  types.
- Read-only local state discovery for account, shell, home, hidden-account,
  Alan-owned sudoers, and matching Terminal Profile state.
- Dry-run planner for create, repair, already-ready, and rollback paths.
- Identifier validation for accounts, GUI users, sudoers file names, and profile
  ids.

Primary files:

- `clients/apple/alan-macos/Models/Shell/ShellValueTypes.swift`
- `clients/apple/scripts/test-shell-runtime-metadata.swift`

Required verification:

- `bash clients/apple/scripts/test-shell-runtime-metadata.sh`
- `openspec validate provision-macos-terminal-accounts --strict`

### 2. Sudoers Renderer And Privileged Executor Boundary

Branch: `macos-terminal-accounts-sudoers-executor`

Depends on: slice 1.

Includes:

- Deterministic Alan-owned sudoers drop-in rendering under
  `/etc/sudoers.d/alan-terminal-<gui>-to-<target>`.
- Rule scope limiting passwordless sudo to the selected target account, with no
  passwordless root or unrelated target grant.
- `visudo -cf` validation helpers.
- Narrow executor interface for create, repair, hide-account, write/remove
  sudoers, and verification commands.
- AppleScript/user-visible privileged runner path plus fake-executor tests for
  success, failure, partial apply, cancellation, and redaction.

Primary files:

- `clients/apple/alan-macos/Models/Shell/ShellValueTypes.swift`
- `clients/apple/scripts/test-shell-runtime-metadata.swift`

Required verification:

- `bash clients/apple/scripts/test-shell-runtime-metadata.sh`
- `openspec validate provision-macos-terminal-accounts --strict`

### 3. Verification, Repair, Rollback, And Profile Handoff

Branch: `macos-terminal-accounts-verification-handoff`

Depends on: slice 2.

Includes:

- Readiness checks for account lookup, non-admin account type, home, shell,
  sudoers validation, and `sudo -n -iu <target> true`.
- Repair-plan generation for failed account, sudoers, and verification states.
- Conservative rollback of Alan-owned sudoers/Profile integration while gating
  account and home deletion behind separate destructive confirmation.
- Creation or update of a matching `sudo_user` Terminal Profile only after
  successful verification.
- Optional current-Space binding after explicit confirmation.

Primary files:

- `clients/apple/alan-macos/Models/Shell/ShellValueTypes.swift`
- `clients/apple/alan-macos/Models/Shell/ShellStateMutations.swift`
- `clients/apple/scripts/test-shell-runtime-metadata.swift`

Required verification:

- `bash clients/apple/scripts/test-shell-runtime-metadata.sh`
- `openspec validate provision-macos-terminal-accounts --strict`

### 4. Settings UI And Dev Dry-Run Smoke

Branch: `macos-terminal-accounts-settings-smoke`

Depends on: slice 3.

Includes:

- Settings entry points for create, preview, apply, repair, rollback, readiness,
  and Terminal Profile linkage.
- Terminal-account wording that explicitly avoids GUI automatic-login language.
- Redaction of generated passwords, administrator credentials, raw privileged
  command payloads, and full sudoers text.
- Dev-channel dry-run smoke that proves no stable-channel profile store or real
  system account state is touched.
- Focused test aggregator updates and OpenSpec task status.

Primary files:

- `clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift`
- `clients/apple/scripts/test-shell-settings-surface.sh`
- `clients/apple/scripts/test-shell-settings-surface.swift`
- `clients/apple/scripts/test-terminal-account-dev-dry-run-smoke.sh`
- `clients/apple/scripts/test-terminal-account-dev-dry-run-smoke.swift`
- `justfile`
- `openspec/changes/provision-macos-terminal-accounts/tasks.md`

Required verification:

- `bash clients/apple/scripts/test-shell-settings-surface.sh`
- `bash clients/apple/scripts/test-terminal-account-dev-dry-run-smoke.sh`
- `just apple-shell-focused-tests`
- `bash clients/apple/scripts/check-shell-contracts.sh`
- `openspec validate provision-macos-terminal-accounts --strict`
- `git diff --check`

