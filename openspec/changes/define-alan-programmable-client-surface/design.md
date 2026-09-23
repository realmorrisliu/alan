## Context and traced path

The current and target paths are:

```text
bare `alan`
  ├─ stdin + stdout are TTY
  │    → LocalAttachment to the dedicated Alan OS Host
  │    → existing `/agent/root`
  │    → `alan-terminal-ui` file-backed renderer
  │    → AgentFS input/output, current generation Connection and governed Tools
  └─ otherwise
       → LocalAttachment to the dedicated Alan OS Host
       → `alan-shell::StdioDriver` generic aP builtins
```

Code confirms the Host already owns this lifecycle. `crates/service-manager`
starts and supervises the Root Agent as an ordinary Process and publishes it at
`/agent/root`; `crates/os-host/src/local.rs` attaches clients to the mounted
namespace and checks that root before returning. The Root Agent template uses
the existing Connection and Tool Process runner. `crates/tui/src/file_backed.rs`
already submits framed input, tails AgentFS output and state, and sends
turn-scoped interruption through the AgentFS control file. It does not own
Process spawn or Host shutdown.

The actual gap is the bare-CLI branch in `crates/alan/src/main.rs`, which always
constructs `StdioDriver`, plus the missing direct composition edge from `alan`
to the already-installed workspace TUI crate. `alan-shell` itself remains
protocol-only and agent-agnostic.

## Decisions

### 1. Select by the terminal boundary, not by model interpretation

After explicit subcommands are dispatched, the bare CLI checks both stdin and
stdout with `std::io::IsTerminal`. If both are terminals, it passes the
existing attached namespace and `/agent/root` to
`alan_tui::run_file_backed(FileBackedRunConfig::new(...))`. If either is
redirected, it keeps the current StdioDriver path.

The TUI composer always submits a task to AgentFS. It does not heuristically
interpret arbitrary prose or shell-looking text as a command. Existing slash
UI controls remain renderer controls. The StdioDriver's bounded builtins remain
the explicit generic command surface when invoked in non-TTY mode.

### 2. Attach the existing Root Agent; do not create another Process

The Host starts the Root Agent and owns its Connection, namespace, credentials,
Tools, sandbox, and lifecycle. The CLI must not call `/proc/clone`, choose an
Agent Definition, create a child Agent, or introduce per-pane Agent state. A
terminal process attaches to the existing `/agent/root` surface only.

Submitted input, output, status, and current Agent UI state remain under
AgentFS. The renderer owns only presentation and its own aP fids. No shell
Process, evaluator, alternate output record, or generic action router is added.

### 3. Keep cancellation turn-scoped and exit non-owning

The existing renderer maps Ctrl-C and Escape to `FileBackedAction::Interrupt`
and writes `interrupt` to `/agent/root/machine/ctl`. This is an Agent Runtime
turn interruption; writing `/proc/<pid>/ctl` would terminate the shared Agent
Process and is explicitly wrong. The same Root Agent remains usable for the
next task. Quitting the renderer stops its own file-tail tasks and restores the
terminal; it does not request Host or Agent shutdown.

### 4. Preserve explicit access grants

The renderer receives only the Host attachment namespace. It does not expose a
raw host path or treat terminal cwd, repository location, or Herdr pane identity
as a grant. A project-inspection acceptance fixture must enter the namespace
through an explicit read-only Host Mount. Tools continue to enforce their
existing descriptors, Tool policy, credentials, and sandbox projection.

### 5. Permit one justified root composition dependency

`alan-terminal-ui` is already a workspace crate and depends only on Alan
protocol/Shell boundaries. The `alan` binary is the composition root, so its
single direct edge to `alan-terminal-ui` is intentional. Record that one edge
in `scripts/rust-dependency-baseline.txt` and exempt only
`alan -> alan-terminal-ui` from the no-expansion ratchet; all other new Alan
crate edges remain rejected. Wire the existing inline-TUI contract check into
the repository quality gate so this route cannot silently regress.

## Validation

- Test the Ctrl-C key mapping and existing file-backed turn-control behavior;
  confirm the Agent Process remains running after interruption.
- If the mounted Connection metadata reports `unconfigured`, fail the task with
  a clear unavailable-Connection error before applying defaults from the
  unrelated local model catalog.
- Build the CLI and Host binaries from the current source.
- Run bare `alan` in a non-Herdr pseudo-terminal and confirm a TTY opens the
  file-backed Root Agent renderer. Run a piped invocation to confirm the
  StdioDriver fallback remains available.
- In both an ordinary terminal and a Herdr sibling pane, use an explicitly
  read-only fixture with known content and an unmounted boundary. Observe
  incremental output, cancel a deliberately lengthy task, and complete another
  task on the same Agent without resubmission.
- Run focused Rust tests, the inline-TUI contract, repository quality, strict
  OpenSpec validation, and current-head CI/review.

## Risks and limits

- A configured generation Connection is needed for successful live tasks. An
  unconfigured Connection must fail clearly without confusing model defaults
  for the selected provider's capabilities.
- This slice does not create durable renderer offsets or claim complete
  detach/reattach recovery. It must never auto-resubmit the last task.
- A real model may complete a test prompt quickly; use a bounded, visibly
  streaming response for cancellation and stop it before allowing another Tool
  dispatch.
