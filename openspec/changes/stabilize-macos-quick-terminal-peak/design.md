## Context

The accepted Quick Terminal Peak contract deliberately chose one global
quick-terminal runtime slot over per-Space or independent terminal runtimes.
That contract preserves scrollback and process state across hide/show, supports
`Open in Space` as a move rather than a copy, and keeps menu, shortcut,
automation, and control-plane behavior aligned through shared shell commands.

The problem is the implementation boundary. The current Peak is a detached
`NSPanel`, but it is driven by the primary shell owner, observes the full
`ShellHostController`, renders the general `TerminalPaneView`, and synchronously
attaches a Ghostty-backed terminal surface during the panel presentation path.
That means an ostensibly independent window still shares the main shell's
SwiftUI state churn, focus race surface, and main-thread terminal attachment
work.

This change keeps the product semantics and refactors the boundary: Quick
Terminal should behave like a dedicated macOS window/controller at the
presentation layer while remaining the same Alan-owned global terminal runtime
at the shell/runtime layer.

## Goals / Non-Goals

**Goals:**

- Preserve one global quick-terminal runtime slot and existing hide/show/close
  semantics.
- Preserve `Open in Space` as a move of the existing runtime into a normal Alan
  tab.
- Add a dedicated presentation controller and window presenter for Peak
  lifecycle, frame, Space behavior, and focus timing.
- Use a narrow Quick Terminal content view instead of the full workspace
  `TerminalPaneView`.
- Present the panel before terminal surface attachment and focus work, so slow
  Ghostty setup cannot run in the same synchronous stack as shell-state mutation
  and panel creation.
- Restore quick-terminal content as hidden on app launch.
- Avoid Alan Dev during verification; stable-channel verification is allowed.

**Non-Goals:**

- No independent quick-terminal runtime owner.
- No change to the single global quick-terminal instance model.
- No per-Alan-space or per-macOS-Space quick terminal instances.
- No focus-loss auto-hide behavior.
- No redesign of the normal terminal pane UI.
- No requirement to complete the full behavior test suite in the first
  implementation slice.

## Decisions

### 1. Keep runtime ownership in the shell model

`ShellHostController` remains the owner of quick-terminal state mutations:
show, hide, close, cwd selection, close guard routing, and promotion. The
terminal runtime still belongs to the existing terminal runtime service and is
keyed by the quick-terminal content identity.

The new controller must not create a second terminal runtime and must not copy
the process during promotion. Its job is to interpret shell state and drive
presentation.

### 2. Add a dedicated presentation state machine

Introduce a `QuickTerminalController` that observes the quick-terminal slot and
tracks presentation states such as:

- `idle`: no visible Peak presentation.
- `panelPreparing`: create or reuse the panel and install lightweight content.
- `panelVisible`: the panel is ordered front and has a window.
- `terminalAttaching`: terminal host attachment is allowed after the window is
  visible or after a main-runloop deferral.
- `terminalFocused`: focus was requested after the surface became attachable or
  ready.
- `hiding`: the panel is ordered out while runtime state remains live.
- `closing`: shell close semantics are running and panel resources may be
  released after the slot is cleared.

The state machine is intentionally presentation-only. Shell mutations continue
to flow through existing commands.

### 3. Isolate `NSPanel` details in a window presenter

Introduce a Quick Terminal window presenter that owns the `NSPanel`,
`NSWindowDelegate`, collection behavior, frame, level, ordering, and key-window
timing. It should make AppKit-valid Space behavior explicit and never combine
mutually exclusive collection behaviors.

Panel ordering and terminal focus should be separate operations. The presenter
may order the panel front immediately, but terminal focus is best-effort and is
scheduled after the panel is visible. Repeated focus attempts should be bounded
and should not continuously call `makeKey`.

### 4. Replace the Peak composition with a narrow content view

The Peak should render a dedicated `QuickTerminalContentView` rather than the
full `TerminalPaneView`. The content view should receive a narrow model with
only the data and callbacks it needs:

- quick-terminal pane/content mount.
- boot profile.
- runtime snapshot/render priority.
- close request handler.
- promote request handler.
- runtime update and metadata callbacks.

It should not include normal workspace sidebar, tab header, split controls,
selection management, or general workspace decoration. This prevents ordinary
workspace state updates from rebuilding the Peak more than necessary.

### 5. Defer terminal surface attachment and focus

Showing the Peak should not synchronously do every step. The flow should be:

1. Apply shell mutation to mark the quick-terminal slot visible.
2. Presentation controller creates/reuses and orders the panel.
3. After the panel is attached/visible, install or update the narrow content
   view.
4. Attach the terminal surface on a deferred main-actor step.
5. Request focus only after the terminal host is registered and window attached.

This separates user-visible window presentation from Ghostty surface setup and
reduces the chance that a slow terminal attachment makes the main window appear
frozen.

### 6. Restore hidden on launch

Workspace manifest materialization may restore quick-terminal content and last
working directory, but it must not auto-present the detached Peak on app launch.
If a previous manifest records `visible`, materialization should produce a
hidden presentation and wait for an explicit user show/toggle command.

### 7. Stage verification

The first implementation slice may focus on structure and stable-channel launch
safety:

- compile the Apple client target,
- prove stable launch no longer traps on Peak collection behavior,
- avoid Alan Dev entirely.

A follow-up verification slice should add or extend:

- presentation state-machine tests,
- AppKit harness coverage for panel collection behavior, visibility, and focus
  ordering,
- runtime attach/focus sequencing tests,
- stable-channel Quick Terminal behavior verification.

## Error Handling

- Panel creation failure should not clear the quick-terminal shell slot. It
  should leave the runtime/content available and allow a later toggle to retry.
- Terminal surface attach failure should be reported in the Peak content area or
  diagnostic state without blocking the main workspace UI.
- Focus failure is best-effort. The Peak may remain visible and accept a later
  click/focus retry instead of repeatedly forcing key status.
- Promotion clears the quick-terminal slot and releases the panel after shell
  mutation succeeds. The presenter does not copy or finalize the runtime.
- Close continues to use the existing close guard and runtime teardown path.

## Migration

Existing persisted quick-terminal slots remain compatible. The only launch-time
behavioral migration is that visible persisted presentation restores as hidden.
Normal panes, pinned tabs, and terminal transcript snapshots keep their existing
restore behavior.
