## Context

alan's macOS shell already has an agent-facing file/socket control plane, which
is a product strength. What is missing is the native automation and test
coverage expected from a terminal-grade Mac app: App Intents for user/system
automation, focused Apple-client tests, UI smoke evidence, and repeatable
quality commands.

This change makes automation and verification first-class without replacing the
control plane. The control plane remains canonical for agents and tooling; App
Intents expose safe, user-facing shell actions to Shortcuts, Spotlight, and
future macOS surfaces.

## Goals / Non-Goals

**Goals:**

- Add App Intents and App Entity queries for shell windows, spaces, tabs, panes,
  attention items, and core terminal workspace actions.
- Keep App Intent outcomes aligned with shell controller/control-plane mutation
  semantics.
- Add layered Apple-client tests: model, runtime service fakes, control-plane
  command execution, App Intent command routing, and UI smoke/screenshot checks.
- Add discoverable `just` or script entry points for focused Apple checks.
- Keep most tests runnable without real Ghostty artifacts, with a smaller
  integration lane for linked Ghostty behavior.

**Non-Goals:**

- Implement all terminal runtime and surface parity work. This change supplies
  automation/test surfaces around those boundaries.
- Expose private socket paths or raw pane IDs as user-facing App Intent copy.
- Replace alan's CLI/daemon tests or Rust runtime quality gates.

## Decisions

1. Model App Intents as a native facade over shell controller actions.

   Intents resolve entities, call the same shell controller commands used by
   menus/control plane where possible, and return structured user-facing
   outcomes.

   Alternative considered: have intents write directly to socket files. That
   duplicates routing, bypasses main-actor state, and makes intent behavior drift
   from the app.

2. Define entity identifiers as stable but display-safe.

   App entities can carry internal IDs, but their display representation uses
   space/tab/pane titles, cwd, process context, and window context rather than
   raw IDs.

   Alternative considered: expose raw pane IDs everywhere for debugging. That is
   useful to agents but poor native UX and conflicts with progressive
   disclosure.

3. Add layered test targets and keep real Ghostty optional by default.

   Shell model, runtime service, control plane, and intent routing tests use
   fakes. A separate integration lane runs when Ghostty artifacts are prepared.

   Alternative considered: one UI test target for everything. That would be slow,
   brittle, and unable to cover model edge cases well.

4. Use repeatable screenshot or UI-smoke evidence for UI conformance.

   Tests do not need to assert every pixel, but they must launch the app and
   exercise representative workflows: launch, switch space/tab, create split,
   open command UI, open pane-scoped Find for the focused pane, and type basic
   terminal input when Ghostty is available.

   Alternative considered: keep screenshot review purely manual. Manual review
   remains useful but should not be the only repeatable signal.

5. Treat secure input and terminal content as sensitive.

   App Intents and test logging must not expose terminal input or secure text
   content. Summaries use metadata and explicit user actions.

## Risks / Trade-offs

- App Intents APIs vary across macOS versions -> Gate availability clearly and
  keep the control plane available on all supported versions.
- UI tests can be flaky -> Start with smoke checks and screenshot capture rather
  than complex pixel-perfect assertions.
- Test target setup can cause Xcode project churn -> Keep target additions
  minimal and document commands in `clients/apple/README.md`.
- Intent/entity queries may run when no window is active -> Return useful empty
  or needs-app-launch states instead of crashing or inventing shell state.
- Automation can leak terminal content -> Restrict summaries and redact secure
  or unknown-sensitive terminal text.

## Migration Plan

1. Add shell command protocols that can be called by control plane, menus, tests,
   and App Intents.
2. Add App Entity types and queries backed by shell state snapshots.
3. Add App Intents for core shell actions and align result semantics with
   control-plane responses.
4. Add test targets and fake fixtures for shell model, runtime service, control
   plane, and intent routing.
5. Add UI smoke/screenshot scripts and document `just` commands.
6. Add a Ghostty-enabled integration lane after local artifacts are prepared.

## Open Questions

- Which macOS deployment target should own App Intent availability for the Apple
  client?
- Should App Intents target only the active window initially, or allow explicit
  window entity selection from day one?
- Which screenshot tooling should be canonical for CI versus local design
  review?
