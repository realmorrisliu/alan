## Context

The `add-macos-terminal-profiles` change gives Alan for macOS a clean model for
launching new terminals through machine-local Terminal Profiles. A `sudo_user`
Terminal Profile can run `sudo -iu alan`, but the operating-system pieces still
need to exist:

- a local Unix account such as `alan`
- a home directory such as `/Users/alan`
- an allowed shell such as `/bin/zsh`
- sudo policy that lets the GUI user enter that account without an interactive
  password prompt

The user explicitly does not want macOS GUI automatic login. The desired
experience is terminal-only: opening a Space or tab can enter the target Unix
account automatically through sudo, while the Mac still boots into the normal
GUI user's login/session model.

This change is more sensitive than Terminal Profiles because it modifies local
system accounts and `/etc/sudoers.d`. It should therefore be a separate
OpenSpec change with a narrow privileged-change contract.

## Goals / Non-Goals

**Goals:**

- Provision standard local Unix accounts for terminal identities.
- Configure passwordless `sudo -iu <target>` from the current GUI user to the
  managed target account.
- Hide managed terminal accounts from normal GUI login surfaces by default.
- Create or update a matching `sudo_user` Terminal Profile after successful
  provisioning.
- Make every privileged change previewable, explicit, idempotent, and
  verifiable.
- Provide repair and rollback plans for partially provisioned accounts.

**Non-Goals:**

- Enabling macOS GUI automatic login.
- Creating admin accounts by default.
- Granting passwordless root access.
- Managing FileVault unlock, SecureToken, volume ownership, or Apple Account
  login.
- Storing reusable plaintext passwords in Alan config, logs, workspace
  manifests, or Terminal Profiles.
- Becoming a general-purpose macOS account-management UI.

## Decisions

1. **Name the feature Managed Terminal Account, not autologin.**

   "Autologin" on macOS means GUI automatic login and carries a different
   security model. The product language should describe terminal-only managed
   accounts and passwordless terminal entry through sudo.

2. **Use a previewable privileged plan.**

   The Settings UI should build a deterministic plan before applying changes:
   create account if missing, ensure home and shell, optionally hide the
   account from the login window, write sudoers drop-in, validate sudoers, run
   non-interactive sudo check, create Terminal Profile. The user approves the
   plan before any privileged command runs.

3. **Start with a CLI/script executor, leave room for a signed helper.**

   The first implementation can route the plan through explicit administrator
   authorization and shell commands. The design should isolate privileged
   operations behind a service boundary so a later signed privileged helper can
   replace shell execution without changing Settings or model semantics.

4. **Create standard accounts with random non-reused passwords.**

   Managed terminal accounts should be standard users, not admins. Alan may
   generate a random high-entropy password to satisfy macOS local-account
   creation requirements, but it should not persist that password. Terminal
   entry uses sudo policy from the GUI user, not password login to the target
   account.

5. **Write narrow sudoers drop-ins and validate before use.**

   Alan should write a dedicated file under `/etc/sudoers.d` for managed
   terminal accounts. Rules should allow the invoking GUI user to run as only
   the specified target account. The sudoers file is validated with `visudo -cf`
   before the feature is marked ready.

6. **Verification is part of provisioning.**

   Provisioning succeeds only after account lookup, home/shell checks, sudoers
   validation, and `sudo -n -iu <target> true` pass. If verification fails,
   Settings shows repair actions rather than silently creating a Terminal
   Profile that will prompt or fail later.

7. **Rollback is explicit and conservative.**

   Rollback can remove Alan-owned sudoers drop-ins and Terminal Profiles.
   Removing the Unix account or home directory is a separate explicit action
   because it may destroy user data. Repair is preferred over destructive
   rollback for partially provisioned accounts.

## Risks / Trade-offs

- **[Risk] Account provisioning is privileged and can weaken the host.** ->
  Require explicit confirmation, narrow sudoers rules, non-admin target
  accounts, and validation before marking ready.
- **[Risk] Users confuse this with GUI automatic login.** -> Never use
  "autologin" in UI labels. State that the feature does not enable GUI
  automatic login.
- **[Risk] Password handling leaks sensitive material.** -> Generate passwords
  in-memory, avoid logging, avoid config persistence, and prefer interactive
  administrator authorization where needed.
- **[Risk] Sudoers syntax errors can break sudo.** -> Validate generated files
  with `visudo -cf` before relying on them and preserve backups for repair.
- **[Risk] Hiding login-window users is platform-version sensitive.** -> Treat
  hidden-login behavior as best-effort and verify account readiness through
  terminal entry, not GUI list visibility.
- **[Risk] SecureToken/FileVault behavior is misunderstood.** -> Keep this
  feature terminal-only and explicitly avoid FileVault unlock or SecureToken
  enablement in V1.

## Migration Plan

1. Add Managed Terminal Account model types for requested account, current
   system state, privileged plan steps, results, and verification status.
2. Add a dry-run planner that inspects local accounts and sudoers state without
   writing.
3. Add sudoers renderer and validation helpers with deterministic file paths and
   no user-controlled raw sudoers fragments.
4. Add an executor boundary for privileged account/sudoers operations.
5. Add verification and repair status calculation.
6. Connect successful provisioning to Terminal Profile creation/update.
7. Add Settings UI for preview, confirmation, progress, repair, and rollback.
8. Validate with focused tests and a dev-channel manual smoke path.

Rollback: remove Alan-owned sudoers entries and matching Terminal Profile when
requested. Account deletion and home deletion require separate explicit user
confirmation and should not run as part of ordinary rollback.

## Open Questions

- Should V1 offer account deletion, or only remove Alan's sudoers/Profile
  integration?
- Should hidden-login-window behavior be default-on for all managed terminal
  accounts, or configurable per account?
- Should the first privileged executor be a user-visible script, an Apple
  Authorization Services flow, or a signed privileged helper?
