## 1. Provisioning Model And Planner

- [x] 1.1 Add Managed Terminal Account value types for request, current state, plan step, apply result, verification status, repair status, and rollback scope.
- [x] 1.2 Implement read-only local state discovery for account existence, account type, home directory, shell, hidden-account state, Alan-owned sudoers state, and matching Terminal Profile state.
- [x] 1.3 Implement a dry-run planner for create, repair, already-ready, and rollback scenarios without writing system state.
- [x] 1.4 Add identifier validation for account names, GUI user names, sudoers file names, and Terminal Profile ids.
- [x] 1.5 Add focused planner tests for missing account, existing standard account, existing admin account, partial sudoers state, existing Terminal Profile, and already-ready account states.

## 2. Sudoers Rendering And Validation

- [x] 2.1 Implement deterministic Alan-owned sudoers drop-in rendering for GUI-user to target-user passwordless terminal entry.
- [x] 2.2 Ensure generated sudoers rules do not grant passwordless root access or unrelated target-user access.
- [x] 2.3 Add validation helpers for sudoers syntax checking with `visudo -cf`.
- [x] 2.4 Add tests for rule scope, invalid identifier rejection, stable file paths, syntax validation success, and validation failure.

## 3. Privileged Execution Boundary

- [x] 3.1 Add a narrow executor interface for privileged operations: create account, repair account properties, hide account, write sudoers drop-in, remove Alan-owned sudoers drop-in, and run verification commands.
- [x] 3.2 Implement the first executor path with explicit administrator authorization or user-visible command execution, keeping reusable credentials out of normal UI and logs.
- [x] 3.3 Ensure generated or entered passwords are never stored in workspace manifests, Terminal Profile definitions, shell state, or normal diagnostics.
- [x] 3.4 Add fake-executor tests for successful apply, apply failure, partial apply, cancellation, and redaction.

## 4. Verification, Repair, And Rollback

- [x] 4.1 Implement readiness verification for account lookup, non-admin account type, home directory, shell, sudoers validation, and `sudo -n -iu <target> true`.
- [x] 4.2 Implement repair-plan generation for failed account, sudoers, and verification steps.
- [x] 4.3 Implement conservative rollback for Alan-owned sudoers and Terminal Profile integration.
- [x] 4.4 Gate account deletion and home-directory deletion behind separate explicit destructive confirmation if V1 includes deletion.
- [x] 4.5 Add focused tests for ready, repairable, failed, rollback, and destructive-operation gating states.

## 5. Terminal Profile Handoff

- [x] 5.1 Connect successful Managed Terminal Account verification to creation or update of a matching `sudo_user` Terminal Profile.
- [x] 5.2 Ensure failed or partial provisioning does not create a ready Terminal Profile.
- [x] 5.3 Add optional current-Space binding after successful provisioning with explicit confirmation.
- [x] 5.4 Add tests for profile creation, profile update, failed-provisioning suppression, and Space binding confirmation.

## 6. Settings UI

- [x] 6.1 Add Settings entry points for creating, previewing, applying, repairing, and rolling back Managed Terminal Accounts.
- [x] 6.2 Use terminal-account wording and explicitly avoid GUI automatic-login wording.
- [x] 6.3 Show privileged plan steps, readiness state, repairable state, rollback scope, and Terminal Profile linkage in compact shell-native UI.
- [x] 6.4 Redact generated passwords, administrator credentials, raw privileged command payloads, and full sudoers text from normal Settings rows.
- [x] 6.5 Add focused Settings/model tests for preview, confirmation, cancellation, ready state, repair state, rollback state, and redaction.

## 7. Verification, Review, And Archive Readiness

- [x] 7.1 Run focused provisioning planner, sudoers renderer, executor, verification, Terminal Profile handoff, and Settings tests.
- [x] 7.2 Run `bash clients/apple/scripts/check-shell-contracts.sh`.
- [x] 7.3 Run a dev-channel manual smoke path that provisions or dry-runs a terminal account without touching stable-channel state.
- [x] 7.4 Run `openspec validate provision-macos-terminal-accounts --strict`.
- [x] 7.5 Review the diff for accidental GUI automatic-login behavior, admin-account defaults, password persistence, passwordless root grants, raw sudoers injection, and workspace-manifest leakage.
- [x] 7.6 Prepare implementation PRs in dependency order: model/planner, sudoers/executor, verification/profile handoff, Settings UI.
- [ ] 7.7 After implementation is merged, sync accepted spec deltas into `openspec/specs/` before archiving.
