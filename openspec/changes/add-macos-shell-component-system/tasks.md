## 1. Establish the design-system home

- [ ] 1.1 Create `clients/apple/alan-macos/Views/Shell/Components/` and add it to the Xcode project group
- [ ] 1.2 Relocate `Views/Shell/Controls/ShellFormControls.swift` content into `Components/` (split by primitive family if the file is large), keeping existing public API names intact
- [ ] 1.3 Update the Apple client README directory section to document `Views/Shell/Components/` as the home of reusable presentational primitives

## 2. Define the token single-source / semantic layer

- [ ] 2.1 Audit `Support/ShellDesignTokens.swift` and confirm every styling value feature surfaces need is exposed under a semantic name (e.g. `sidebarSelection`, `action`, `focus`)
- [ ] 2.2 Add any missing semantic tokens identified by the audit (no token *value* changes; semantic naming only)
- [ ] 2.3 Document the rule (design-system layer only may read raw tuples / `ShellPalette.*`) in a short header comment in the `Components/` home

## 3. Build canonical primitives (Phase 0 builds only — no feature-file edits)

> Phase 0 only *creates* the primitives in the design-system home and records which
> existing structs each one is designed to supersede. The actual replacement/deletion
> in `TerminalPaneView.swift` / `ShellSidebarView.swift` / `MacShellRootView.swift`
> happens in the per-surface migration changes (see 6.3), never here — task 5.3 keeps
> Phase 0 free of feature-surface migration.

- [ ] 3.1 Implement `ShellRow` (icon + title + subtitle + accessory, with hover/selected/disabled states) in the design-system home, designed to supersede the shell row structs `ShellSettingsRow`, `ShellSettingsAgentSummaryRow`, `TerminalInfoRow`, `ShellTabSidebarRow`, and `ShellSidebarTabControlRow` during their surface migrations (do not edit those files in Phase 0; console's `TimelineRow` is out of scope)
- [ ] 3.2 Implement card/panel surface modifiers (`shellCardSurface`, `shellPanelSurface`) in the design-system home, designed to supersede `TerminalInfoCard` and the `ShellWorkspacePanelFrame` modifier later (build only)
- [ ] 3.3 Implement `ShellBadge`/`ShellChip` in the design-system home, designed to supersede `TerminalPaneChip` later (build only)
- [ ] 3.4 Confirm the existing `ShellButtonPressStyle` is the single canonical press/hover style for shell controls and expose it from the design-system home (no console styles in scope: `SidebarActionButtonStyle`/`InlineActionButtonStyle` are console-only)
- [ ] 3.5 Confirm the canonical field primitive `ShellTextField` is the shell field treatment (console's `CompactDarkFieldStyle` is out of scope and not touched)
- [ ] 3.6 Add `ShellSectionHeader` and confirm `ShellFormSectionLabel` coverage

## 4. Preview galleries and accessibility baseline

- [ ] 4.1 Add a `#Preview` gallery for every primitive covering default/hover/selected/disabled and dark appearance
- [ ] 4.2 Ensure each control primitive exposes a VoiceOver-accessible label, scales under Dynamic Type without clipping, and honors reduce-motion for animated feedback

## 5. Verification

- [ ] 5.1 Run `just apple-shell-focused-tests` and the UI smoke check; confirm pass
- [ ] 5.2 Capture preview-gallery screenshots (light + dark) for review of the primitive catalog
- [ ] 5.3 Confirm no feature surface was migrated in this change (Phase 0 is foundation only) and no token *values* changed
- [ ] 5.4 Verify the migration-debt baseline recorded in the spec (the canonical home for the ratchet reference point) still matches the tree at foundation-merge time, re-measuring if any count drifted. The baseline is **feature-surface** occurrences only — exclude the out-of-scope console AND the design-system layer (`Support/ShellDesignTokens.swift` and the control/`Components` home), which the spec permits to reference `ShellPalette.*` directly. Feature-surface counts for the four counted signals: `ShellPalette.*` ≈ 136, `RoundedRectangle` ≈ 29, raw color literals ≈ 20, raw `.font(.system(size:))` ≈ 37. Compute as `all − console − design-system-layer` to avoid both the broken-exclusion-glob trap and counting permitted token-layer refs; e.g. for `RoundedRectangle`, `rg -o 'RoundedRectangle' clients/apple/alan-macos -g '*.swift'` (71) minus `Views/Console` (34) minus `ShellFormControls.swift` (8) = 29; for `ShellPalette.*`, 197 minus console (0) minus `ShellDesignTokens.swift` (46) minus `ShellFormControls.swift` (15) = 136; raw color (`Color.<hue>` + `Color(...)` constructors, excluding `.clear`/`.primary`/`.secondary`/`.accentColor`) = 16+4+0 = 20; raw font (`.font(.system(size:`) = 30+6+1 = 37. Also confirm the spec's known primitive-role duplicate list matches the tree — rows (`ShellSettingsRow`, `ShellSettingsAgentSummaryRow`, `TerminalInfoRow`, `ShellTabSidebarRow`, `ShellSidebarTabControlRow`), card/surface (`TerminalInfoCard`, `ShellWorkspacePanelFrame` modifier, `ShellSettingsNavigationRowBackground`, `ShellSidebarRowBackground`), chip (`TerminalPaneChip`) — remembering this list is a non-exhaustive backlog; the enforceable rule is role-based (no *new* primitive-role duplicate), not membership in the list, and feature-specific composite views (layout/split/title-bar/find-bar) are not debt

## 6. Review, archive readiness, and follow-up sequencing

- [ ] 6.1 Request code review / open PR for the Phase 0 foundation change
- [ ] 6.2 Sync the `macos-shell-component-system` delta spec into `openspec/specs/` after merge, then archive this change — valid because the contract is a ratchet (binds new/migrated code, forbids new violations, records remaining surfaces as tracked debt), so it is true at sync time even though surfaces are not yet migrated
- [ ] 6.3 Record the strangler-fig migration backlog as separate follow-up changes, one per **surface** (not per file — a file may host several surfaces). Five surfaces at landing across the three debt-carrying files: from `TerminalPaneView.swift` (file debt 82/14) → terminal-pane SwiftUI chrome and the settings surface; from `Views/Shell/ShellSidebarView.swift` (49/11) → sidebar and space slider; from `MacShellRootView.swift` (5/4) → root chrome. Each change targets one surface, gated on inline-styling reduction + screenshot parity per the spec; the legacy/mobile console (`Views/Console/`) is excluded. Completeness is verified at the file level — the per-file debt (82+49+5=136, 14+11+4=29) reconciles with the baseline and a file is fully retired only when all its surfaces are migrated — so deriving the list from the debt keeps any surface (e.g. root chrome) from being left unowned while preserving one-surface-per-PR isolation
