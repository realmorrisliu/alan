## 1. Prerequisite And Bounded Cleanup

- [ ] 1.1 Start from main after `clean-canonical-spec-debt` is merged and verify the OpenSpec/current-surface guard is green.
- [ ] 1.2 Run a dry inventory for the historical `AlanNative` Application Support tree, Application Support `shell-state-*.json`, terminal-only or `quick_terminal` manifests, lowercase `alan.app`, links targeting it, Managed-User-owned `sudo_user` profiles, and candidate legacy Alan sudoers entries; record only sanitized paths, ownership classification, and intended action.
- [ ] 1.3 Review the inventory with the operator and explicitly confirm each deletion class; classify ambiguous files, non-Alan-owned links, Unix accounts, and home directories as leave-untouched.
- [ ] 1.4 Remove only confirmed unprivileged Alan-owned legacy paths and links, then rerun the dry inventory to record the sanitized result.
- [ ] 1.5 While the current signed helper still supports exact verification, explicitly authorize and run cleanup of verified Alan-owned legacy sudoers entries; do not delete any unverified entry, account, or home directory.
- [ ] 1.6 Record bounded cleanup success/failure evidence in change verification and confirm no cleanup executable or compatibility reader is intended to remain in the final merged tree.

## 2. Workspace Persistence Hard Cut

- [ ] 2.1 Delete `ShellStatePersistenceStore`, Application Support `shell-state-*.json` writing, `restorePrevious`, restored-window discovery, legacy `ShellStateSnapshot` decoding, and all production/test call sites.
- [ ] 2.2 Simplify persistence coordination and writers so durable writes cover only the current workspace manifest while temporary control-plane `state.json` and event files remain live IPC projections.
- [ ] 2.3 Delete historical `AlanNative` Application Support fallback directories and migration reads; add absence tests proving current stable/dev channel roots are the only paths inspected.
- [ ] 2.4 Delete terminal-only workspace manifest structs, Swift/Rust upgrade logic, FFI migration operations, `quick_terminal` fields, tolerant decoders, fixtures, and positive migration tests.
- [ ] 2.5 Make current schema/content-contract validation fail closed, preserve unsupported bytes through the corrupt-evidence path, and create a current default manifest without fallback restoration.
- [ ] 2.6 Update focused persistence tests to prove current manifest restore, in-memory `ShellStateSnapshot`, temporary control-plane state, corrupt evidence, and no persistent shell-state file.

## 3. Installer And Link Hard Cut

- [ ] 3.1 Remove `ALAN_LEGACY_APP_BUNDLE_NAME`, lowercase-bundle process detection, deletion, and associated stable-channel tests from install scripts and channel descriptors.
- [ ] 3.2 Remove direct command-line-link repair behavior that recognizes lowercase `alan.app` as an Alan-owned destination while preserving canonical `Alan.app` and `Alan Dev.app` channel handling.
- [ ] 3.3 Add focused tests proving stable and dev installers manage only current channel-owned bundles/links and leave an unrelated lowercase path untouched.

## 4. Managed User Sudoers Hard Cut

- [ ] 4.1 Remove Managed User sudoers state, rule rendering, syntax validation, non-interactive sudo verification, verification steps, and legacy-sudoers ownership evidence from Swift models and Rust shell core.
- [ ] 4.2 Remove the corresponding shell-core FFI payloads, adapters, error/status variants, Settings summaries, and fixtures; regenerate or update bindings through the existing constrained facade workflow.
- [ ] 4.3 Remove helper XPC legacy sudoers fields, path verification, ownership inference, readiness status, cleanup plan step, apply/rollback operation, and diagnostics from app and helper implementations.
- [ ] 4.4 Make current Managed User readiness depend only on account/home/shell, active-channel ownership marker, helper diagnosis, and helper-managed PTY smoke verification.
- [ ] 4.5 Make current provisioning/repair/rollback plans contain only current helper account, home, hidden-login, ownership-marker, verification, profile, and separately confirmed destructive account/home actions.
- [ ] 4.6 Make new Managed User profiles use `managed_user`; remove runtime migration of Managed-User-owned `sudo_user` profiles while preserving manually authored operator-owned `sudo_user` and `sudo_root` profiles.
- [ ] 4.7 Remove legacy-sudoers rows, statuses, copy, and cleanup actions from Settings and update current helper-owned readiness/repair/conflict presentation.

## 5. Absence Guards And Focused Tests

- [ ] 5.1 Add a current-surface guard for `AlanNative` fallback reads, persistent shell-state/`restorePrevious`, terminal-only/`quick_terminal` codecs, lowercase installer handling, and Managed User sudoers compatibility; exclude immutable archives and this bounded cleanup record.
- [ ] 5.2 Replace old migration/cleanup oracle tests with current manifest, current channel, helper ownership-marker, PTY, `managed_user`, manual sudo-profile preservation, and retired-surface rejection/absence tests.
- [ ] 5.3 Confirm the final source tree contains no permanent cleanup command, migration reader, legacy-state detector, or fixture that requires old state to be accepted.

## 6. Verification And Delivery

- [ ] 6.1 Run `just shell-core-test`, `just shell-core-ffi-test`, `just apple-shell-focused-tests`, installer/channel tests, the macOS absence guard, `cargo test --workspace`, and `git diff --check`.
- [ ] 6.2 Install and freshly relaunch `Alan Dev.app`; verify current workspace manifest restore, absence of Application Support shell-state files, `alan-dev shell state` through temporary control-plane IPC, helper-owned Managed User PTY launch, and manual sudo-profile preservation.
- [ ] 6.3 Run the repeatable Apple UI smoke for Settings Managed Users and workspace restart, recording evidence that no legacy cleanup row/status or restore fallback appears.
- [ ] 6.4 Update affected canonical Purpose text during spec sync so distribution, workspace, shell-core, helper, and Managed User capabilities no longer claim legacy migration or sudoers ownership.
- [ ] 6.5 Open a narrowly scoped PR and keep the current HEAD under Codex review until every thread is resolved, required CI is green, and a delayed refresh shows no new findings before merge.
- [ ] 6.6 After merge, sync all nine capability deltas into canonical specs and mark the change archive-ready only when main has no steady-state reader or migrator and `remove-residual-compatibility-shims` is merged or independently green.
