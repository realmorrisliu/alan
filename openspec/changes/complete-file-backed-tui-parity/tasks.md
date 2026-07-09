## 1. Runtime UI file surfaces

- [x] 1.1 Add the `machine/ui/` subtree and `machine/ui/events` stream to AgentFS with in-band contracts for renderer-visible runtime state.
- [x] 1.2 Project runtime activity, plan snapshots, renderer-safe thinking state, warnings, and compaction or memory-flush notices into the new UI files from `alan-agent-engine`.
- [x] 1.3 Add focused AgentFS and runtime tests proving the new UI snapshots hydrate correctly and the event stream can be tailed/resumed by offset.

## 2. File-backed terminal parity

- [x] 2.1 Teach file-backed `alan-terminal-ui` to hydrate and watch `machine/ui/*` so live activity, thinking, plan state, and notices no longer depend on daemon session events.
- [x] 2.2 Keep the local file-backed interaction baseline aligned with the daemon-backed path, including pending input handling, completion, scrollback, and live-region display-tier behavior.
- [x] 2.3 Remove or demote the daemon-backed local terminal path once file-backed local mode covers the same user-visible behavior.

## 3. Verification and cleanup

- [x] 3.1 Run focused Rust tests for AgentFS/runtime UI files, file-backed terminal rendering, and local `alan` launch behavior.
- [x] 3.2 Review the remaining local terminal code paths and delete obsolete daemon-only local helpers that are no longer part of the contract.
- [x] 3.3 After merge, sync `agent-runtime-ui-file-surfaces` and updated `rust-inline-tui` specs into `openspec/specs/` before archiving the change.
