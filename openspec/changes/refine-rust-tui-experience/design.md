## Context

`crates/tui` is a daemon-backed inline ratatui app (see `rust-inline-tui` spec). Its `SessionReducer` (`history.rs`) maps each `EventEnvelope` 1:1 onto a `HistoryCell` that is appended to `cells` and later drained into terminal scrollback (`app.rs::drain_committed_scrollback`). The render layer (`ui.rs`) draws two stacked `Paragraph`s with no styling. The composer (`composer.rs`) supports only char insert, backspace, left/right, and shift+enter. Mouse capture is enabled (`terminal.rs`) but no mouse events are consumed.

The architecture (inline model, daemon-backed, typed cells, scrollback commit, frame coalescing) is sound and is kept. What changes is *which* events become permanent content, the introduction of an ephemeral live region, and the input/command surface.

## Goals / Non-Goals

**Goals:**
- A clean transcript that contains only conversational substance.
- Unmistakable "the agent is working" feedback with a one-keystroke interrupt.
- Power-user input: standard readline keys, persisted history, discoverable commands, inline `$`/`@` references.
- Keyboard-only; terminal-native selection/copy preserved.

**Non-Goals:**
- Rich per-tool rendering / diffs (slice B — needs protocol changes).
- Approval-flow redesign and auto-approve policy (slice C).
- Any sandbox work (slice D).
- A modal (vim-style) editor or a separate command palette; no in-app mouse.

## Decisions

### D1. Three-tier event model
Introduce an explicit classification consumed by the reducer:
- **Permanent**: user message, assistant text, completed tool calls, fatal errors, plan snapshots.
- **Ephemeral (live region only, never committed)**: thinking stream, running tool, recoverable warnings/errors, compaction/memory-flush notices, turn activity.
- **Suppressed (tracing log only)**: `TurnStarted`/`TurnCompleted`, resize, sequence-gap, hydration counts, `Connected to …`.

Rationale: the current "append everything" model is the root cause of the noise. Alternative (a verbosity flag gating each cell) was rejected — it keeps the wrong default and still pollutes scrollback.

### D2. Live region as a separate render surface
Split rendering into committed scrollback (already terminal-owned) and a live region redrawn each frame. The live region owns: activity line (conditional), composer (dynamic height, capped with internal scroll), hint line. Assistant text streams into the live region and is committed to scrollback only at completed line boundaries.

Rationale: matches the inline model and eliminates the commit/redraw duplication risk in the current `append_text`-then-drain path.

### D3. Thinking collapsed by default
Thinking renders dimmed in the live region while streaming; on `is_final` it collapses to `✓ thought for {duration}`. A keybinding toggles a session-wide "expand thinking" state that re-renders collapsed entries inline.

### D4. Readline editing + persisted history
Replace the ad-hoc composer key handling with a readline-style editor (grapheme-aware cursor, word ops, line ops, kill-to-start). Input history is an append-only file under `~/.alan` (e.g. `~/.alan/tui_history`), loaded at startup, deduplicated on adjacent repeats, recalled with `↑/↓`.

### D5. Unified completion infrastructure; two command species
A single completion-popup component (filter + selection list, rendered above the composer) is driven by a trigger registry:
- `/` → **client commands** from an in-TUI data-driven registry; selecting runs a local `AppAction`, never sent to the agent.
- `$` → **inline skill references** whose candidates come from the daemon skills-catalog endpoint; selecting inserts a `$skill-name` token into the message; the whole message is submitted as one turn (the daemon/runtime resolves the references — text-token transport for now).
- `@` → **inline file references** from a workspace path index; selecting inserts the path token.

Rationale: separating client commands (`/`) from skill references (`$`) keeps "control the TUI" and "summon a capability" visually distinct; sourcing skills from the catalog means new skills appear automatically without TUI edits.

### D6. Always-available interrupt
`Esc` issues `AppAction::Interrupt` at any time during a running turn; the activity line advertises it. This is the human's stop control and must remain responsive under streaming load.

## Risks / Trade-offs

- [Dropping mouse capture changes behavior users may rely on] → It restores terminal-native selection (the more common expectation) and no mouse events were consumed anyway; documented as a UX-breaking but net-positive change.
- [Live-region line-boundary commit can mis-handle wrapping at resize] → Reuse and extend the existing scrollback-prune width-reflow tests; add tests for streaming + resize interleaving.
- [Skill candidates require a daemon round-trip] → Cache the catalog per session and refresh lazily; degrade to no `$` candidates (still allow free typing) if the endpoint is unavailable.
- [Persisted history may capture sensitive input] → Store under `~/.alan` with user-only permissions; provide `/clear` semantics and document the file location.

## Migration Plan

No data migration. The change is internal to the TUI. Roll out behind the normal build; rollback is reverting the change. Existing `rust-inline-tui` scenarios continue to hold (daemon-backed, scrollback commit, resize reflow); new scenarios are additive except the mouse-capture removal.

## Open Questions

- Exact keybinding for thinking toggle (`Ctrl+R` vs `Tab`) — resolve during implementation against composer key conflicts.
- Whether `/clear` also truncates the persisted history file or only the on-screen transcript.
