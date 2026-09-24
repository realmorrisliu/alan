# Unify command and Agent interaction

Status: accepted direction, 2026-09-24. The user confirmed the consolidated
design after all interview rounds. Runtime implementation remains pending. The target revises ADR-0056 input semantics
while preserving its non-owning attachment and Process lifecycle boundaries.

Alan offers one interaction and execution model: its Agent Machine can perform
an exact deterministic command or advance through model reasoning. The TUI
presents that interaction. Reusable aP Shell facilities can remain separate code;
users do not have to choose between disconnected command and Agent contexts.

The later user-approved KISS revision on the same date supersedes the initial
bounded namespace-command grammar: use a mature Host shell with internal aP access. The subsequent accepted
refinement hides protocol operations behind task-oriented alan9 commands. Linux/FUSE exploration is deferred. See the
[decision report](../../openspec/changes/unify-agent-command-input/decision-report.md).

## Decision

- `!command` selects one exact Host shell script; `:content` selects Agent reasoning.
  Unprefixed input eventually uses typed intent classification. Terminal and
  redirected input share the rules; all redirected stdin is one submission.
- The ordinary prompt is `alan: `; a leading `!` changes it to `alan! ` for that
  submission. Empty-command Backspace returns to Agent input, and each accepted
  submission resets the next entry to `alan: `. Prefix intent remains canonical
  runtime input, not renderer-owned execution. Future automatic routing requires
  a qualified visible route contract, never silent execution under an Agent prompt.
- Classification selects a path without rewriting commands or granting authority.
  Ambiguity asks for clarification; evaluator failure uses bounded governed Agent
  fallback. Missing generation or a required unavailable response channel fails
  explicitly. Execution failure never silently repairs or redispatches work.
- Commands execute through the existing governed native Tool boundary using a
  mature Host shell. The body is passed unchanged; shell syntax, pipelines,
  redirection and expansion belong to that shell. No general shell interpreter
  or command-string path rewriting is implemented in Alan.
- aP remains the service/resource protocol. `/mnt` controls resource reachability,
  not prompt inclusion or a universal native filesystem view. Host Mount grants
  feed both HostFS and native sandbox authority. Virtual mounts grant no native
  paths. Task-oriented alan9 commands encapsulate their protocol operations;
  ordinary users and Agent task instructions need not manipulate aP paths,
  descriptors or commit documents.
- alan9 control commands are ordinary governed executables primarily for Agents,
  also callable explicitly by advanced users. They use existing aP service owners
  and caller-scoped authority, not a privileged manager API or a parallel state store.
- Project read/edit/search tools and native commands share public Host paths and
  the same granted backing files. Structured tool adapters resolve these paths
  internally; no shadow project copy or shell-string translation is introduced.
  Successful committed edits are visible to the next native read or `git diff`;
  external edits likewise appear to subsequent Agent reads. Staged edits must be
  identified as pending rather than presented as saved project changes.
- Native commands use Host paths and a Host-adapter-resolved cwd. The adapter may
  expose authorized execution paths to the calling Agent and its evidence; this
  narrowly revises the old blanket Host-path secrecy rule without making path
  text a capability or moving grant ownership out of Host Mount Service.
- Prefix parsing remains nonrecursive; slash controls remain explicit. `!` never
  grants additional authority. User and Agent commands share the same execution
  boundary; OS sandboxing and existing degradation rules remain applicable.

- The Agent Process owns shared cwd and Machine-ordered ordinary submissions.
  A standalone explicit user `cd` updates cwd; script-local `cd` and per-action
  Agent cwd do not. Cwd retains an authorized Host Mount reference and relative
  location; its execution path is resolved by the Host adapter. Responses and
  controls retain priority. Interrupt cancels current work and pauses the queue;
  explicit continuation/discard controls pending work. Detach leaves work running.
- Commands and results share existing Action and durable execution evidence.
  Subsequent model input receives bounded output and references, with truncation
  and retention loss explicit. Command completion does not generate commentary.
- Restart never implies automatic replay. Recoverable pending work is paused,
  unknown effects reconciled and cwd restored only from reliable, still-authorized
  state. Missing cwd requires an explicit choice before dependent work.
- Explicit `!`/`:` delivery comes first, retaining the Agent baseline for unprefixed
  input. Automatic routing follows typed capability delivery, shadow qualification
  and explicit activation. Jev remains a candidate adapter, not a first-slice dependency.

## Trade-off and ownership

One interaction preserves context and permits fast exact commands, but requires
explicit ambiguity, ordering and recovery rules. Keeping two product modes would
leave the original split; model-generated rewriting would weaken exact-command
semantics. Unification preserves namespace rights, applicable governance, sandbox,
Process identity and existing evidence owners. It adds no global router or second
Kernel Process type and does not require a universal model review of human commands.

Normative deltas, design and acceptance tasks live in
[unify-agent-command-input](../../openspec/changes/unify-agent-command-input/proposal.md).
[The interview record](../../openspec/changes/unify-agent-command-input/design-interview.md)
retains the confirmed choices. Generic typed evaluation remains with
[add-cognitive-model-routing](../../openspec/changes/add-cognitive-model-routing/).
Current code forwards `!` text into Agent input; the old bash specification is
not evidence of a delivered deterministic command path.
