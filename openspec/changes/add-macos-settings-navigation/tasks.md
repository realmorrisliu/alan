## 1. Settings Group Model

- [ ] 1.1 Replace the navigation group order with General, Terminal, Agent, and System.
- [ ] 1.2 Add a group-section model that can assemble user-task sections from existing row IDs without rebuilding row contents.
- [ ] 1.3 Ensure Terminal contains Terminal Profiles, Managed Terminal Account, Mac login session, and sudo behavior rows.
- [ ] 1.4 Ensure Agent contains Alan selector, provider connection, model, credential, account action, runtime default, skill status, skill package source, and command line tool rows.
- [ ] 1.5 Ensure System contains app identity, install channel, daemon endpoint, updates, Alan home, shell state, shell control, and diagnostics rows.
- [ ] 1.6 Rename the old `Public skills` row to `Skill package path`.

## 2. Settings Layout

- [x] 2.1 Update `ShellSettingsContentView` to own `selectedGroup` state defaulting to General.
- [x] 2.2 Replace the single full-page section scroll with a two-column Settings layout.
- [x] 2.3 Add a compact `ShellSettingsNavigationView` with SF Symbol icons, restrained selected state, and shell-native density.
- [ ] 2.4 Add an Alan-only selector affordance at the top of Agent without showing Codex until Codex settings are supported.
- [ ] 2.5 Render only the selected group in the main content area while preserving existing row controls and value labels.
- [ ] 2.6 Preserve compact unavailable provider connection and skill catalog rows inside Agent when daemon or skill catalog data is unavailable.

## 3. Tests

- [ ] 3.1 Update model tests for General, Terminal, Agent, and System group order.
- [ ] 3.2 Add row membership tests proving Terminal Profiles and Managed Terminal Accounts stay in Terminal and do not appear under Agent provider connection rows.
- [ ] 3.3 Add row membership tests proving provider connection, runtime defaults, skills, skill package path, and command line tool rows appear in Agent.
- [ ] 3.4 Add row membership tests proving daemon endpoint, shell state, shell control, and diagnostics rows appear in System.
- [ ] 3.5 Keep or update redaction tests so grouped visible text does not expose secrets, raw custom commands, or credential setting names.
- [ ] 3.6 Keep or update tests for compact unavailable provider connection and skill catalog behavior.
- [ ] 3.7 Keep or update existing Settings singleton and non-terminal content lifecycle tests.

## 4. Verification

- [ ] 4.1 Run `bash clients/apple/scripts/test-shell-settings-surface.sh`.
- [ ] 4.2 Run `bash clients/apple/scripts/test-shell-runtime-metadata.sh`.
- [ ] 4.3 Run `bash clients/apple/scripts/check-shell-contracts.sh`.
- [ ] 4.4 Run `openspec validate add-macos-settings-navigation --strict`.
- [ ] 4.5 Build and relaunch Alan Dev fresh, then verify Settings in light mode for default General selection, Agent/System group switching, long Agent/System content, and row control layout.

## 5. Review And Archive Readiness

- [ ] 5.1 Review the implementation against `openspec/specs/macos-shell-ui-ux-conformance/spec.md` and this change's delta spec for duplicate or conflicting Settings requirements.
- [ ] 5.2 Before archiving after merge, sync the accepted delta requirements into `openspec/specs/macos-shell-ui-ux-conformance/spec.md`.
- [ ] 5.3 Archive the completed change only after implementation, verification, PR review, and merge are complete.
