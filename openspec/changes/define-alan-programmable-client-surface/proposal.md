## Why

Bare `alan` currently attaches to the Alan OS Host but enters only the generic
line-oriented StdioDriver. Alan already starts a Root Agent Process with its
configured generation Connection and governed Tools, and `alan-terminal-ui`
already renders a mounted Agent Process through AgentFS files. The missing piece
is one small composition path joining the terminal CLI to that existing Agent
and renderer; another evaluator, Process manager, or command runtime is not
needed for the first usable task loop.

## What Changes

- When both stdin and stdout are terminals, bare `alan` attaches its mounted
  namespace to the existing `/agent/root` through `alan-terminal-ui`.
- In that terminal UI, submitted text is one natural-language task for the Root
  Agent. It is not parsed as shell syntax; all Agent effects continue through
  the existing Namespace, Connection, Tool governance, Host Mounts, and
  sandbox owners.
- Keep the existing StdioDriver when either stdin or stdout is not a terminal.
  Its explicit `ls`, `cat`, `tail`, `write`, `echo`, and `spawn` builtins remain
  the generic file-native Shell surface.
- Use AgentFS Process IO for input and incremental output. Ctrl-C interrupts the
  current Agent turn through `/agent/root/machine/ctl`, not Kernel Process
  control, and a subsequent task can use the still-running Root Agent.
- Closing the renderer closes its own file streams only; it does not stop the
  shared Host or Root Agent.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `alan-shell`: distinguish terminal Agent task input from the generic
  StdioDriver command grammar and define the TTY/non-TTY entry behavior.
- `alan-renderer-host-contract`: define the minimal Root Agent attachment,
  incremental IO, turn-scoped interrupt, and detach-without-shutdown behavior.

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
  conformance. Those remain later roadmap work if the tracer bullet shows a
  real gap.

## Impact

The composition change belongs in `crates/alan/src/main.rs`: add the existing
`alan-terminal-ui` workspace crate as a direct dependency, choose its
file-backed renderer only for a TTY, and preserve the StdioDriver fallback.
The Rust architecture ratchet will record this single root-composition edge;
it does not add an external package. A focused renderer test will lock down
Ctrl-C mapping, and ordinary-terminal/Herdr acceptance will validate real
read-only Agent tasks, incremental output, cancellation, and continued use.
