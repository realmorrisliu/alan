## 1. Establish the design-system home

- [ ] 1.1 Create `clients/apple/alan-macos/Views/Shell/Components/` and add it to the Xcode project group
- [ ] 1.2 Relocate `Views/Shell/Controls/ShellFormControls.swift` content into `Components/` (split by primitive family if the file is large), keeping existing public API names intact
- [ ] 1.3 Update the Apple client README directory section to document `Views/Shell/Components/` as the home of reusable presentational primitives

## 2. Define the token single-source / semantic layer

- [ ] 2.1 Audit `Support/ShellDesignTokens.swift` and confirm every styling value feature surfaces need is exposed under a semantic name (e.g. `sidebarSelection`, `action`, `focus`)
- [ ] 2.2 Add any missing semantic tokens identified by the audit (no token *value* changes; semantic naming only)
- [ ] 2.3 Document the rule (design-system layer only may read raw tuples / `ShellPalette.*`) in a short header comment in the `Components/` home

## 3. Build canonical primitives (consolidating duplicates)

- [ ] 3.1 Implement `ShellRow` (icon + title + subtitle + accessory, with hover/selected/disabled states) to replace `ShellSettingsRow`, `ShellSettingsAgentSummaryRow`, `ShellTabSidebarRow`, `TerminalInfoRow`, `TimelineRow`
- [ ] 3.2 Implement card/panel surface modifiers (`shellCardSurface`, `shellPanelSurface`) consolidating `TerminalInfoCard`
- [ ] 3.3 Implement `ShellBadge`/`ShellChip` consolidating `TerminalPaneChip`
- [ ] 3.4 Implement one shared press/hover `ButtonStyle`/`ViewModifier` replacing `SidebarActionButtonStyle`, `InlineActionButtonStyle`, `ShellButtonPressStyle`
- [ ] 3.5 Confirm the canonical field primitive (`ShellTextField` + shared field style) supersedes `CompactDarkFieldStyle`; mark the duplicate for deletion during its surface migration
- [ ] 3.6 Add `ShellSectionHeader` and confirm `ShellFormSectionLabel` coverage

## 4. Preview galleries and accessibility baseline

- [ ] 4.1 Add a `#Preview` gallery for every primitive covering default/hover/selected/disabled and dark appearance
- [ ] 4.2 Ensure each control primitive exposes a VoiceOver-accessible label, scales under Dynamic Type without clipping, and honors reduce-motion for animated feedback

## 5. Verification

- [ ] 5.1 Run `just apple-shell-focused-tests` and the UI smoke check; confirm pass
- [ ] 5.2 Capture preview-gallery screenshots (light + dark) for review of the primitive catalog
- [ ] 5.3 Confirm no feature surface was migrated in this change (Phase 0 is foundation only) and no token *values* changed

## 6. Review, archive readiness, and follow-up sequencing

- [ ] 6.1 Request code review / open PR for the Phase 0 foundation change
- [ ] 6.2 Sync the `macos-shell-component-system` delta spec into `openspec/specs/` after merge, then archive this change
- [ ] 6.3 Record the strangler-fig migration backlog as separate follow-up changes, one per surface (sidebar → space slider → console → settings panels → terminal-pane SwiftUI chrome), each gated on inline-styling reduction + screenshot parity per the spec
