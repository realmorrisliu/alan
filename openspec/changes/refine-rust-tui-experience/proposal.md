## Why

The Rust TUI (`crates/tui`) is the primary product entry point for the Alan agent — the macOS app is only a Ghostty host that runs it, so the TUI's quality is the product's quality. Today it is a bare ratatui skeleton: every protocol event is appended verbatim as a permanent transcript line (turn boundaries, resize notices, sequence-gap warnings, hydration counts, `{:?}`-formatted plans, raw `request_id`s), there is no activity indicator while the agent works, the composer lacks standard editing keys and input history, and there is no command or completion surface. This change brings the TUI to a Claude Code / Codex-class baseline.

This is **slice A of a four-slice program** to reach top-tier agent-TUI parity. The later slices depend on or extend this one: B (`add-structured-tool-rendering`), C (`introduce-auto-approve-policy`), D (`add-os-sandbox-enforcement`). Slice A has **no backend dependencies** and is shippable on its own.

## What Changes

- **Event display tiers**: classify every protocol event into permanent transcript content, ephemeral live-region status, or suppressed-to-log. Stop appending turn boundaries, resize notices, sequence-gap warnings, hydration internals, and `Connected to …` as permanent transcript cells.
- **Live region**: introduce a persistent bottom region holding (1) an activity line shown only while a turn runs (spinner + current action + elapsed + `esc to interrupt` + model), (2) a dynamic-height composer, (3) a context-sensitive hint line. Stream assistant text in the live region and commit to scrollback at line boundaries.
- **Thinking collapse**: render reasoning dimmed while streaming, collapse to a one-line summary on completion, with a keybinding to toggle expansion live.
- **Keyboard-only editing**: drop mouse capture entirely; adopt readline/emacs editing conventions (`Ctrl+A/E`, `Ctrl+W`, `Ctrl+U`, `Alt+←/→`, `Home/End`); add input history recall (`↑/↓`) **persisted across sessions** under `~/.alan`.
- **Command & completion surface**: `/` opens a data-driven client-command menu (compact, rollback, clear, help, quit, toggle-thinking); `$` triggers **inline** skill references sourced dynamically from the daemon skills catalog; `@` triggers **inline** workspace file-path completion. All three share one completion-popup infrastructure.
- **Plan rendering**: replace `{:?}` debug formatting with a real checklist; never render internal ids (`request_id`, tool ids) on screen.
- **BREAKING** (UX): mouse interactions are no longer captured by the TUI; terminal-native selection/copy is restored.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `rust-inline-tui`: adds event display tiers, the live activity region, thinking collapse, keyboard-only readline editing with persisted history, and the `/`·`$`·`@` command/completion surface; removes mouse capture and verbatim event-to-cell rendering.

## Impact

- Code: `crates/tui` (all modules — `app.rs`, `history.rs`, `ui.rs`, `composer.rs`, `terminal.rs`, `lib.rs`); new completion/command/history modules; reads daemon skills-catalog endpoint already present in the endpoint contract.
- Dependencies: a new on-disk input-history file under `~/.alan`.
- No protocol or daemon changes (those are slices B–D).
