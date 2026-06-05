## Context

`ShellSettingsContentView` currently renders a `ShellSettingsSurfaceSnapshot` as
one vertical scroll of settings sections. The model already separates content
into Interface, Terminal Profiles, Terminal Accounts, Accounts, Sessions,
Capabilities, and Local, but the UI shows them as one mixed page.

The first internal-navigation pass used six groups named General, Terminal,
Accounts, Sessions, Capabilities, and Advanced. That reduced page length, but it
still exposed storage or implementation concepts as primary navigation. The
revised IA groups settings by user task and configuration scope: Terminal as a
standalone terminal-app capability, Agent as Alan agent configuration, and
System as the local host/runtime environment.

This change follows the existing shell-hosted Settings contract: Settings stays
inside the Alan shell content area and keeps the same tab, split, and sidebar
behavior. The work is an information-architecture change inside that content
surface, not a move to a native `Settings` scene or a separate preference
window.

## Goals / Non-Goals

**Goals:**

- Add a compact left navigation inside the Settings content surface.
- Group settings by user task and scope: General, Terminal, Agent, and System.
- Render only the selected group in the main content area.
- Add an Alan-only agent selector affordance inside Agent, while leaving Codex
  hidden until it can actually be configured.
- Preserve current row behavior, redaction, unavailable states, and direct
  controls.
- Keep Settings visually calm, shell-native, and light-mode-first.

**Non-Goals:**

- Add new settings or change settings semantics.
- Persist the selected Settings group in the first pass.
- Add Codex configuration, show disabled Codex rows, or support switching to an
  unsupported agent.
- Add an explicit Agent-to-Terminal-Profile binding. For this slice, Terminal
  and Agent are managed independently.
- Make provider connection, skills, runtime default, or System rows
  freeform-editable beyond their current row behavior.
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

3. **Map rows into task and scope groups rather than storage sections.**

   The top-level navigation order is:

   - General: Interface rows.
   - Terminal: terminal profiles and local terminal identity rows.
   - Agent: Alan agent selector, connection rows, runtime defaults, skill
     status/source rows, and command line tool entry point.
   - System: app install/update rows, daemon endpoint, host storage paths, shell
     state/control rows, and diagnostics rows.

   Terminal Profiles and Managed Terminal Accounts belong together because both
   configure the terminal app's local startup identity. They should not be
   owned by Agent; they remain useful even if no agent surface is active. Agent
   owns LLM provider connection, runtime defaults, skills, skill package source,
   and CLI entry points because those describe how the Alan agent is configured
   and invoked. System owns daemon and shell-control state because those are
   host-level runtime services shared by Alan surfaces rather than private
   per-agent settings.

4. **Represent Agent as a future selector with only Alan visible now.**

   Agent should include a compact selector-like affordance so the IA can grow to
   multiple agents, but the current UI should only show Alan. Codex should not
   appear as a disabled option or coming-soon panel because unsupported choices
   add clutter without actionability.

5. **Rename ambiguous skill path copy.**

   The old Local row title `Public skills` is unclear. In the revised Agent
   group it should be named `Skill Packages` and placed under a Skill
   Sources section. This distinguishes current agent skill availability from
   the filesystem package source that feeds the skill catalog.

6. **Default to General and avoid persistence initially.**

   Settings opens on General every time. Persisting the selected group is not
   needed for the first IA pass and could make Settings reopen into an advanced
   maintenance surface unintentionally.

7. **Use quiet navigation styling.**

   The Settings navigation should feel subordinate to Alan's main sidebar:
   compact rows, SF Symbols, restrained text, and subtle selection. It should
   not use page tabs, large cards, marketing copy, or a second app-level
   navigation system.

## Risks / Trade-offs

- [Risk] The accepted spec forbids a "separate settings navigation shell." ->
  The delta clarifies that the forbidden pattern is an independent app-level
  navigation shell, while this change adds an internal content navigation that
  remains visually subordinate to the shell.
- [Risk] Long groups such as Agent and System still need scrolling. ->
  The right pane can scroll independently while the left group list remains
  stable.
- [Risk] Group mapping could duplicate or lose rows. -> Add model tests for
  group order and row membership, including Terminal independence, Agent row
  ownership, and System host/runtime ownership.
- [Risk] Settings becomes a place for every runtime knob. -> Keep this change
  scoped to current rows and keep advanced runtime controls governed by existing
  progressive-disclosure requirements.
- [Risk] CLI placement can be confused with System install state. -> The command
  line tool is treated as an Agent entry point for invoking Alan, while app
  install channel, daemon, shell state/control, and diagnostics remain System
  host/runtime rows.

## Migration Plan

1. Add a navigation group enum/model in `ShellSettingsSurfaceModel.swift`.
2. Replace section-ID-only grouping with a task-section model that can group
   existing rows by row ID without rebuilding row contents.
3. Map current rows into General, Terminal, Agent, and System sections.
4. Update `ShellSettingsContentView` to own `selectedGroup` state and render a
   two-column layout.
5. Extract or update a small `ShellSettingsNavigationView` for left navigation
   rows.
6. Reuse existing `ShellSettingsSectionView`, `ShellSettingsRow`,
   `ShellSettingsValueLabel`, and divider behavior where possible.
7. Add focused model tests, then run existing shell settings/runtime metadata
   tests.
8. Verify a fresh Alan Dev relaunch in light mode before UI acceptance.

Rollback is straightforward: keep the grouping model unused and return
`ShellSettingsContentView` to rendering `snapshot.sections` in the previous
single-scroll layout. Row-level behavior remains unchanged.

## Open Questions

- None for this implementation slice.
