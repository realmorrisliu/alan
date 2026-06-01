## Why

Alan Terminal Profiles can launch into separate Unix users, but users still need
to create those accounts and configure passwordless `sudo -iu <user>` manually.
Alan for macOS should offer a controlled local provisioning flow for terminal
identities without enabling macOS GUI automatic login.

## What Changes

- Add **Managed Terminal Account** provisioning for local standard Unix accounts
  intended for terminal use, such as `alan`, `univer`, or `lab`.
- Create or repair accounts with home directory, shell, hidden-login-window
  preference, and non-admin account type.
- Configure a narrow sudoers drop-in that allows the current GUI user to enter
  the managed account with `sudo -iu <target>` without a password.
- Verify the provisioned account with non-interactive sudo checks before marking
  it ready.
- Create or update the matching Terminal Profile after provisioning succeeds.
- Provide a dry-run/preview plan and explicit confirmation before privileged
  changes are applied.
- Keep macOS GUI automatic login out of scope and explicitly disabled as a
  product concept for this feature.

## Capabilities

### New Capabilities

- `macos-terminal-account-provisioning`: Defines managed local terminal account
  creation, sudoers configuration, verification, repair, rollback, and Terminal
  Profile handoff.

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: Add Settings affordances for creating and
  repairing Managed Terminal Accounts without implying GUI automatic login.
- `macos-shell-build-test-contract`: Require focused verification for account
  provisioning plans, sudoers generation, validation, rollback, and UI safety
  copy.

## Impact

- Apple client needs a local account provisioning model and Settings flow.
- A privileged execution path is required for user creation, account hiding,
  sudoers drop-in writes, and validation commands.
- Implementation should prefer a previewable command plan first; a signed
  privileged helper can follow if direct shell execution is not acceptable.
- Terminal Profile creation depends on the `add-macos-terminal-profiles` change
  or an equivalent Terminal Profile store.
- Tests must cover dry-run output, generated sudoers text, `visudo` validation
  behavior, idempotent repair, rollback planning, and redaction of passwords or
  secret material.
