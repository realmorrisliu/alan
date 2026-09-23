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
       → one Agent task from stdin, final answer on stdout, diagnostics on stderr
```

Code confirms the Host already owns this lifecycle. `crates/service-manager`
starts and supervises the Root Agent as an ordinary Process and publishes it at
`/agent/root`; `crates/os-host/src/local.rs` attaches clients to the mounted
namespace and checks that root before returning. The Root Agent template uses
the existing Connection and Tool Process runner. `crates/tui/src/file_backed.rs`
already submits framed input, tails AgentFS output and state, and sends
turn-scoped interruption through the AgentFS control file. It does not own
Process spawn or Host shutdown.

At the start of this change, the bare-CLI branch in
`crates/alan/src/main.rs` always constructed `StdioDriver`, and product Host
boot constructed an empty `ToolRegistry` despite the existing Core Tools. The
implemented composition routes terminal and redirected input to the existing
Agent renderer/one-shot path and registers the Core Tool catalog so `!` reaches
the existing governed `bash` Tool.
`alan-shell` itself remains protocol-only and agent-agnostic.

## Decisions

### 1. Select by the terminal boundary, not by model interpretation

After explicit subcommands are dispatched, the bare CLI checks stdin and
stdout with `std::io::IsTerminal`. If both are terminals, it passes the
existing attached namespace and `/agent/root` to
`alan_tui::run_file_backed(FileBackedRunConfig::new(...))`. If stdin is
redirected, it reads one task, writes the final answer to stdout, keeps
diagnostics on stderr, and reports task failure with a nonzero exit code. If
stdin is a terminal but stdout is redirected, it errors before attaching to the
Host instead of waiting for terminal EOF. The one-shot waiter opens tape and UI
tails against one concrete Root Agent PID, retries the pair if that identity
changes during attach, and uses that same Process path for submission and
observation. It reopens both tails together if the Root Agent PID changes
during a task, recovering the matching submitted turn from the replacement
Process. EOF or an IO error on either tail first triggers a Root Agent PID
check. If the published PID still matches, the waiter allows one 250 ms
PID-poll interval for the Service Manager to publish the exit, then rechecks;
this covers stream closure before PID clearing without retrying a persistently
broken stream. It lets the existing poll cadence bridge a temporarily
unavailable PID during supervised replacement, then reattaches the pair and
recovers from replacement history. If the PID remains unchanged, the original
tail failure remains terminal.

The TUI composer always submits one task to AgentFS. Ordinary prose is not
interpreted as a shell command. A leading `!` is the explicit shell escape:
the Agent is instructed to run the exact remainder using its existing `bash`
Tool, without rewriting or adding commands. Tool policy, approval, sandbox, and
Host Mount boundaries remain authoritative. Product boot registers the
existing Core Tool catalog and implementations; it does not add a new Tool or
register the explorer-only Tools. Existing slash UI controls remain renderer
controls. On host-backed sandbox adapters, filesystem operands and redirection
targets are projected to their authorized Host paths; recognized data positions
such as `echo`/`printf`, Git commit messages, and AWK assignments/programs keep
their original namespace text. AWK `-f` scripts and input-file operands are
projected when the selected backend permits that script form; conservative
backends still reject opaque scripts when they cannot validate protected paths.

The redirected one-shot client waits for task completion or explicit Ctrl-C;
it does not impose a client-only deadline. A `Running` Activity event establishes
that the submitted task has started even when the engine has not yet persisted
its user message to tape. Generation failures are persisted to the UI event
stream before the Agent returns to idle, so the one-shot client can report them
without a tape record. A terminal task error takes precedence over any
intermediate assistant content. Both a TTY renderer and the redirected waiter
keep Ctrl-C pending until the submitted turn is observed as `Running` or
`Paused`, then send the turn interrupt; sending it before acceptance could let
an idle Runtime consume the interrupt before reading the task. They discard the
pending interrupt if the turn settles before becoming active. TTY and redirected
clients take the same channel-scoped, nonblocking task-submission lease and
check the live Root Agent activity before writing. This also prevents a new
client from submitting after a TTY renderer exits and releases its lease while
the shared task keeps running. If the lease is held or activity is Running or
Paused, the new invocation fails clearly and can be retried when idle; durable
request identities are unnecessary while supported clients honor this boundary.
Because tape and UI events have independent tails, observing `Idle` does not
mean the client has consumed every tape record. After `Idle`, one-shot reads the
complete tape from its pinned Process and selects the latest assistant record
for the correlated user task; if no final answer can be correlated, it reports
an unknown outcome instead of returning a streamed preamble.

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
next task. Both TTY and redirected one-shot paths retain Ctrl-C until their
asynchronous Activity watcher observes the submitted turn as `Running` or
`Paused`, then write the same control command. A deferred interrupt is dropped
if the submission settles before becoming active.
Quitting the renderer stops its own file-tail tasks and restores the
terminal; it does not request Host or Agent shutdown.

### 4. Rebind file tails when the Root Agent Process changes

The renderer watches the Service Manager's Root Agent PID independently of
local task state. The PID can update asynchronously after an input write, so an
immediate check alone can miss the replacement. When it changes, the renderer
closes only its old AgentFS tails, hydrates the replacement Process's current
state, and opens new tails without resubmitting input. When this renderer has
no pending task, it preserves its visible transcript and appends replacement
history after the longest matching suffix of retained history. A partially
pruned first cell is matched by its retained rendered-text suffix. Matching
uses the earliest window when identical history repeats; without stable tape
record IDs this favors preserving intervening turns over silently skipping
them, though identical repeated text can remain visually ambiguous. For a
locally submitted task,
current-turn tape is merged after the matching user entry only when a
post-submission `Running` event correlates it to the pending turn. Tape-less
terminal errors are retained only when the replacement's UI history has that
correlated `Running` → `Error` → `Idle` sequence. If the replacement is idle
but the available tape/UI evidence cannot identify the submitted turn, the
renderer reports an unknown outcome instead of matching old prompt text or
stale errors; local turn correlation then ends while PID polling continues.
Watcher sends remain cancellable while the bounded event queue is full, so
stopping old watchers during rebind cannot deadlock the renderer. This is live
Process rebinding, not durable stream-offset recovery or cross-Host restoration.

### 5. Preserve explicit access grants

The renderer receives only the Host attachment namespace. It does not expose a
raw host path or treat terminal cwd, repository location, or Herdr pane identity
as a grant. A project-inspection acceptance fixture must enter the namespace
through an explicit read-only Host Mount. Tools continue to enforce their
existing descriptors, Tool policy, credentials, and sandbox projection.

### 6. Permit two justified root composition dependencies

`alan-terminal-ui` is already a workspace crate and depends only on Alan
protocol/Shell boundaries. The `alan` binary is the composition root, so its
single direct edge to `alan-terminal-ui` is intentional. Record that one edge
in `scripts/rust-dependency-baseline.txt`. `alan-os-host` is the product
composition root for the Root Agent, so its direct edge to `alan-tools` is
intentional to register the existing Core Tool catalog and implementations.
Record and exempt only `alan -> alan-terminal-ui` and
`alan-os-host -> alan-tools` from the no-expansion ratchet; all other new Alan
crate edges remain rejected. Wire the existing inline-TUI contract check into
the repository quality gate so this route cannot silently regress.

## Validation

- Test the Ctrl-C key mapping and existing file-backed turn-control behavior;
  confirm the Agent Process remains running after interruption.
- Verify `!<command>` requests that exact command through the existing `bash`
  Tool, while policy, approval, sandbox, and Host Mount checks still apply;
  preserve namespace paths used as `echo`/`printf` data and Git commit message
  values while mapping file operands and redirection targets to the authorized
  Host Mount;
  verify product boot registers only the existing Core Tool set.
- Verify redirected stdin is one Agent task, stdout contains only its answer,
  stderr carries diagnostics, and failures produce a nonzero exit code.
  Include a Root Agent PID change during the wait and ensure the answer is not
  lost or duplicated; verify a concurrent redirected client cannot claim that
  answer. Verify both TTY and redirected clients refuse submission while the
  Root Agent remains active after its prior TTY renderer exits. On PID change,
  include an older identical prompt in replacement tape
  with no current turn and require an unknown outcome rather than the old
  answer. Wait beyond five minutes without an invented client timeout; report a
  runtime error even when it precedes tape persistence, prefer that error over
  intermediate assistant content, and verify that a successful turn with a
  tool-call preamble still returns its final answer when `Idle` arrives before
  the final tape-tail record. One-shot must reconcile from the pinned tape after
  `Idle`. Verify one-shot Ctrl-C is retained
  until `Running` confirms acceptance before writing the turn interrupt. Verify
  the TTY renderer also defers Ctrl-C/Escape until its submitted turn becomes
  active and drops the deferred interrupt if the turn settles first.
- If the mounted Connection metadata reports `unconfigured`, fail the task with
  a clear unavailable-Connection error before applying defaults from the
  unrelated local model catalog.
- Build the CLI and Host binaries from the current source.
- Run bare `alan` in a non-Herdr pseudo-terminal and confirm a TTY opens the
  file-backed Root Agent renderer. Run a piped invocation to confirm the
  one-task stdio contract.
- In both an ordinary terminal and a Herdr sibling pane, use an explicitly
  read-only fixture with known content and an unmounted boundary. Observe
  incremental output, cancel a deliberately lengthy task, and complete another
  task on the same Agent without resubmission. Verify tail rebinding after a
  Root Agent PID change between submissions.
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
