## 1. Inventory And Boundaries

- [ ] 1.1 Confirm the current Settings entry path, singleton behavior, and
  non-terminal lifecycle tests still match the existing content-container
  contract.
- [ ] 1.2 Inventory the concrete data sources for Interface, Accounts,
  Sessions, Capabilities, and Local rows, and classify each as editable,
  read-only, action-only, or deferred.
- [ ] 1.3 Decide the first implementation slice for Sessions and Capabilities:
  summary-only, collapsed advanced, or fully hidden until daemon persistence is
  available.

## 2. Settings Information Architecture

- [ ] 2.1 Refactor `ShellSettingsContentView` into small section and row
  components without changing current Interface preference behavior.
- [ ] 2.2 Add the default section order: Interface, Accounts, Sessions,
  Capabilities, and Local.
- [ ] 2.3 Update row labels and secondary text so primary labels are
  user-facing and raw config filenames or IDs appear only as diagnostic detail.
- [ ] 2.4 Keep the visual treatment compact and shell-native: no page hero,
  nested cards, dashboard metrics, decorative gradients, or separate settings
  navigation shell.

## 3. First-Phase Data And Controls

- [ ] 3.1 Keep appearance mode, sidebar visibility, and inactive split dimming
  directly editable through the existing app preference state.
- [ ] 3.2 Add read-only Local rows for install channel, CLI tool name, daemon
  URL/default bind, update policy, and relevant data roots using existing
  channel/update/host helpers.
- [ ] 3.3 Add Accounts summary rows for current/default connection profile,
  provider, model, credential status, and test/login/set-key action availability
  through typed connection-control data rather than direct TOML parsing.
- [ ] 3.4 Add Sessions summary or controls for governance, reasoning effort,
  streaming mode, and recovery mode without exposing deprecated
  `thinking_budget_tokens`.
- [ ] 3.5 Add Capabilities summary state from the skill catalog when available,
  using `enabled` and `allow_implicit_invocation` terminology and avoiding
  legacy mount-mode labels.
- [ ] 3.6 Ensure unavailable data sources render compact unavailable states
  instead of stack traces, debug payloads, or empty sections.

## 4. Safety And Redaction

- [ ] 4.1 Ensure Settings never displays bearer tokens, API keys, refresh
  tokens, managed auth file contents, or raw secret-store values.
- [ ] 4.2 Ensure first-phase Accounts, Capabilities, and Local rows do not offer
  freeform editing of `agent.toml`, `connections.toml`, `host.toml`,
  `models.toml`, or credential stores.
- [ ] 4.3 Verify dev-channel Settings displays dev labels and locations
  (`Alan Dev`, `alan-dev`, `~/.alan-dev`, dev daemon defaults) without falling
  back to stable channel state.

## 5. Verification

- [ ] 5.1 Add or update focused Swift/script tests for Settings section
  presence, Interface preference bindings, and singleton Settings behavior.
- [ ] 5.2 Add tests or fixtures proving non-terminal Settings content does not
  create a shell process or Ghostty host.
- [ ] 5.3 Add tests for redaction and read-only behavior for Accounts, Local,
  and Capabilities rows.
- [ ] 5.4 Run `bash clients/apple/scripts/test-shell-runtime-metadata.sh`.
- [ ] 5.5 Run `bash clients/apple/scripts/check-shell-contracts.sh`.
- [ ] 5.6 Run the relevant macOS build or focused Apple client validation from a
  repo-local DerivedData path.
- [ ] 5.7 Install or launch a fresh app build and manually verify the Settings
  tab after relaunching the current Alan channel.

## 6. Review And Archive Readiness

- [ ] 6.1 Review the implementation against
  `openspec/specs/macos-shell-ui-ux-conformance/spec.md` and this change's
  delta spec for duplicate or conflicting Settings requirements.
- [ ] 6.2 Run `openspec validate improve-macos-settings-surface --strict`.
- [ ] 6.3 Before archiving after merge, sync the accepted delta requirements into
  `openspec/specs/macos-shell-ui-ux-conformance/spec.md`.
- [ ] 6.4 Archive the completed change only after implementation, verification,
  and PR merge are complete.
