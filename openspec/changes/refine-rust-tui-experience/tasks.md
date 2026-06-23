## 1. Event display tiers

- [x] 1.1 Define a display-tier classification for every `Event` variant (permanent / ephemeral / suppressed) in the reducer layer
- [x] 1.2 Stop appending `TurnStarted`/`TurnCompleted`, resize, sequence-gap, hydration, and `Connected to …` as transcript cells; route them to `tracing` only
- [x] 1.3 Remove `request_id` and tool-call ids from all rendered output
- [x] 1.4 Replace `{:?}` plan rendering with a status-aware checklist renderer
- [x] 1.5 Tests: reducer assigns the correct tier per event; no internal ids in rendered lines

## 2. Live region

- [x] 2.1 Split rendering into committed scrollback vs a redrawn live region (activity line, composer, hint line)
- [x] 2.2 Implement the activity line (spinner + action + elapsed + `esc to interrupt` + model), shown only during a running turn
- [x] 2.3 Move running tools, recoverable warnings, and compaction/memory notices into the live region (never committed)
- [x] 2.4 Stream assistant text in the live region; commit to scrollback at completed line boundaries (via the viewport-drain commit path)
- [x] 2.5 Dynamic live-region height with an upper cap and internal scroll
- [x] 2.6 Tests: activity line appears/disappears with turn state; streaming + resize interleaving preserves content

## 3. Thinking collapse

- [x] 3.1 Render thinking dimmed while streaming; collapse to a one-line summary with duration on completion
- [x] 3.2 Add a session-wide thinking-expand toggle keybinding (Ctrl+R) and re-render
- [x] 3.3 Tests: collapse-on-final and toggle behavior

## 4. Composer: readline editing + history

- [x] 4.1 Rewrite composer editing with char-aware cursor, word ops (`Ctrl+W`, `Alt+←/→`), line ops (`Ctrl+A/E`, `Home/End`), kill-to-start (`Ctrl+U`)
- [x] 4.2 Implement persisted input history under `~/.alan` (load, append, adjacent-dedupe)
- [x] 4.3 Wire `↑/↓` history recall with edit-buffer preservation
- [x] 4.4 Tests: each editing op; history persists across simulated sessions

## 5. Command & completion surface

- [x] 5.1 Build a shared completion-popup component (filter + selection list above composer)
- [x] 5.2 Data-driven client-command registry for `/`; replace the hardcoded `==` command matching in `handle_submit`
- [x] 5.3 `$` inline skill references sourced from the daemon skills-catalog endpoint (with graceful degradation)
- [x] 5.4 `@` inline file-path completion from a workspace path index
- [x] 5.5 Tests: trigger routing; `/` runs local actions; `$`/`@` insert tokens into the turn; catalog-unavailable degradation

## 6. Keyboard-only / mouse

- [x] 6.1 Remove `EnableMouseCapture`/`DisableMouseCapture` and the unused mouse event arm
- [x] 6.2 Tests/manual: terminal-native selection works; no mouse capture

## 7. Verification

- [x] 7.1 `just verify` (fmt + lint + test + mock smoke)
- [ ] 7.2 Manual UI smoke against `Alan Dev.app` confirming clean transcript, activity line, completion popups, and native copy
