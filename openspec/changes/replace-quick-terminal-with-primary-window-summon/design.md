## Context

Alan currently has a single primary macOS shell window and also carries a
standalone Quick Terminal feature: shell action IDs, control commands, a global
quick-terminal slot, detached Peak presentation, promotion into a Space,
terminal lifecycle handling, manifest restore data, and focused tests. That
feature creates a second terminal ownership path at the same time the shell
product is moving toward one authoritative primary shell window.

The replacement is intentionally macOS-only. The action is about finding,
reopening, moving, activating, and focusing the native primary window. It is not
a shell workspace mutation and should not live in Rust shell core or the shell
action registry.

## Goals / Non-Goals

**Goals:**

- Remove standalone Quick Terminal product behavior and implementation surface.
- Reuse the former global Quick Terminal shortcut for Primary Window Summon.
- Preserve the selected shell Space, Tab, PaneSlot, split layout, and mounted
  content during summon.
- Reopen or create the single primary shell window when the app is already running
  but the primary window is closed.
- Best-effort move or summon the primary window to the user's current macOS
  Space/display, then activate the app and focus the current selected content.
- Decode and discard legacy quick-terminal restore data without migrating it to
  a tab or preserving it as hidden runtime state.
- Keep normal terminal close, transcript snapshot, and manifest behavior for
  ordinary terminal content.

**Non-Goals:**

- No detached Peak, quick-terminal NSPanel, or floating terminal window.
- No global quick-terminal runtime slot, promotion flow, or hidden session.
- No compatibility aliases for `quick_terminal.*` control commands or
  `shell.quick_terminal.*` shell actions.
- No multiple primary shell windows.
- No cross-platform Rust core contract for window summoning.
- No migration of old Quick Terminal content into regular tabs during upgrade.

## Decisions

### 1. Make summon an app/window command

Primary Window Summon is owned by the macOS app shell owner and native command
layer. The command locates the process-scoped primary shell owner, opens the
`main` window scene if needed, asks AppKit to bring that window to the current
desktop context when possible, activates the app, and restores focus into the
currently selected content.

Alternative considered: keep a shell action such as
`shell.quick_terminal.toggle` and point it at the main window. That would keep a
misleading shell workspace command alive and make Rust shell core responsible
for native window placement.

### 2. Preserve workspace selection rather than creating a terminal

Summon does not select a different shell Space, create a tab, create a pane, or
force terminal content into focus if the selected content is not terminal. It
focuses the window/view, and for selected terminal content it requests terminal
input focus through the existing terminal runtime focus path.

Alternative considered: always open or focus a terminal when the shortcut is
pressed. That recreates the Quick Terminal mental model and would surprise users
who intentionally left Alan on another content type.

### 3. Delete Quick Terminal state instead of migrating it

On load, old manifests may still contain a `quick_terminal` record. The new
contract reads enough to tolerate those files, discards the record during
materialization, and omits it from all future writes. It does not create a tab,
runtime, transcript restore, or hidden session from that data.

Alternative considered: promote old Quick Terminal content into a normal tab on
first launch. The product decision is to remove the feature without
compatibility migration, and upgrades quit running sessions, so automatic
promotion would preserve a behavior the user no longer wants.

### 4. Remove Quick Terminal from Rust shell core

Because summon is native window behavior, Rust shell core should not retain
Quick Terminal actions, reducer operations, model fields, manifest restore
payloads, FFI commands, or tests except for narrow legacy decode tolerance where
needed to read existing files.

Alternative considered: keep the Rust core fields as dormant compatibility
state. That keeps the largest duplicate logic path alive and prevents the core
from being the clean authority for real shell workspace state.

### 5. Keep failure behavior best-effort

macOS Space movement is not a fully reliable public API surface. The command
should attempt the current active Space/display placement path available to the
app; if it cannot guarantee movement, it still activates the app, brings the
primary window forward, and focuses selected content without mutating workspace
selection.

Alternative considered: fail the command when Space movement cannot be proven.
That would make the shortcut unreliable for users and tests because AppKit and
Mission Control behavior varies by OS and user settings.

## Risks / Trade-offs

- **Risk**: Hidden Quick Terminal restore data caused previous stable launch
  crashes; a partial cleanup could keep startup paths alive. **Mitigation**:
  add tests that old visible and hidden `quick_terminal` records decode, produce
  no runtime/panel state, and are omitted from the next manifest write.
- **Risk**: Removing shell action IDs may break tests, menus, automation, or
  FFI paths that still enumerate quick-terminal actions. **Mitigation**: make
  the removal explicit in action registry and build/test specs, then update all
  focused contract checks in the same implementation.
- **Risk**: AppKit cannot always move a window to the active Space. **Mitigation**:
  specify best-effort movement plus deterministic activation/focus fallback.
- **Risk**: Shortcut ownership can become split between old shell registry and
  new app command. **Mitigation**: define exactly one owner: macOS app/window
  command routing; shell registry and Rust core must not expose Quick Terminal
  aliases.

## Migration Plan

1. Remove Quick Terminal command/action/control registration from Swift and Rust
   shell paths.
2. Add a macOS app-level Primary Window Summon command and wire the former
   shortcut/menu surface to it.
3. Remove Peak presenter/window/view code and quick-terminal ShellHostController
   state transitions.
4. Remove quick-terminal model, reducer, FFI, manifest write, and control-plane
   DTO ownership from Rust shell core and Swift adapters.
5. Keep only legacy manifest decode tolerance needed to discard old
   `quick_terminal` records safely, then write manifests without that field.
6. Replace Quick Terminal tests with primary-window summon and legacy-discard
   tests.

Rollback is source rollback. There is no data migration to reverse because old
quick-terminal records are discarded and future manifests omit them.

## Manual Verification

Before merge, run one headed macOS smoke for the native window behavior that
cannot be fully proven by static contract checks:

1. Launch Alan with one primary shell window containing an active terminal pane.
2. Place the Alan window on Space A, switch the user to Space B, then invoke the
   former Quick Terminal shortcut.
3. Verify Alan activates, the same primary shell window is brought to the current
   desktop context when AppKit can move it, and the selected terminal receives
   input focus.
4. Close the primary window while the app keeps running, invoke the shortcut
   again, and verify Alan opens the single primary window instead of creating a
   detached terminal runtime.
5. On a setup where Space movement is blocked or unreliable, verify the fallback
   still activates Alan, brings the primary window forward, and preserves the
   selected Space, Tab, PaneSlot, split layout, and content runtime identity.

## Open Questions

- None for the initial implementation. The command is macOS-only, targets the
  single primary shell window, preserves workspace selection, and uses
  best-effort current-Space/display movement.
