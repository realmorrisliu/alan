## Context

The accepted terminal session snapshot work persists bounded transcript text in
the workspace manifest and seeds restored terminal content on restart. The
current product behavior renders that restored text above the live terminal in
`RestoredTerminalTranscriptView`. This is useful because it is honest about the
prior process being gone and keeps the new shell clean, but the current panel is
too disconnected from the terminal surface:

- restored text is laid out as a narrow SwiftUI text block instead of matching
  the live terminal text column;
- the panel remains visible after user clear actions because it is not part of
  the real terminal buffer;
- the restored snapshot can remain in shell state and runtime cache after the
  user has dismissed the visual context.

This change keeps the separate panel model and polishes the boundaries around
it. It does not attempt to serialize Ghostty state or replay the snapshot into
the new PTY.

## Goals / Non-Goals

**Goals:**

- Preserve the restored transcript panel as a distinct visual region above the
  live terminal.
- Make restored transcript text visually line up with terminal text: same
  leading column, terminal-like monospace metrics, row rhythm, full-width
  leading layout, and readable foreground treatment.
- Ensure restored panel height is stable and bounded by the restored row limit
  used by the view.
- Let supported terminal clear intents remove the restored panel.
- Remove cleared restored transcript data from in-memory shell state, runtime
  restored-cache state, and future manifest writes.
- Keep normal terminal input delivery unchanged when clear actions are used.
- Keep implementation scoped to restored transcript presentation and dismissal.

**Non-Goals:**

- Do not replay restored transcript text into the new PTY.
- Do not claim the prior PTY, foreground process, alternate-screen app, or
  renderer state survived app restart.
- Do not add a full terminal buffer restore API unless Ghostty already exposes a
  reliable clear or restore seam needed by the implementation.
- Do not add a preferences surface for restored transcript behavior.
- Do not redesign terminal chrome, pane title bars, split layout, or sidebar
  behavior.

## Decisions

### Restored context remains a distinct panel

Alan will keep restored transcript context as a separate panel above the live
terminal. The panel may use a slightly distinct background and a subtle divider
so users can tell it belongs to the previous app session. It must not become a
card-heavy surface, a warning banner, or a debug disclosure UI.

Alternative considered: replay restored text into the live PTY. That gives the
most natural `clear` behavior, but it makes old output look like fresh shell
output and can muddy the user's mental model after restart.

### Text layout follows terminal metrics

The restored panel will render transcript text with terminal-like monospace
metrics and leading alignment. Its text origin should match the live terminal's
text origin as closely as the current SwiftUI/AppKit composition permits. It
should use full-width leading layout and horizontal scrolling rather than
wrapping or centering a narrow text block.

The panel height remains derived from bounded restored rows, but the panel
should not make the live terminal look split into mismatched typography zones.
The visual difference should come from background and a separator, not from
different text rhythm.

Alternative considered: keep the current small SwiftUI text styling and only
add clear behavior. That solves persistence but leaves the main perceived
quality issue visible.

### Restored transcript dismissal is content-scoped

Clearing the restored panel is a content-level state mutation, not local view
state. The mutation removes `payload.terminal.transcriptSnapshot` for the
mounted terminal ContentInstance, tells the runtime service to evict any
restored transcript cached for the same content ID, and schedules normal shell
state persistence. After dismissal, tab switches, split view reconstruction,
window restoration, and subsequent manifest saves must not reintroduce the old
snapshot.

Alternative considered: hide the SwiftUI view with local `@State`. That is
visually fast but stale restored data remains in shell state and can reappear
after view reconstruction or restart.

### Clear intents share one dismissal path

The following clear intents should call the same content-scoped dismissal path:

- terminal `Ctrl-L` in the focused terminal, while still forwarding the key to
  Ghostty;
- typed `clear` submitted at the prompt when Alan can recognize the command
  from reliable terminal input or semantic command boundaries;
- Alan's explicit Clear command, including Cmd-K or menu/command routing once
  that action exists.

Raw application-emitted clear-screen escape sequences may also dismiss the
restored panel if Ghostty exposes a reliable screen-clear or scrollback-reset
signal. The implementation should not infer arbitrary renderer internals from
unrelated scrollback metrics or parse terminal output text as a fallback.

Alternative considered: dismiss on the first normal keypress. That removes the
panel quickly but loses useful context when the user types a command while still
referencing the restored output.

### Runtime cache eviction mirrors shell state

`TerminalRuntimeRegistry` and `TerminalRuntimeService` will expose a narrow
restored transcript eviction API keyed by terminal content ID. The shell state
mutation owns the user-visible truth; the runtime eviction prevents an existing
or future surface handle from reseeding itself from an already-cleared snapshot.
Fake runtime services used by tests should implement the same behavior so cache
eviction is verifiable without launching Ghostty.

Alternative considered: only remove the manifest payload. That may still leave
an already-created runtime handle carrying seeded restored transcript metadata
or fallback ring-buffer state.

## Data Flow

1. Workspace manifest materialization creates terminal ContentInstances that may
   include `payload.terminal.transcriptSnapshot`.
2. The terminal pane view reads the current shell state and renders
   `RestoredTerminalTranscriptView` only while the mounted content still has a
   transcript snapshot.
3. `bootProfile(for:)` may seed the runtime service as it does today, but
   seeding does not make the panel state authoritative.
4. A supported clear intent resolves to the focused pane's mounted terminal
   content ID and calls the shared restored transcript clear mutation.
5. The mutation removes the snapshot from shell state, evicts the runtime cache
   for that content ID, publishes the updated shell state, and persists the
   workspace manifest through the existing persistence path.
6. The view re-renders without the restored panel. The live terminal still
   receives the clear key or command action when appropriate.

## Error Handling

- If a clear intent has no focused pane or no mounted terminal content, it
  should no-op for restored transcript dismissal and continue normal terminal or
  shell command handling.
- If runtime cache eviction cannot find a cached restored transcript, dismissal
  still succeeds because shell state is the visible source of truth.
- If manifest persistence fails after dismissal, Alan should surface or record
  the same persistence diagnostics used by other shell state saves; the UI does
  not re-show the panel in the current process solely because persistence
  failed.
- If typed `clear` recognition is not reliable for a terminal mode or IME
  composition state, the implementation should avoid false positives and rely on
  `Ctrl-L` plus explicit Clear command for that case.

## Testing

- Add shell state or controller tests proving a restored transcript clear
  mutation removes the snapshot from the mounted terminal content and from the
  persisted manifest.
- Add runtime service tests proving restored-cache eviction prevents a reused
  content ID from inheriting old restored transcript state.
- Add terminal host or controller tests proving `Ctrl-L` and the explicit Clear
  command dismiss the restored panel while preserving normal terminal delivery.
- Add typed `clear` coverage only if recognition is implemented through a
  reliable input or semantic-command seam.
- Add focused view/model tests for restored panel layout expectations:
  full-width leading alignment, terminal-like monospace typography, bounded
  height, and no centered/narrow text block.
- Add running-app visual verification in the dev channel after implementation:
  restore a transcript, confirm panel text aligns with the live terminal text
  column, invoke clear, and confirm the panel disappears and does not return
  after relaunch.

## Rollout

This is a local macOS client behavior change. Old manifests remain compatible
because transcript snapshots are already optional. The first implementation can
ship without a preference because clearing is explicit and the panel remains
bounded.
