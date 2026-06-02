## 1. Settings Group Model

- [ ] 1.1 Add a `ShellSettingsNavigationGroup` model with default order: General, Terminal, Accounts, Sessions, Capabilities, Advanced.
- [ ] 1.2 Add a grouping helper that maps `ShellSettingsSurfaceSnapshot.sections` into the six navigation groups without rebuilding row contents.
- [ ] 1.3 Ensure Terminal combines Terminal Profiles and Terminal Accounts, while Accounts contains only provider connection rows.
- [ ] 1.4 Ensure Advanced maps Local rows, including performance diagnostics and export action rows.

## 2. Settings Layout

- [ ] 2.1 Update `ShellSettingsContentView` to own `selectedGroup` state defaulting to General.
- [ ] 2.2 Replace the single full-page section scroll with a two-column Settings layout.
- [ ] 2.3 Add a compact `ShellSettingsNavigationView` with SF Symbol icons, restrained selected state, and shell-native density.
- [ ] 2.4 Render only the selected group in the main content area while preserving existing row controls and value labels.
- [ ] 2.5 Preserve compact unavailable rows for Accounts and Capabilities when daemon or skill catalog data is unavailable.

## 3. Tests

- [ ] 3.1 Add model tests for navigation group order and row membership.
- [ ] 3.2 Add tests proving Terminal Profiles and Managed Terminal Accounts stay in Terminal and do not appear under provider Accounts.
- [ ] 3.3 Keep or update redaction tests so grouped visible text does not expose secrets, raw custom commands, or credential setting names.
- [ ] 3.4 Keep or update tests for compact unavailable Accounts and Capabilities behavior.
- [ ] 3.5 Keep or update existing Settings singleton and non-terminal content lifecycle tests.

## 4. Verification

- [ ] 4.1 Run `bash clients/apple/scripts/test-shell-settings-surface.sh`.
- [ ] 4.2 Run `bash clients/apple/scripts/test-shell-runtime-metadata.sh`.
- [ ] 4.3 Run `bash clients/apple/scripts/check-shell-contracts.sh`.
- [ ] 4.4 Run `openspec validate add-macos-settings-navigation --strict`.
- [ ] 4.5 Build and relaunch Alan Dev fresh, then verify Settings in light mode for default General selection, group switching, long Terminal/Advanced content, and row control layout.

## 5. Review And Archive Readiness

- [ ] 5.1 Review the implementation against `openspec/specs/macos-shell-ui-ux-conformance/spec.md` and this change's delta spec for duplicate or conflicting Settings requirements.
- [ ] 5.2 Before archiving after merge, sync the accepted delta requirements into `openspec/specs/macos-shell-ui-ux-conformance/spec.md`.
- [ ] 5.3 Archive the completed change only after implementation, verification, PR review, and merge are complete.
