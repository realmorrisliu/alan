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
  renderer attached to `/agent/root`; keep StdioDriver for redirected IO.
- [ ] 2.2 Verify incremental AgentFS output, Ctrl-C turn interruption,
  subsequent input, and renderer exit without killing the shared Host or Agent.
- [x] 2.3 Verify unavailable Connection behavior: a clear error appears before
  provider dispatch, the renderer accepts another submission, and the Host and
  Root Agent remain available.
- [ ] 2.4 Verify a known-content read-only Host Mount, an unmounted boundary,
  and existing Tool failure handling without adding a second test-only path.
- [ ] 2.5 In an ordinary terminal and a Herdr sibling pane, record the current
  build and actual input/output for two successive tasks, cancellation, and a
  further successful task on the same Root Agent.

> Live acceptance status (2026-09-23): the current build opens correctly in an
> ordinary TTY and a Herdr sibling pane, and both show the explicit
> unavailable-Connection error. `alan connection list` reports no profiles in
> either stable or dev. Successful model tasks, read-only project inspection,
> and live cancellation therefore remain unverified; do not treat the error
> path as a completed tracer bullet.

## 3. Verification and delivery

- [x] 3.1 Run focused Rust tests, the inline-TUI contract, Repository Quality
  Gate, strict OpenSpec validation, and diff checks.
- [ ] 3.2 Complete ready/Codex review, root-cause fixes and resolution, and all
  current-head checks; then merge the implementation.
- [ ] 3.3 Sync only implemented deltas. Before archiving, hand any reliability
  work revealed by acceptance to an active change and keep the roadmap link live.
