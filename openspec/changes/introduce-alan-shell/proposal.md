## Why

The north-star milestone is talking to an agent from Alan Shell — and the point
of the Plan 9 model is that this is *not* a chat feature: it is ordinary file
operations (`tail` the output, write the input) over the namespace. This change
defines `alan-shell` as a general namespace shell that operates only through aP
file operations and knows nothing about agents. "Talking to an agent" falls out
of `cat`/`tail`/`echo` over `/agent/<pid>` files, proving the uniform-tooling
claim (ADR-0024 D4).

## What Changes

- Add `alan-shell`, a client that speaks aP (the `alan-ap` protocol) and operates
  the namespace: walk/list, read (`cat`), write (`echo >`), tail (blocking
  watch), and spawn. It depends only on the protocol — never on a file server or
  backend.
- Keep the shell agnostic: it has no agent-specific command, mode, or `attach`
  sugar. Conversing with an agent is composing `echo > /agent/<pid>/io/input`
  with `tail /agent/<pid>/io/output`, exactly as for any process's IO.
- Ship a minimal line-oriented stdio driver first (M1). Ratatui rendering remains
  `alan-terminal-ui`'s job and is deferred (ADR-0025).
- Support concurrent streaming: tailing a stream while still accepting input.

## Capabilities

### New Capabilities

- `alan-shell`: the general namespace shell — aP-only builtins (list, read,
  write, tail, spawn), a line-oriented stdio driver, and concurrent stream
  tailing, with no agent-specific behavior.

### Modified Capabilities

- None.

## Impact

- Depends on `define-plan9-kernel-substrate` (aP protocol, namespace, blocking
  reads). To talk to an agent it also needs `alan-agentfs` (`/agent`) and, behind
  it, `add-llm-file-server`.
- Reaches the north-star milestone once `alan-agentfs` and `alan-llmfs` are in
  place: type in `alan-shell`, get a streamed agent response — all through files.
- ADRs: implements the client layer of ADR-0025 and ADR-0024 D4 (uniform
  operation via files and `ctl`); `alan-terminal-ui` later renders this shell.
