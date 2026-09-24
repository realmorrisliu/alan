# Design: shell-like terminal interaction

## Current flow

`alan` attaches to `/agent/root` and `alan-terminal-ui` reads AgentFS files.
`FileBackedApp` reconciles output streams with tape; `TerminalSession` uses a
Ratatui inline viewport and `insert_before` to preserve committed output.
Today the viewport is the whole terminal. `draw` splits it into a history pane
and a fixed bottom live region. The live region prints internal attachment
copy, completion candidates, and the composer. That bottom anchoring is the
observed product mismatch.

## Change

Keep the Process/AgentFS execution path and current stream reconciler. Render a
compact Ratatui inline viewport at the end of terminal output:

1. Render recent transcript lines, then the `alan >` editable prompt.
2. Commit transcript lines that no longer fit above the viewport through the
   existing `Terminal::insert_before`/scrollback path and prune the same
   rendered prefix from the in-memory view.
3. Show the prompt immediately after the latest transcript line; do not fill
   unused terminal rows or pin the prompt to the screen bottom.
4. Size the inline viewport to the retained transcript and current input state.
   It grows as recent output or temporary candidates/forms need rows, and
   commits older transcript lines to scrollback at the terminal-height bound.
   When temporary UI closes, shrink back to the transcript and prompt.
5. Draw slash, `$`, and `@` candidates adjacent to the prompt using the existing
   completion state and keyboard behavior. Keep only the selected input and
   accepted transcript in terminal history.
6. Keep actionable failures in the visible transcript. Preserve detailed
   diagnostics in the existing error chain/logs; do not replace user messages
   with tracing-only output.

Use current `Composer`, completion, typed `HistoryCell`,
`drain_committed_scrollback`, and AgentFS watchers. No new TUI dependency,
execution layer, or stored UI session is needed.

## Ownership and non-goals

- Agent Runtime, Agent Machine, tape, rollout/checkpoint, and Process lifecycle
  remain their current owners.
- A renderer quit/detach ends the renderer only. It neither kills the Host nor
  resubmits a task on attach. Ctrl-D on an empty prompt follows this detach path;
  Ctrl-C continues to request interruption.
- This change does not alter current routing: ordinary and `!` entries
  continue to go to the Agent; slash commands remain local. Command routing and
  intent classification are outside this renderer presentation slice.
- `Stream` is currently append-only. This change verifies current offsets and
  visible read failures; it does not invent a retention policy or gap-recovery
  state that the owner cannot produce.
- Model discovery/selection is excluded here. ChatGPT's current catalog is a
  static Provider projection; the callable model is fixed by a Connection.
  The next slice must settle how a running Agent selects a different mounted
  Connection before implementing a model picker.

## Verification

- Unit tests assert transcript-before-prompt layout, scrollback prefix handling,
  completion accept/dismiss, Unicode/multiline editing, visible errors, and
  restoration on quit.
- Existing Root Agent replacement/reconciliation tests remain authoritative
  for no duplicate submission; do not weaken them.
- Live-accept in an ordinary terminal and Herdr with the current dev build.
