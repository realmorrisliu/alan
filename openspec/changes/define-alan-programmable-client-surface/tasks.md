## 1. Shared Alan Shell Command Layer

- [ ] 1.1 Extract the current private `LineCommand` parser from `StdioDriver`
  into a reusable headless Alan Shell command module without adding grammar,
  heuristic path/Tool inference, or agent-specific commands.
- [ ] 1.2 Introduce a reusable command executor and output-sink contract for
  finite and live command output; keep the `alan-shell` crate dependent only on
  `alan-ap`.
- [ ] 1.3 Rewire `StdioDriver` to the shared parser/executor and preserve its
  current `ls`, `cat`, `tail`, `write`, `echo`, `spawn`, concurrent-input, error,
  and tail-cleanup behavior with focused tests.
- [ ] 1.4 Add namespace-derived discovery helpers/tests for Paths, file kinds,
  access visibility, `/bin`, Tool Manifests, `/man`, and `/lib/skill`, with no
  generic UI schema or renderer-private authority.

## 2. Streaming Process Output Foundation

- [ ] 2.1 Evolve the user-space `ProcessRunner` bridge so a running Process can
  append output incrementally to the existing `/proc/<pid>/io/output` Stream
  before returning its terminal exit outcome, without adding a Kernel concept.
- [ ] 2.2 Preserve compatibility for existing agent/tool runners while migrating
  them to the streaming output sink, and keep executable resolution constrained
  to the Process invocation Namespace.
- [ ] 2.3 Add ProcFS tests proving pre-exit output reads, ordered stream offsets,
  final exit state, runner failure, interrupt/cancel, and output-stream closure.
- [ ] 2.4 Re-run dependency-boundary tests proving `alan-kernel` still depends
  only on `alan-ap` and the streaming runner bridge does not leak shell, editfs,
  Tool, or renderer semantics into Kernel.

## 3. Editfs Validation And Materialization

- [ ] 3.1 Remove `ExecutionPolicy::{AcceptAll,DenyAll}` and refactor `ctl exec`
  into atomic validation of evaluator Process Path, body revision, address
  revision, and selected range; editfs must never spawn or execute the command,
  and the legacy path-less `exec rev:... addr:... <start>..<end>` syntax is
  rejected rather than validated.
- [ ] 3.2 Define and implement Process-linked editfs event records for execution
  start and result materialization while keeping Process status, output,
  cancellation, and exit truth exclusively under `/proc`.
- [ ] 3.3 Add the complete-document `materialize` control write: expected
  revision and append position plus bounded UTF-8 result bytes and the
  evaluator's `/proc/<pid>` path, committing atomically on clunk as an ordinary
  body edit plus a Process-linked materialization event, failing stale commits
  with no side effects.
- [ ] 3.4 Reject post-hoc Process/range association records that do not carry
  their bytes in a `materialize` commit, and add concurrency tests proving a
  materialization event can never name another writer's bytes.
- [ ] 3.5 Replace the existing editfs policy tests with focused tests for current
  and stale evaluator snapshots, legacy path-less exec rejection, partial ctl
  writes, concurrent selection/body changes, safe result append, materialization
  records, and blocking event reads.

## 4. Native run Tool And Package Projection

- [ ] 4.1 Implement the native Rust `run` Tool as an ordinary executable Process
  that receives a buffer Path or bounded descriptors plus the expected body and
  address snapshot, reads the captured command, and validates through editfs
  before dispatch.
- [ ] 4.2 Dispatch the shared Alan Shell command executor inside `run` under the
  inherited Process Namespace and credentials; ensure missing executables,
  mounts, descriptors, or policy approval cannot be supplied by editfs.
- [ ] 4.3 Stream every command result to `/proc/<pid>/io/output`; for finite
  bounded UTF-8 output, implement safe end-append materialization with bounded
  conflict retries and distinct command-vs-materialization status diagnostics.
- [ ] 4.4 Keep `tail` running with its source Stream Descriptor, publish live
  bytes only through Process output, close descriptors on exit/cancel, and prove
  explicit finite capture by running `cat` on retained Process output.
- [ ] 4.5 Add `/bin/run`, `/lib/exec/run/manifest`, and `/man/1/run` through a
  named namespace-bootstrap compatibility projection; document its deletion
  gate when `alan-binfs`/normal package mounts land and do not expose `run` only
  through the Agent Execution Engine-private Tool registry.
- [ ] 4.6 Add Tool/process tests for manifest/manual discovery, successful and
  stale selection, unknown command, finite output, non-UTF-8/oversized output,
  spawn denial, side-effect failure, materialization conflict, live tail,
  cancellation, and Process exit state.
- [ ] 4.7 Add governance-parity tests: a Tool whose direct spawn requires policy
  escalation or approval raises the identical escalation, approval, and audit
  records when spawned by the evaluator through `run`; `run` itself carries no
  pre-approved authority and the audit names the inner Tool identity.

## 5. Headless Programmable Client Harness

- [ ] 5.1 Assemble a headless Namespace with `/proc`, `/srv/edit`, the single
  `/mnt/edit` buffer, `/bin/run`, package metadata, and representative readable,
  writable, and Stream files without adding a buffer manager or new root.
- [ ] 5.2 Prove the complete loop: discover namespace files, edit/select a Shell
  command, spawn `run`, observe `/proc/<pid>`, validate through editfs, stream
  output, materialize a finite result, and inspect Process-linked buffer events.
- [ ] 5.3 Run the same harness operations through human-shaped and agent-shaped
  aP clients and prove neither path needs renderer-specific domain code,
  daemon/session transport, or editfs-owned authority.
- [ ] 5.4 Add concurrency and recovery coverage for stale selections, concurrent
  edits, safe materialization retry, live output offsets, renderer/client
  detach, reconnect from offsets, cancel, and explicit capture.

## 6. Documentation And Boundary Review

- [ ] 6.1 Update Alan Shell/editfs architecture documentation and public Rust API
  docs to use Programmable Client Surface, Alan Shell Evaluator Process, and
  `run` consistently with `CONTEXT.md` and AGENTS.md component names.
- [ ] 6.2 Document that Rust to WASM Component is the explicit promotion target
  for mature Tool/File-Server behavior while WASM hosting, WIT packages, build,
  signing, installation, and projection remain a separate future change.
- [ ] 6.3 Audit the diff for forbidden ClientSurface objects/ids/managers,
  execution registries, editfs command authority, generic UI schemas,
  surface-only parsers, new top-level roots, and hidden renderer actions.

## 7. Verification, Review, And Archive Readiness

- [ ] 7.1 Run `cargo fmt --all` and focused tests for `alan-shell`, `alan-editfs`,
  `alan-kernel`, the native runtime bootstrap/runner owner, and the new headless
  harness.
- [ ] 7.2 Run focused Clippy with warnings denied for every materially changed
  Rust crate, then run the proportional workspace test/check gate required by
  the final implementation diff.
- [ ] 7.3 Run `openspec validate define-alan-programmable-client-surface
  --strict`, `openspec validate --all --strict`, and `git diff --check`.
- [ ] 7.4 Review the final diff against ADR-0024 through ADR-0027, the Rust test
  placement contract, namespace amplification caveats, Process/file ownership,
  and the named binfs compatibility-projection deletion gate.
- [ ] 7.5 After implementation review and merge, sync the four delta specs into
  `openspec/specs/`, verify the merged behavior, and archive the change.
