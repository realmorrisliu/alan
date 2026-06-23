## Why

The current macOS Terminal Profile and local identity surfaces expose implementation
state instead of a coherent user model. Settings shows Profile and Identity rows
that are mostly non-interactive, while the Space menu presents `Default` beside
`Login shell` as if they were separate terminal identities.

This change repairs that model around two distinct layers: Terminal Profiles are
general startup profiles, and Managed Users are local terminal-only Unix accounts
that can create read-only managed Terminal Profiles after verification.

## What Changes

- Keep `Login shell` as Alan's built-in default terminal identity and safe
  fallback; do not expose `Default` as a separate Terminal Profile or menu item.
- Preserve two layers in Settings:
  - `Managed Users` creates, repairs, verifies, and removes Alan-owned
    terminal-only local users.
  - `Terminal Profiles` lists and edits general startup profiles, while managed
    user profiles remain visible but read-only.
- Support multiple Managed Users. The V1 creation form accepts only Unix user
  name and display label; Alan derives home directory, shell, hidden login-window
  setting, sudoers rule, verification, and Terminal Profile handoff.
- Make successful Managed User creation add a selectable identity only. It does
  not bind the current Space and does not change the default terminal identity.
- Update Space profile menus so an unbound Space shows `Login shell` as selected,
  ready Managed Users as selectable identities, and missing or not-ready
  profiles with clear repair states.
- Tighten verification so failed or partial Managed User setup cannot create a
  Terminal Profile that appears ready for passwordless terminal entry.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-terminal-account-provisioning`: multiple Managed Users, name+label V1
  input, read-only managed profile handoff, and no automatic Space binding.
- `macos-terminal-profiles`: built-in `Login shell` default/fallback semantics,
  read-only managed profiles, and no user-facing global default profile.
- `macos-shell-ui-ux-conformance`: Settings IA and Space menu behavior for
  Managed Users, Terminal Profiles, and `Login shell`.
- `macos-shell-terminal-lifecycle`: terminal startup fallback when no explicit
  or Space-bound profile exists.
- `macos-shell-build-test-contract`: focused tests for multiple Managed Users,
  read-only managed profiles, and menu/default semantics.

## Impact

- Apple client Settings models and views for Terminal identity.
- Terminal Profile store, editor affordances, and Space profile menus.
- Managed Terminal Account discovery, planning, verification, local profile
  handoff, and privileged apply flow.
- Shell state mutations and terminal launch resolution where global default
  Terminal Profile capture is currently applied.
- Focused Swift/script tests and fresh Alan Dev visual smoke verification.
