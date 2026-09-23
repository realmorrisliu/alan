## Why

Bare `alan` currently attaches to the Alan OS Host but enters only the generic
line-oriented StdioDriver. The Host starts a Root Agent Process with its
configured generation Connection, and `alan-terminal-ui` already renders a
mounted Agent Process through AgentFS files. Product boot does not currently
register the existing Core Tools in the Root Agent's ToolRegistry, so explicit
`!` shell requests cannot work through the intended governed `bash` Tool. The
missing pieces are small composition paths joining the terminal CLI and the
existing Core Tools to that Agent; another evaluator, Process manager, or
command runtime is not needed for the first usable task loop.

## What Changes

- When both stdin and stdout are terminals, bare `alan` attaches its mounted
  namespace to the existing `/agent/root` through `alan-terminal-ui`.
- Product Host boot registers the existing Core Tool catalog and implementations
  in the Root Agent ToolRegistry; it does not add a new Tool or broaden that set.
- In that terminal UI, ordinary submitted text is one task for the Root Agent.
  A leading `!` explicitly requests the exact remainder as a shell command via
  the existing `bash` Tool; it does not bypass Tool governance, Host Mounts,
  approvals, or sandbox boundaries.
- When stdin/stdout are redirected, read stdin as one task, write only the
  final answer to stdout, diagnostics to stderr, and report success/failure in
  the exit code. Keep file-native Shell operations behind explicit input such
  as `!` rather than interpreting arbitrary task text as commands.
- Use AgentFS Process IO for input and incremental output. Ctrl-C interrupts the
  current Agent turn through `/agent/root/machine/ctl`, not Kernel Process
  control, and a subsequent task can use the still-running Root Agent.
- After a task submission, track the Root Agent PID until the turn settles;
  re-open file-backed streams if the Process changes asynchronously, without
  resubmitting the task or discarding the renderer's prior transcript.
- Closing the renderer closes its own file streams only; it does not stop the
  shared Host or Root Agent.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `alan-shell`: define TTY Agent REPL and one-shot redirected-IO behavior.
- `alan-renderer-host-contract`: define the minimal Root Agent attachment,
  explicit `!` shell intent, incremental IO, turn-scoped interrupt, and
  detach-without-shutdown behavior.

## Non-Goals

- Do not add an Alan Shell Evaluator Process, `run` Tool, parser, script
  language, generic executable packaging, binfs requirement, or editfs
  execution/materialization contract.
- Do not spawn a new Agent for each terminal invocation or add another
  execution manager, conversation object, namespace root, or Herdr-specific
  runtime. Keep the attached Host as the only Host lifecycle owner.
- Do not change Agent Machine generation, add typed evaluation/Jev, relax
  Connection or Tool governance, or infer Host Mount grants from cwd/pane state.
- Do not promise cross-Host recovery, saved stream offsets, or full terminal UX
  conformance. Rebinding after a live Root Agent PID change is not durable
  cross-Host recovery; broader detach/reattach behavior remains later roadmap
  work if the tracer bullet shows a real gap.

## Impact

The composition changes belong in `crates/alan/src/main.rs` and
`crates/os-host/src/boot.rs`: the CLI uses the existing `alan-terminal-ui`
renderer for a TTY and a one-shot Agent task for redirected input; product Host
boot registers the existing Core Tools for the Root Agent. The Rust
architecture ratchet records only these two root-composition edges, with no
new external package or Tool implementation. A focused renderer test will
lock down Ctrl-C mapping, and ordinary-terminal/Herdr acceptance will validate
real read-only Agent tasks, explicit `!` shell use, incremental output,
cancellation, and continued use.
