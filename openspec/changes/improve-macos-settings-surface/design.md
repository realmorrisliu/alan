## Context

The current macOS Settings content is a shell tab rendered by
`ShellSettingsContentView`. It exposes three `@AppStorage` preferences:
appearance, sidebar visibility, and inactive split dimming. That matches the
current implementation but no longer matches the amount of user-relevant state
Alan needs to explain: connection profiles, session defaults, skill exposure,
install channel, daemon URL, CLI tool installation, update policy, and local
data roots.

Existing contracts already define the lower-level surfaces:

- settings content stays inside shell workspace chrome rather than becoming a
  separate settings window or dashboard
- connection profile metadata and credential material stay separated, with
  daemon/CLI control surfaces owning profile operations
- skills expose `enabled` and `allow_implicit_invocation` through the daemon
  catalog and override APIs
- stable and dev install channels have separate app identity, alan home,
  command name, daemon defaults, and shell-control namespace

The Settings change should therefore organize and expose configuration state,
not create a new configuration authority.

## Goals / Non-Goals

**Goals:**

- Turn Settings into a useful macOS shell control surface with clear sections:
  Interface, Accounts, Sessions, Capabilities, and Local.
- Preserve the existing tab-hosted Settings model and calm Arc-like shell
  visual language.
- Keep first-phase editing safe: local UI preferences are editable; connection,
  skill, runtime, daemon, and install details are summarized or routed through
  existing dedicated actions.
- Show status and sources in user-facing language while avoiding raw IDs,
  secrets, implementation class names, and unbounded debug details.
- Create a structure that can later accept editable Accounts, Session Defaults,
  and Capabilities controls without another IA rewrite.

**Non-Goals:**

- Adding a native macOS `Settings` scene or replacing the shell-tab Settings
  contract.
- Building a full `agent.toml`, `connections.toml`, `host.toml`, or
  `models.toml` editor.
- Introducing a new secret storage backend or reading secret values into the UI.
- Implementing every advanced runtime option in the first Settings pass.
- Changing provider, skills, daemon, or install-channel contracts.

## Decisions

1. **Settings remains a shell content tab.**

   The user opens Settings through the existing command path and receives a
   singleton shell content tab. This preserves workspace context, sidebar
   selection, split behavior, and the existing OpenSpec content-container
   contract. A separate `Settings` scene would fight the current shell model and
   split configuration across multiple windows.

2. **Use sections by user task, not by storage file.**

   The visible grouping is:

   - Interface: immediate app presentation preferences
   - Accounts: current profile, provider, model, credential state, test/login
   - Sessions: governance, reasoning, streaming, recovery defaults
   - Capabilities: skill exposure summaries and future override controls
   - Local: channel, daemon, CLI, updates, data paths, reset/diagnostic actions

   This avoids exposing storage names such as `agent.toml`, `connections.toml`,
   or `host.toml` as the primary navigation model. Storage paths can appear as
   secondary detail only when they help diagnose local state.

3. **First-phase editing is intentionally conservative.**

   Interface rows keep direct controls because they are local, reversible, and
   already implemented through `@AppStorage`. Accounts and Local rows begin as
   summaries plus explicit actions, because they touch auth, secrets, daemon
   routing, CLI installation, or update policy. Session defaults and
   Capabilities can start as read-only or collapsed advanced sections until the
   persistence and daemon flows are wired.

4. **Settings reads from existing authorities.**

   UI preferences use `@AppStorage`. Connection summaries use the daemon
   connection API when available. Skills use the daemon skill catalog. Local
   install status uses `AlanInstallChannel`, `AlanCommandLineToolInstaller`,
   `AlanMacUpdatePolicy`, and host-config resolution helpers. Settings should
   not parse or mutate TOML directly except through existing control APIs.

5. **Use quiet row groups rather than cards or a dashboard.**

   Each section should be a compact macOS-like row group with a clear label,
   restrained secondary text, and one focused trailing control or action where
   needed. Avoid nested cards, marketing copy, hero metrics, large icons, or
   debug-first composition. The content should remain scan-friendly inside a
   terminal workspace.

## Risks / Trade-offs

- **[Risk] Settings becomes a dumping ground for every runtime knob.** ->
  Keep Advanced collapsed and require each exposed control to map to a user
  task, not merely to an available config field.
- **[Risk] Read-only summaries feel non-functional.** -> Pair summaries with
  focused actions such as Test, Login, Set Key, Install CLI, Check Updates, or
  Reveal Data, but keep unsafe editing out of the first pass.
- **[Risk] Connection status leaks secrets or provider internals.** -> Show
  credential kind/status and profile/model labels only; never render secret
  values, bearer tokens, or managed auth contents.
- **[Risk] Direct file parsing diverges from runtime behavior.** -> Prefer
  existing daemon/CLI/control APIs and channel helpers. If a needed read API is
  missing, add a typed helper rather than ad hoc path parsing in the view.
- **[Risk] Section count adds visual weight.** -> Keep headings small, rows
  compact, and sections separated by rhythm rather than heavy card chrome.

## Migration Plan

1. Refactor `ShellSettingsContentView` into small section and row components
   while preserving existing `@AppStorage` behavior.
2. Add Interface, Accounts, Sessions, Capabilities, and Local sections with
   first-phase controls and summaries.
3. Wire Local summaries from existing install-channel, update-policy, and CLI
   installer helpers.
4. Add typed Apple-client read models for connection and skill summaries only
   when the implementation uses daemon APIs instead of file parsing.
5. Add focused tests for section presence, singleton Settings behavior,
   non-terminal lifecycle, redaction, and first-phase read-only rows.
6. Validate with focused shell scripts, Swift tests, and a fresh installed app
   relaunch before visual acceptance.

Rollback: if a data source proves unreliable, keep the section but show an
unavailable status with no mutation path. The existing Interface preferences can
remain functional independently of Accounts, Capabilities, or Local summaries.

## Open Questions

- Should Session defaults persist first in Apple client preferences or through
  the daemon's existing session-default/config surface once one is available?
- Should Capabilities show all skills immediately or start with a compact
  summary plus a searchable expanded view?
- Should Local data actions include destructive reset controls in this change,
  or only reveal/copy paths and diagnostics?
