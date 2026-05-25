## Why

Alan's macOS Settings tab currently exposes only three local UI preferences, even
though the app already has user-relevant configuration and status surfaces for
connections, session defaults, skills, install channel, daemon access, CLI tools,
and updates. This makes Settings feel too thin for a terminal-first app that now
depends on both local shell behavior and agent/runtime configuration.

## What Changes

- Reorganize the macOS Settings tab into stable, task-oriented sections:
  Interface, Accounts, Sessions, Capabilities, and Local.
- Keep Settings inside the existing shell tab model and avoid turning it into a
  separate preference window, page-like dashboard, or nested navigation shell.
- Keep first-phase editing conservative: Interface preferences remain editable,
  Accounts and Local start as read-only summaries with focused actions, and
  advanced runtime/skill controls use progressive disclosure.
- Surface connection profile, provider, model, credential status, install
  channel, daemon URL, CLI tool, update policy, and local data locations without
  exposing secrets or raw implementation IDs.
- Clarify which configuration surfaces are safe for everyday users and which are
  advanced agent/runtime controls.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: Add requirements for a grouped, calm,
  terminal-first Settings tab that presents local UI preferences, connection
  summaries, session defaults, capability summaries, and local install/runtime
  status without adding dashboard chrome or leaking low-level configuration
  details.

## Impact

- `clients/apple/alan-macos/TerminalPaneView.swift` settings content will need to
  move from a single three-row group to sectioned settings content.
- The Apple client may need small read-only model/view helpers for current
  connection, install channel, daemon, CLI, update, and data-path summaries.
- `AlanAPIClient` may need connection and skills read endpoints before editable
  Accounts or Capabilities controls are added.
- Tests should cover Settings section presence, singleton Settings behavior,
  non-terminal content lifecycle, secret redaction, and first-phase read-only
  behavior for connection/local status rows.
