## Context

`ShellSettingsContentView` currently renders a `ShellSettingsSurfaceSnapshot` as
one vertical scroll of settings sections. The model already separates content
into Interface, Terminal Profiles, Terminal Accounts, Accounts, Sessions,
Capabilities, and Local, but the UI shows them as one mixed page.

This change follows the existing shell-hosted Settings contract: Settings stays
inside the Alan shell content area and keeps the same tab, split, and sidebar
behavior. The work is an information-architecture change inside that content
surface, not a move to a native `Settings` scene or a separate preference
window.

## Goals / Non-Goals

**Goals:**

- Add a compact left navigation inside the Settings content surface.
- Group settings by user task: General, Terminal, Accounts, Sessions,
  Capabilities, and Advanced.
- Render only the selected group in the main content area.
- Preserve current row behavior, redaction, unavailable states, and direct
  controls.
- Keep Settings visually calm, shell-native, and light-mode-first.

**Non-Goals:**

- Add new settings or change settings semantics.
- Persist the selected Settings group in the first pass.
- Make provider Accounts, Capabilities, Sessions, or Local/Advanced rows
  freeform-editable.
- Replace the outer shell sidebar, tab model, or content-container contract.
- Introduce a page-like Settings dashboard, hero, or separate preference window.

## Decisions

1. **Use an internal two-column Settings layout.**

   The selected design is a left navigation column plus a right selected-group
   content area. A pure scroll-anchor approach would preserve the long-page
   clutter, while a summary-heavy settings dashboard would add another layer of
   model and visual weight. Showing one group at a time directly solves the
   hierarchy problem.

2. **Keep `ShellSettingsSurfaceSnapshot` as the row source of truth.**

   The grouping model should derive from existing sections instead of
   duplicating row construction. This keeps current redaction, mutability,
   unavailable rows, and direct bindings aligned with existing tests.

3. **Map storage-oriented sections into user-task groups.**

   The group mapping is:

   - General: Interface rows.
   - Terminal: Terminal Profiles and Terminal Accounts rows.
   - Accounts: provider connection rows.
   - Sessions: new-session runtime rows.
   - Capabilities: skill catalog rows.
   - Advanced: Local rows, including diagnostics.

   Terminal Profiles and Terminal Accounts belong together because both are
   local terminal-entry configuration. Provider connection profiles remain in
   Accounts so terminal identity is not confused with LLM provider identity.

4. **Default to General and avoid persistence initially.**

   Settings opens on General every time. Persisting the selected group is not
   needed for the first IA pass and could make Settings reopen into an advanced
   maintenance surface unintentionally.

5. **Use quiet navigation styling.**

   The Settings navigation should feel subordinate to Alan's main sidebar:
   compact rows, SF Symbols, restrained text, and subtle selection. It should
   not use page tabs, large cards, marketing copy, or a second app-level
   navigation system.

## Risks / Trade-offs

- [Risk] The accepted spec forbids a "separate settings navigation shell." ->
  The delta clarifies that the forbidden pattern is an independent app-level
  navigation shell, while this change adds an internal content navigation that
  remains visually subordinate to the shell.
- [Risk] Long groups such as Terminal and Advanced still need scrolling. ->
  The right pane can scroll independently while the left group list remains
  stable.
- [Risk] Group mapping could duplicate or lose rows. -> Add model tests for
  group order and row membership, including Terminal/Profile separation from
  provider Accounts.
- [Risk] Settings becomes a place for every runtime knob. -> Keep this change
  scoped to current rows and keep advanced runtime controls governed by existing
  progressive-disclosure requirements.

## Migration Plan

1. Add a navigation group enum/model in `ShellSettingsSurfaceModel.swift`.
2. Add a grouping helper that maps `ShellSettingsSurfaceSnapshot.sections` into
   the six user-task groups.
3. Update `ShellSettingsContentView` to own `selectedGroup` state and render a
   two-column layout.
4. Extract a small `ShellSettingsNavigationView` for left navigation rows.
5. Reuse existing `ShellSettingsSectionView`, `ShellSettingsRow`,
   `ShellSettingsValueLabel`, and divider behavior where possible.
6. Add focused model tests, then run existing shell settings/runtime metadata
   tests.
7. Verify a fresh Alan Dev relaunch in light mode before UI acceptance.

Rollback is straightforward: keep the grouping model unused and return
`ShellSettingsContentView` to rendering `snapshot.sections` in the previous
single-scroll layout. Row-level behavior remains unchanged.

## Open Questions

- None for this implementation slice.
