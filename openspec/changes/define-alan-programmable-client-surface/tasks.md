## 1. Trace and replan

- [x] 1.1 Trace bare `alan` through LocalAttachment, StdioDriver, Root Agent,
  generation Connection, governed Tools, and AgentFS IO; record the missing
  composition edge.
- [x] 1.2 Replace the superseded proposal/design and retained deltas with the
  bounded TTY-to-Root-Agent tracer bullet and explicit input/access rules.
- [x] 1.3 Keep each behavior with its existing owner; remove editfs, `run`,
  executable packaging, and unrelated UI/runtime requirements from this slice.

## 2. Minimal usable Agent loop

- [x] 2.1 Route bare `alan` with TTY stdin/stdout to the existing file-backed
  renderer attached to `/agent/root`; use Agent task semantics for redirected
  IO rather than the interactive renderer.
- [x] 2.2 Verify incremental AgentFS output, Ctrl-C turn interruption,
  subsequent input, tail rebinding after a Root Agent PID change during both a
  local turn and idle reattachment (including replacement history merge), and
  renderer exit without killing the shared Host or Agent.
- [x] 2.3 Verify unavailable Connection behavior: a clear error appears before
  provider dispatch, the renderer accepts another submission, and the Host and
  Root Agent remain available.
- [x] 2.4 Verify a known-content read-only Host Mount, an unmounted boundary,
  and existing Tool failure handling without adding a second test-only path.
- [x] 2.5 In an ordinary terminal and a Herdr sibling pane, record the current
  build and actual input/output for two successive tasks, cancellation, and a
  further successful task on the same Root Agent.
- [x] 2.6 Route `!<command>` through the existing governed `bash` Tool by
  registering only the existing Core Tool set in product Root Agent boot;
  verify the exact set and shell behavior, including preserving namespace-path
  data passed to `echo`/`printf` or a Git commit message while projecting file
  operands and redirection targets.
- [x] 2.7 Keep recoverable Agent errors in the rendered transcript.
- [x] 2.8 Implement and verify terminal-mode selection, one-shot redirected
  stdin/stdout/stderr, Root Agent PID rebinding (including tail closure before
  PID polling and a temporary missing PID during supervised restart), and
  exit-code behavior. Keep waiting for the task
  outcome without a client-only timeout; persist generation failures as UI
  terminal-error events before idle without requiring tape; recognize
  `Running`/terminal-error events before tape persistence; prefer terminal
  errors over intermediate assistant content; retain Ctrl-C until `Running`
  confirms acceptance before sending interruption; exclude concurrent one-shot
  writers; and fail closed when replacement tape/UI history cannot correlate
  the result to this submission. Open the tape/UI tail pair against one
  concrete PID and retry the pair if it changes during attachment.

> Live acceptance status (2026-09-23): an explicit read-only synthetic Host
> Mount and `bash` read returned the expected sentinel; a leading `!` reached
> the existing `bash` Tool and succeeded while that Process-scoped grant was
> active. After restarting dev Host, a fresh Herdr TUI reattached and rendered
> a response. A new approved read-only mount then exposed that native bash
> validation treated `/mnt/<mount>` as outside its Host Mount. The shared path
> guard now maps namespace paths to their backing Host paths for authorization,
> and native shell execution maps them before dispatch; sandbox and Tool tests
> pass. A rebuilt dev Host then returned the sentinel from the same `!cat`; the
> read-only grant was revoked afterward. Unmounted-path and read-only-write
> denials pass sandbox tests. A `Ctrl-Q` renderer exit left dev Host Ready and
> Connections usable; a new renderer attached and completed another task. A
> separate task had exposed a PID-update race: the Process can change just
> after the post-submit PID check, leaving TUI tails attached to the prior
> Process. TTY and one-shot paths now poll and recover from that replacement;
> local tests pass. `bash` still requires an explicit Host execution adapter,
> so `!` does not bypass a Host Mount. A 100-line live response exposed
> interleaved terminal rows: committed history had been written directly to
> stdout between Ratatui draws. The renderer now uses Ratatui's inline viewport
> and scrollback insertion; the rebuilt TUI displayed the ordered response
> legibly in both visible output and scrollback, and `Ctrl-Q` returned to fish
> without stopping dev Host. Live Host reattachment after an active Root Agent
> replacement remains unverified. A
> one-shot command returned the exact Agent answer on stdout with empty stderr
> and exit 0; empty stdin produced a stderr diagnostic and exit 1. The TUI
> integration test also replaces the Root Agent PID after accepting one-shot input and
> recovers the matching answer once from the new Process. The interactive
> watcher integration now changes the Root Agent PID and verifies the renderer
> reattaches, preserves its earlier transcript, and hydrates the current turn
> from the replacement Process. Current build `bef854e3` was tested on dev on
> 2026-09-23: ordinary PTY `w4X:p8` returned `ALAN_RESTART_CHECK_ONE`,
> `ALAN_RESTART_CHECK_TWO`, and `ALAN_AFTER_GENERATION_CANCEL` after a Ctrl-C
> cancellation; Herdr sibling `w4X:p9` returned two successive markers, then
> `ALAN_HERDR_SIBLING_AFTER_CANCEL` after the same cancellation sequence. Both
> clients attached to the same Root Agent. A separate `!sleep 30` attempt
> returned `Tool Process has no explicit Host execution adapter` and was not
> counted as cancellation evidence.
> On 2026-09-24, after restarting only dev Host, a fresh Herdr TUI returned
> `ALAN_POST_RESTART_OK`; redirected one-shot returned
> `ALAN_ONE_SHOT_POST_RESTART_OK` with exit 0. Stable Host PID was unchanged.
> The one-shot integration test also closes old tails while the Service
> Manager PID is temporarily `0`, then publishes a replacement with the
> correlated answer; the waiter remains pending and recovers that answer.

> The watcher integration also replaces an idle Root Agent after another
> client has completed a task; reattachment preserves the prior transcript and
> appends the replacement's completed turn exactly once.

## 3. Verification and delivery

- [x] 3.1 Run focused Rust tests, the inline-TUI contract, Repository Quality
  Gate, strict OpenSpec validation, and diff checks.
- [ ] 3.2 Complete ready/Codex review, root-cause fixes and resolution, and all
  current-head checks; then merge the implementation.
- [ ] 3.3 Sync only implemented deltas. Before archiving, hand any reliability
  work revealed by acceptance to an active change and keep the roadmap link live.
