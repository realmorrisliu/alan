## Completion Audit

Change: `provision-macos-terminal-accounts`

Status: implementation-ready, pending merged-spec sync.

### Requirement Evidence

| Requirement area | Evidence |
| --- | --- |
| Managed Terminal Account value model and terminal-only semantics | managed account request/state/plan/result/verification/rollback types in `clients/apple/alan-macos/Models/Shell/ShellValueTypes.swift`; wording coverage in `clients/apple/scripts/test-shell-settings-surface.swift` |
| Read-only local discovery and dry-run planning | discoverer and planner types in `ShellValueTypes.swift`; focused planner coverage in `clients/apple/scripts/test-shell-runtime-metadata.swift` and `clients/apple/scripts/test-terminal-account-dev-dry-run-smoke.swift` |
| Standard non-admin account creation, home/shell handling, and hidden-login default | plan/executor steps in `ShellValueTypes.swift`; dev dry-run smoke checks missing-account and hidden-account planning without touching system state |
| Narrow sudoers rendering and validation | `ManagedTerminalAccountSudoersRule` and validation helpers in `ShellValueTypes.swift`; tests cover one-target scope, no passwordless root grant, no unrelated-user grant, stable file path, and syntax validation behavior |
| Privileged execution boundary | executor protocol/fake executor/authorized script executor types in `ShellValueTypes.swift`; tests cover success, failure, partial apply, cancellation, and redaction |
| Mandatory readiness verification | readiness verifier in `ShellValueTypes.swift`; tests cover account lookup, non-admin checks, home/shell checks, sudoers validation, non-interactive sudo failure, and repair-plan generation |
| Conservative rollback | rollback planning in `ShellValueTypes.swift`; tests cover Alan-owned sudoers/Profile integration rollback and destructive account/home deletion gating |
| Terminal Profile handoff only after ready verification | handoff planner/executor behavior in `ShellValueTypes.swift`; tests cover profile creation/update, failed-provision suppression, and optional Space binding confirmation |
| Settings UI safety | `ShellSettingsSurfaceModel.swift`, `test-shell-settings-surface.swift`, and `test-terminal-account-dev-dry-run-smoke.swift` cover terminal-account wording, preview, confirmation/cancellation, ready/repair/rollback state, and redaction |
| Stable-channel state isolation in dev smoke | `clients/apple/scripts/test-terminal-account-dev-dry-run-smoke.sh` verifies dev-only profile store behavior and no stable-channel profile store creation |

### Verification Gates

Required focused gates for acceptance:

- `bash clients/apple/scripts/test-shell-runtime-metadata.sh`
- `bash clients/apple/scripts/test-shell-settings-surface.sh`
- `bash clients/apple/scripts/test-terminal-account-dev-dry-run-smoke.sh`
- `bash clients/apple/scripts/check-shell-contracts.sh`
- `just apple-shell-focused-tests`
- `openspec validate provision-macos-terminal-accounts --strict`
- `git diff --check`

### Remaining Work

Task 7.7 remains open by design: accepted spec deltas can only be synced into
`openspec/specs/` after the implementation is merged. Do not archive this change
before that merge and sync happen.

