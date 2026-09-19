## 0. Alan OS And App Alignment

- [x] 0.1 Record the macOS shell component system as a host-surface/design-system capability that may render terminal, Alan OS, Agent Process, and Alan App file projections while leaving Kernel, file-server, Agent Runtime Service, and app-domain authority outside the component layer

## 1. Establish the design-system home

- [ ] 1.1 Create `clients/apple/alan-macos/Views/Shell/Components/` and add it to the Xcode project group
- [ ] 1.2 Relocate `Views/Shell/Controls/ShellFormControls.swift` content into `Components/` (split by primitive family if the file is large), keeping existing public API names intact
- [ ] 1.3 Update the Apple client README directory section to document `Views/Shell/Components/` as the home of reusable presentational primitives

## 2. Define the token single-source / semantic layer

- [ ] 2.1 Audit `Support/ShellDesignTokens.swift` and confirm every styling value feature surfaces need is exposed under a semantic name (e.g. `sidebarSelection`, `action`, `focus`)
- [ ] 2.2 Add any missing semantic tokens identified by the audit (no token *value* changes; semantic naming only)
- [ ] 2.3 Document the rule in a short header comment in the `Components/` home: feature surfaces consume semantic token namespaces (`ShellPaper`/`ShellInk`/`ShellSignal`/`ShellPalette`/`ShellType`/`ShellSpacing`) or primitives — referencing a token namespace is compliant; raw literals (`Color(red:`/`Color.red`/`.font(.system(size:`/numeric `.padding(`) are the debt
- [ ] 2.4 Wire the existing raw-literal guard into CI: add a blocking step to `.github/workflows/ci.yml` that runs `./scripts/check-shell-design-tokens.sh` (it is ubuntu-safe pure bash) so the ratchet stops being a local-only `just guard-shell-design-tokens` recipe. Do **not** add a parallel counter — adopt the existing guard. (Optional, can be deferred: extend the guard's pattern to also match `RoundedRectangle` with a literal radius and raw named hues like `Color.red`, regenerating the baseline with `--update-baseline`.)

## 3. Build canonical primitives (Phase 0 builds only — no feature-file edits)

> Phase 0 only *creates* the primitives in the design-system home and records which
> existing structs each one is designed to supersede. The actual replacement/deletion
> in `TerminalPaneView.swift` / `ShellSidebarView.swift` / `MacShellRootView.swift`
> happens in the per-surface migration changes (see 6.3), never here — task 5.3 keeps
> Phase 0 free of feature-surface migration.

- [ ] 3.1 Implement `ShellRow` (icon + title + subtitle + accessory, with hover/selected/disabled states) in the design-system home, designed to supersede the shell row structs `ShellSettingsRow`, `ShellSettingsAgentSummaryRow`, `TerminalInfoRow`, `ShellTabSidebarRow`, and `ShellSidebarTabControlRow` during their surface migrations (do not edit those files in Phase 0)
- [ ] 3.2 Implement card/panel surface modifiers (`shellCardSurface`, `shellPanelSurface`) in the design-system home, designed to supersede `TerminalInfoCard` and the `ShellWorkspacePanelFrame` modifier later (build only)
- [ ] 3.3 Implement `ShellBadge`/`ShellChip` in the design-system home, designed to supersede `TerminalPaneChip` later (build only)
- [ ] 3.4 Confirm the existing `ShellButtonPressStyle` is the single canonical press/hover style for shell controls and expose it from the design-system home
- [ ] 3.5 Confirm the canonical field primitive `ShellTextField` is the shell field treatment
- [ ] 3.6 Add `ShellSectionHeader` and confirm `ShellFormSectionLabel` coverage

## 4. Preview galleries and accessibility baseline

- [ ] 4.1 Add a `#Preview` gallery for every primitive covering default/hover/selected/disabled and dark appearance
- [ ] 4.2 Ensure each control primitive exposes a VoiceOver-accessible label, scales under Dynamic Type without clipping, and honors reduce-motion for animated feedback

## 5. Verification

- [ ] 5.1 Run `just apple-shell-focused-tests` and the UI smoke check; confirm pass
- [ ] 5.2 Capture preview-gallery screenshots (light + dark) for review of the primitive catalog
- [ ] 5.3 Confirm no feature surface was migrated in this change (Phase 0 is foundation only) and no token *values* changed
- [ ] 5.4 Confirm the raw-literal migration-debt baseline is the **existing** design-token guard, not a new count: `./scripts/check-shell-design-tokens.sh` passes, and `scripts/shell-design-token-baseline.txt` matches the current shell tree (`MacShellRootView.swift` 1, `TerminalPaneView.swift` 58, `ShellSidebarView.swift` 16). Do not introduce a parallel ShellPalette/RoundedRectangle count — `ShellPalette.*` is compliant token usage, not debt. Also confirm the spec's known primitive-role duplicate list matches the tree — rows (`ShellSettingsRow`, `ShellSettingsAgentSummaryRow`, `TerminalInfoRow`, `ShellTabSidebarRow`, `ShellSidebarTabControlRow`), card/surface (`TerminalInfoCard`, `ShellWorkspacePanelFrame` modifier, `ShellSettingsNavigationRowBackground`, `ShellSidebarRowBackground`), chip (`TerminalPaneChip`) — remembering this list is a non-exhaustive backlog; the enforceable rule is role-based (no *new* primitive-role duplicate), and feature-specific composite views (layout/split/title-bar/find-bar) are not debt

## 6. Review, archive readiness, and follow-up sequencing

- [ ] 6.1 Request code review / open PR for the Phase 0 foundation change
- [ ] 6.2 Sync the `macos-shell-component-system` delta spec into `openspec/specs/` after merge, then archive this change — valid because the contract is a ratchet (binds new/migrated code, forbids new violations, records remaining surfaces as tracked debt), so it is true at sync time even though surfaces are not yet migrated
- [ ] 6.3 Record the strangler-fig migration backlog as separate follow-up changes, one per **surface** (not per file — a file may host several surfaces). Five surfaces at landing across the three debt-carrying files (design-token guard baseline): from `TerminalPaneView.swift` (guard baseline 58) → terminal-pane SwiftUI chrome and the settings surface; from `Views/Shell/ShellSidebarView.swift` (16) → sidebar and space slider; from `MacShellRootView.swift` (1) → root chrome (visual controls/ghost chrome only; window placement stays in `Support/ShellWindowPlacement.swift`). Each change targets one surface, reduces that file's guard count via `--update-baseline` after a reviewed reduction, and is gated on screenshot parity. Completeness is the guard baseline file itself — every listed shell file must reach 0, and a file reaches 0 only when all its surfaces are migrated — so no surface (e.g. root chrome) is left unowned while one-surface-per-PR isolation holds
