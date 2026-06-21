## 1. Prerequisites And Architecture Lock

- [x] 1.1 Confirm `add-macos-alan-owned-pty-runtime` has landed or is available on the implementation branch, including Ghostty external-PTY attachment.
- [x] 1.2 Inventory existing Managed User `osascript`, sudoers, `sudo -n -iu`, Terminal Profile, Settings, and verification call sites that must move to the helper path.
- [x] 1.3 Choose the Apple-supported helper registration approach for the active deployment target and record the stable/dev helper labels, Mach services, bundle identifiers, and data roots.
- [x] 1.4 Define helper request/response DTOs for status, diagnosis, apply, PTY start, PTY terminate, integration removal, denial, and sanitized errors.

## 2. Helper Packaging And IPC

- [x] 2.1 Add the privileged helper target, bundle metadata, launchd service metadata, signing configuration, and stable/dev channel identity generation.
- [x] 2.2 Add app-side helper status, install, update, uninstall, and signature validation services with fake implementations for tests.
- [x] 2.3 Add XPC or equivalent local IPC client/server boundaries with code-signing requirement checks for the matching channel.
- [x] 2.4 Implement helper request validation for account identifiers, fixed `/Users/<account>` home paths, `/bin/zsh` shell allowlist, request capability, and channel matching.
- [x] 2.5 Add sanitized helper logging and error mapping that excludes credentials, terminal transcripts, raw scripts, and full privileged payloads.

## 3. Managed User Helper Operations

- [x] 3.1 Add Alan-managed account ownership evidence and discovery rules so ordinary macOS accounts are reported as `accountNotAlanManaged`.
- [x] 3.2 Move Managed User diagnosis to helper-backed account, home, shell, hidden-login, ownership, legacy sudoers, profile handoff, and PTY-readiness inspection.
- [x] 3.3 Move Managed User create/repair apply to helper-authored declarative plans without accepting raw shell, arbitrary executable, or raw sudoers steps.
- [x] 3.4 Implement verified legacy Alan sudoers cleanup for deterministic Alan-owned paths only, preserving non-Alan sudoers files.
- [x] 3.5 Keep destructive account or home deletion behind a separate explicit confirmation path for Alan-managed accounts only.

## 4. Managed User PTY Provider

- [x] 4.1 Add `managed_user` Terminal Profile launch identity and keep existing `sudo_user` profiles operator managed.
- [x] 4.2 Update managed-user-generated Terminal Profiles to use `managed_user` and migrate legacy Alan-managed `sudo_user` profiles when helper readiness is available.
- [x] 4.3 Add a helper-backed PTY provider to the Alan-owned terminal runtime for `managed_user` launches.
- [x] 4.4 Route helper-backed PTY input, resize, EOF, interrupt, terminate, kill, exit observation, and cleanup through terminal ContentInstance runtime handles.
- [x] 4.5 Ensure Ghostty attaches only as renderer/protocol adapter to the Alan-provided managed-user PTY endpoint.

## 5. Settings And Product States

- [x] 5.1 Add Settings > Terminal helper status states for not installed, outdated, invalid signature, installing/updating, healthy, unavailable, and uninstallable.
- [x] 5.2 Update Managed User rows to show helper-backed states including repairable, ready, account not Alan managed, legacy sudoers present, PTY spawn failed, and destructive confirmation required.
- [x] 5.3 Update create/repair review sheets so privileged plans reference helper operations and legacy cleanup instead of sudoers setup.
- [x] 5.4 Disable ready Space/Profile selection for helper-unready Managed Users while preserving login-shell fallback.
- [x] 5.5 Keep Terminal Profile editor read-only for managed profiles and route repair/remove actions back to Managed Users.

## 6. Remove Old Managed User Runtime Path

- [x] 6.1 Remove `osascript` with administrator privileges as the Managed User executor.
- [x] 6.2 Remove Alan-managed sudoers generation, validation, and `sudo -n -iu <target>` readiness as helper-backed Managed User runtime paths.
- [x] 6.3 Preserve `sudo_user` launch behavior only for manual operator-managed Terminal Profiles.
- [x] 6.4 Add contract checks that fail if Managed User helper-backed execution reintroduces raw sudoers editing, `do shell script ... with administrator privileges`, or sudo-based readiness fallback.

## 7. Verification

- [x] 7.1 Add focused helper unit tests for DTO validation, channel labels, code-signing denial paths, account identifier rejection, home/shell allowlists, and sanitized errors.
- [x] 7.2 Add fake-helper Settings and Managed User tests for status, diagnose, apply, start PTY, terminate PTY, remove integration, and denial states.
- [x] 7.3 Add terminal runtime tests for `managed_user` launch resolution, helper PTY lifecycle, child exit, signal routing, renderer attachment failure, and no sudo fallback.
- [x] 7.4 Add migration tests for legacy Alan sudoers cleanup, legacy managed `sudo_user` profile migration, manual `sudo_user` preservation, and non-Alan account preservation.
- [x] 7.5 Run `openspec validate add-macos-privileged-helper-pty-provider --strict`.
- [x] 7.6 Run focused Apple shell/settings/runtime checks and helper integration smoke where local signing and authorization prerequisites are available.
  - Evidence: shell contract checks, settings surface tests, Debug build, `just install-dev`, dev helper update, and live helper status smoke passed. `univer` account-specific smoke correctly stops at `account_not_alan_managed`, so no managed-user PTY smoke was run without an Alan-managed account prerequisite.

## 8. Review And Archive Readiness

- [x] 8.1 Review the diff for privileged API broadening, raw command execution, credential persistence, transcript logging, manual-account takeover, and sudoers fallback.
- [x] 8.2 Keep implementation PRs ordered so the PTY runtime dependency lands before helper-backed Managed User terminal launch.
- [x] 8.3 After implementation lands, sync accepted delta specs into `openspec/specs/`.
- [x] 8.4 Archive the completed OpenSpec change after synced specs validate.
