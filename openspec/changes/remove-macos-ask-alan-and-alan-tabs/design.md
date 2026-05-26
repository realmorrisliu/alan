## Context

Alan's macOS shell currently contains two overlapping agent-oriented surfaces:

1. A floating `Ask alan...` command input opened from the sidebar, menu, or
   `Command-P`.
2. A first-party `New alan tab` launch mode that creates terminal panes whose
   launch target is `.alan`.

The accepted product direction is terminal-first. Alan should remain available
inside normal terminal panes through CLI commands and terminal-launched agent
processes, but the macOS app should not expose a separate first-party alan tab
type or floating Ask alan command UI. The user confirmed there is no need to
preserve legacy `.alan` tab compatibility.

## Goals / Non-Goals

**Goals:**

- Remove Ask alan/floating command input from visible macOS shell UI.
- Remove `Command-P` as an Alan-owned command input shortcut.
- Remove `New alan tab` from menus, sidebar actions, command vocabulary,
  shell action registry, App Intents, and automation helpers.
- Remove the macOS app's `.alan` tab launch mode and automatic alan-tab runtime
  branch.
- Preserve CLI commands and terminal-launched agent metadata.
- Update OpenSpec and focused contract tests so the removed surfaces cannot be
  reintroduced by accident.

**Non-Goals:**

- Do not remove `alan ask`, `alan chat`, daemon sessions, LLM provider support,
  or runtime APIs outside the macOS shell product surface.
- Do not add compatibility shims for old `.alan` tabs, old App Intents, old
  automation requests, or old workspace manifests.
- Do not replace Ask alan with another palette, search box, launcher, or agent
  overlay in this change.
- Do not change user-launched agent detection in ordinary terminal panes.

## Decisions

### Delete the product capability, not just the buttons

The implementation should remove the macOS shell's Ask alan and alan-tab domain
paths from UI, action descriptors, commands, automation, and runtime launch
resolution. Hiding buttons would leave dead capabilities behind in automation
or tests and would make the feature easy to revive accidentally.

Alternative considered: leave `.alan` launch target internally and only remove
visible UI. This is lower risk in the short term, but it contradicts the user's
goal to treat the feature as if it never existed.

### Keep CLI and terminal activity metadata

The removal boundary is the macOS shell product surface. CLI commands remain
valid, and terminal metadata can still recognize Alan/Codex/agent processes
started by the user in normal terminal panes. This keeps the app useful for
terminal-first agent workflows without maintaining a special alan tab type.

Alternative considered: remove all references to alan agent activity metadata.
That would overreach into terminal observation and break useful background
activity semantics that are not part of Ask alan or New alan Tab.

### No legacy compatibility path

The change should not add explicit unsupported responses, migration code, or
manifest fallback logic for `.alan` launch targets. If implementation code can
delete the enum case and dependent branches cleanly, it should do so. Any old
fixture or test that depended on `.alan` tabs should be rewritten or removed.

Alternative considered: keep decode compatibility and return
`unsupported_launch_target`. The user explicitly rejected this because there is
no real legacy state to preserve.

### `Command-P` becomes unowned by Alan shell

After removal, Alan should not bind `Command-P` to a replacement action. The
shortcut should fall through to the system, focused text control, or nothing,
depending on normal macOS responder behavior.

Alternative considered: reuse `Command-P` for Find, quick terminal, or a
terminal command. That would create a new behavior decision unrelated to the
removal.

## Risks / Trade-offs

- Removing enum cases can create a broad compile fallout -> work from outer
  entry points inward and keep terminal-launched agent metadata separate from
  launch targets.
- Tests may conflate `.alan` launch target with agent activity detection ->
  update tests to use normal terminal launch targets plus agent metadata where
  the behavior being tested is activity semantics.
- OpenSpec has several older positive requirements for command input -> replace
  them with deletion guards instead of leaving stale contracts.
- Deleting App Intents is a breaking automation change -> accepted because the
  feature is being removed without compatibility.

## Migration Plan

1. Remove visible Ask alan and New alan Tab entry points.
2. Remove floating command input state/view plumbing.
3. Remove `newAlanTab` and command-input actions from action and workspace
   command models.
4. Remove Create Alan Tab App Intent and automation helper paths.
5. Remove `.alan` launch target/runtime branch and update affected tests.
6. Update shell contract scripts to reject reintroduced Ask alan, Command-P
   command input, New alan Tab, and default `alan chat` launch paths.

Rollback is simply reverting this change before release. No data migration is
planned or required.

## Open Questions

No blocking questions. During implementation, any fixture that exists only to
exercise `.alan` tab creation should be removed rather than migrated.
