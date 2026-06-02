## 1. Baseline And Acceptance Criteria

- [x] 1.1 Review the current Settings screenshots and code paths in
  `TerminalPaneView.swift` to identify the source-list, detail pane, section,
  row, value, and control components that need visual polish.
- [x] 1.2 Translate the spec scenarios into a short implementation checklist:
  compact source list, inset grouped detail form, stable row columns, subtle
  surface depth, controlled accent color, and no dashboard/card-page chrome.
- [x] 1.3 Confirm the existing General, Terminal, Agent, and System group
  membership from `ShellSettingsSurfaceModel.swift` remains unchanged for this
  polish slice.

## 2. Native Source List And Detail Structure

- [x] 2.1 Rework the internal Settings navigation into a compact native
  source-list treatment with restrained row height, icon size, label weight,
  material depth, and selection state.
- [x] 2.2 Rework the selected-group detail pane into a left-anchored compact
  content column with a stable maximum width instead of full-window stretched
  settings rows.
- [x] 2.3 Replace card-like section treatment with inset grouped settings rows
  using quiet fill, subtle stroke, native separators, and dividers aligned from
  the text column.
- [x] 2.4 Tune window/titlebar, Settings source list, detail pane, grouped rows,
  and controls so the layers remain distinguishable without decorative
  gradients or dashboard shadows.

## 3. Row Rhythm, Typography, And Controls

- [x] 3.1 Establish Settings-specific typography roles for pane title, source
  list item, section label, row label, row description, and trailing value.
- [x] 3.2 Align icon, label, description, value, action, toggle, and segmented
  control columns across rows, with a stable trailing control edge.
- [x] 3.3 Add concise secondary copy for ambiguous General rows such as Sidebar
  and Inactive split dimming while keeping descriptions visually subordinate.
- [x] 3.4 Reduce accent-color dominance by using native control styling and
  limiting bright blue treatment to active state and actionable affordances.
- [x] 3.5 Verify sparse groups still feel intentional through compact row height,
  section spacing, and form rhythm rather than oversized headers or filler
  panels.

## 4. Behavior Preservation

- [x] 4.1 Preserve existing Settings group selection, row membership, singleton
  Settings tab behavior, and non-terminal lifecycle behavior.
- [x] 4.2 Preserve existing `@AppStorage` bindings for appearance, sidebar
  visibility, and inactive split dimming.
- [x] 4.3 Preserve redaction, unavailable-state rendering, read-only rows, and
  action-only row semantics for Terminal, Agent, and System settings.
- [x] 4.4 Update focused Swift/script tests only where visual metadata or row
  descriptions require expected snapshot/model changes.

## 5. Verification

- [x] 5.1 Run `bash clients/apple/scripts/test-shell-settings-surface.sh`.
- [x] 5.2 Run `bash clients/apple/scripts/test-shell-runtime-metadata.sh`.
- [x] 5.3 Run `bash clients/apple/scripts/check-shell-contracts.sh`.
- [x] 5.4 Run `git diff --check`.
- [x] 5.5 Run a macOS app build from a repo-local DerivedData path.
- [x] 5.6 Assemble or launch a fresh Alan Dev build, open Settings in light
  mode, and capture/review a screenshot against the native-surface checklist.
- [x] 5.7 Run `openspec validate polish-macos-settings-native-surface --strict`.

## 6. Review And Archive Readiness

- [x] 6.1 Review the implementation against this change's spec delta and the
  existing `macos-shell-ui-ux-conformance` contract for duplicate or conflicting
  Settings requirements.
- [x] 6.2 Document the final visual verification result, including whether the
  screenshot passes compact source list, inset grouped form, row alignment,
  surface depth, controlled accent color, and no dashboard chrome.
- [ ] 6.3 Before archiving after merge, sync accepted delta requirements into
  `openspec/specs/macos-shell-ui-ux-conformance/spec.md`.
- [ ] 6.4 Archive the completed change only after implementation, verification,
  and PR merge are complete.
