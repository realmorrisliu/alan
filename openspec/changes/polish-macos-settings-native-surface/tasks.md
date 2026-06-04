## 1. Baseline And Acceptance Criteria

- [x] 1.1 Review the current Settings screenshots and code paths in
  `TerminalPaneView.swift` to identify the source-list, detail pane, section,
  row, value, and control components that need visual polish.
- [x] 1.2 Translate the spec scenarios into a short implementation checklist:
  compact source list, direct sectioned preference list, unified setting rows,
  subtle surface depth, controlled accent color, and no dashboard/card-page chrome.
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
- [x] 2.3 Replace card-like section treatment with direct preference sections
  using readable section titles, native separators, and dividers aligned from
  the text column.
- [x] 2.4 Tune window/titlebar, Settings source list, detail pane, preference rows,
  and controls so the layers remain distinguishable without decorative
  gradients or dashboard shadows.

## 3. Row Rhythm, Typography, And Controls

- [x] 3.1 Establish Settings-specific typography roles for pane title, source
  list item, section label, row label, row description, and trailing value.
- [x] 3.2 Align label, description, value, action, toggle, and segmented control
  positions across rows with one title/detail/control template and a bounded
  trailing control edge.
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
- [x] 5.5 Run a macOS app build from a writable DerivedData path.
- [ ] 5.6 Assemble or launch a fresh Alan Dev build, open Settings in light
  mode, and capture/review a screenshot against the native-surface checklist.
- [x] 5.7 Run `openspec validate polish-macos-settings-native-surface --strict`.

## 6. Review And Archive Readiness

- [x] 6.1 Review the implementation against this change's spec delta and the
  existing `macos-shell-ui-ux-conformance` contract for duplicate or conflicting
  Settings requirements.
- [x] 6.2 Document the final visual verification result, including whether the
  screenshot passes compact source list, direct sectioned preference layout, row alignment,
  surface depth, controlled accent color, and no dashboard chrome.
- [ ] 6.3 Before archiving after merge, sync accepted delta requirements into
  `openspec/specs/macos-shell-ui-ux-conformance/spec.md`.
- [ ] 6.4 Archive the completed change only after implementation, verification,
  and PR merge are complete.

## 7. Control-Panel Direction Pivot

- [x] 7.1 Remove the stable white page sheet as the primary Settings detail
  container.
- [x] 7.2 Replace uppercase section labels and sheet/card hierarchy with
  section titles, horizontal dividers, and compact preference rows.
- [x] 7.3 Simplify System metadata copy into direct control-panel labels such as
  Bundle ID, Channel, Daemon endpoint, Alan home, Skill packages, and Control
  namespace.
- [x] 7.4 Change read-only metadata rows to the shared title/detail template so
  System values sit below their labels instead of forming a database-like table.
- [x] 7.5 Add compact System control affordances for natural local actions such
  as copying the daemon endpoint and opening local folders, without adding fake
  edit controls for read-only install facts.
- [x] 7.6 Simplify Settings source-list selection to a native capsule fill with
  primary selected text and no blue accent bar.
- [x] 7.7 Keep the detail content in a left-anchored 760pt maximum-width column
  so wide windows do not create a sparse full-width reading path.
- [x] 7.8 Re-run focused Settings tests, shell contract checks, diff checks, and
  OpenSpec validation after the direction pivot.
- [ ] 7.9 Relaunch Alan Dev fresh and review a light-mode screenshot against the
  Linear/Raycast-style control-panel criteria.

## 8. Native Capsule And Typography Pass

- [x] 8.1 Apply the final source-list capsule selection, 188pt navigation width,
  24pt navigation top inset, 12/8pt navigation side insets, 30pt navigation row
  height, 760pt content width, 48pt detail padding, 48/56pt row heights,
  unified setting rows, bounded 188pt control column, and lighter section/row
  typography.
- [x] 8.2 Replace web-style action language with native Settings copy, including
  Show..., Create..., Preview..., and a real daemon endpoint Copy button.
- [x] 8.3 Reorganize Agent into Agent, Runtime, Skills, and Entry Points, with
  Skill packages inside Skills and without a separate Sources section.
- [x] 8.4 Re-run focused Settings tests, shell contract checks, diff checks, and
  OpenSpec validation after the native capsule pass.
- [ ] 8.5 Relaunch Alan Dev fresh and review a light-mode screenshot against the
  "navigation Apple, content Linear, controls native" target.
