## Why

`alan-terminal-ui` now has a usable local file-backed mode for input, output,
requests, actions, and control commands, but the daemon-backed path still owns
several user-visible behaviors because the file surface does not yet project the
full runtime state. As long as thinking, plan updates, warnings, compaction
outcomes, and turn-lifecycle state live only in session events, file-backed TUI
cannot reach feature parity and the daemon compatibility path cannot be safely
removed.

## What Changes

- Project the remaining renderer-relevant runtime state into agent-owned files
  instead of keeping it daemon-event-only.
- Define durable file surfaces for plan state, warning/notice chronology,
  compaction and memory-flush outcomes, and renderer-visible turn activity.
- Extend the file-backed `alan-terminal-ui` path to hydrate and render those
  surfaces directly from `/agent/<pid>` without daemon session APIs.
- Retire daemon-backed-only UI features from `alan-terminal-ui` by making the
  file-backed renderer the complete local terminal path.

## Capabilities

### New Capabilities
- `agent-runtime-ui-file-surfaces`: durable file surfaces for renderer-visible
  runtime state that is not already covered by `io/output`, `requests/`, or
  `actions/`.

### Modified Capabilities
- `rust-inline-tui`: local terminal behavior changes from partial file-backed
  parity to complete renderer-host parity over file surfaces, enabling removal
  of daemon-only local UI behavior.

## Impact

- Affected code: `crates/agent-engine`, `crates/agentfs`, `crates/tui`,
  `crates/alan`, and related tests.
- Affected behavior: local terminal rendering, hydration, notices, activity, and
  planning/thinking visibility move from daemon session events to mounted agent
  files.
- Affected contracts: renderer-host/file-layout semantics for runtime UI state,
  plus `rust-inline-tui` local-mode requirements.
