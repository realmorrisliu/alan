## Context

ADR-0026 adopts the Acme interaction idea: editable text is programmable and
the interaction surface is itself a file server. ADR-0027 places that work in
Ring 4 and states the stronger Alan claim that one namespace is the runtime, UI,
and extension boundary. The repository now has the relevant pieces, but not the
loop between them:

- `alan-shell` is an aP-only namespace client with generic `ls`, `cat`, `tail`,
  `write`, `echo`, and `spawn` builtins. Its command parser and dispatcher are
  private to `StdioDriver`.
- `alan-editfs` exposes one buffer at `/mnt/edit` with
  `body`/`tag`/`addr`/`ctl`/`event`, but its `ExecutionPolicy` only records an
  accept/deny decision and cannot execute under the caller's namespace.
- `/proc` already supplies the correct execution identity, lifecycle, IO, and
  control files. The current `ProcessRunner` returns one terminal
  `ProcessOutcome`, so the user-space runner bridge needs an incremental output
  sink for a live `tail` process.
- renderer hosts are already required to read files and write `ctl`, while the
  current TUI completion model still receives renderer-assembled command,
  Skill, and host-file candidate lists.

The design was sharpened through a domain-modeling workshop. The
resolved terms live in `CONTEXT.md`: Programmable Client Surface belongs to
Alan Shell; an Alan Shell Evaluator Process is an ordinary Process created by
the `run` Tool; a WASM Component is a future portable Tool or File-Server
Service implementation, not a new Alan OS client API.

## Goals / Non-Goals

**Goals:**

- Connect Alan Shell, editfs, `/proc`, and renderer-host contracts into one
  text-first programmable client loop.
- Make every selected-text execution a real Process spawned under the caller's
  bounded Namespace.
- Reuse one explicit Alan Shell grammar for stdio and editable-buffer execution.
- Keep selection validation, Process execution, result materialization, and
  live observation visible as ordinary file operations.
- Prove the loop with a native-Rust, headless end-to-end harness over the
  existing single `/mnt/edit` buffer.
- Preserve Rust to WASM Component as the promotion direction without coupling
  this slice to a WASM host.

**Non-Goals:**

- No new Alan Kernel primitive, ClientSurface object, execution registry,
  opaque execution id, `/client` root, or `/mnt/client` service.
- No complete `rc`-like language, variables, pipelines, conditions, loops,
  functions, or new surface-only grammar.
- No multi-buffer allocator, buffer manager, clone file, or cross-buffer
  navigation.
- No generic widget/form/view schema and no generated dashboard framework.
- No Ratatui or Alan for macOS implementation in the first slice.
- No WASM host, WIT package contract, component build/sign/install pipeline, or
  runtime projection implementation.
- No claim that the in-process v1 namespace is a hardware-enforced security
  boundary; native execution still requires OS sandbox projection.

## Decisions

### 1. Programmable Client Surface is an Alan Shell contract, not a component

The surface is the composition of a caller's mounted Namespace, generic Alan
Shell operations, an editable interaction buffer, and a renderer projection.
No new durable system object represents that composition. Domain truth remains
in its owning service tree, editable interaction state remains in editfs,
execution truth remains in `/proc`, and cursor/viewport/hover state remains in
the renderer.

Alternative considered: add a ClientSurface type, manager, id, and service
tree. Rejected because it would duplicate existing owners and contradict the
namespace-as-runtime/UI thesis.

### 2. One shared explicit grammar drives stdio and selected-text execution

Extract the current `LineCommand` parsing and dispatch from `StdioDriver` into
a reusable headless Alan Shell command layer. The first grammar remains exactly
the bounded set already implemented: `ls`, `cat`, `tail`, `write`, `echo`,
`spawn`, plus empty/exit handling where appropriate. Unknown text fails
explicitly. The surface never guesses that arbitrary prose is a path or Tool.

Alternative considered: design a complete `rc` dialect now. Rejected because
it would lock language semantics before dogfood establishes the needed
composition forms. Alternative considered: create an editfs-specific parser.
Rejected because two grammars would immediately drift.

### 3. `run` is a Tool and every execution is an ordinary Process

Install a first-party Tool at `/bin/run`, with its machine-readable manifest at
`/lib/exec/run/manifest` and manual at `/man/1/run`. A client reads the current
`addr` snapshot and spawns `run`, passing the `/mnt/edit` path or bounded buffer
descriptors plus the expected body/address revisions and range. The resulting
Alan Shell Evaluator Process is visible at `/proc/<pid>`; that path is the sole
execution identity.

The target `alan-binfs` crate does not yet exist. The first slice therefore
uses a named compatibility projection in the namespace-native bootstrap to
bind the executable and its package files. That projection has one deletion
gate: replace it with the normal binfs/package mount when `alan-binfs` lands.
It must not make `run` available only as an Agent Execution Engine-private JSON
Tool, because humans and non-agent clients must be able to spawn the same
command file.

The `run` Tool uses its `ProcessInvocation` pid and inherited Namespace to read
the selected bytes and submit a complete-document `ctl exec` containing the
process path and revision snapshot. Editfs atomically verifies that the
captured bytes still describe the active selection before the Tool dispatches
the shared command executor. A stale or invalid selection makes the Tool exit
without executing the command.

Alternative considered: a renderer-local helper. Rejected because humans,
agents, tests, and other clients would not share one executable. Alternative
considered: an editfs-owned execution registry with correlation ids. Rejected
because `/proc` already owns execution identity, status, output, cancellation,
and exit state.

### 4. Editfs validates interaction state but never supplies execution authority

Remove `ExecutionPolicy::{AcceptAll,DenyAll}` as the execution boundary. A
successful `ctl exec` means only that the body/address snapshot is current and
the caller had write access to `ctl`. Whether `/bin/run` is visible, may be
spawned, may access a mount, or may perform a side effect is decided by the
spawner's Namespace, descriptors, Tool governance, credentials, and sandbox
projection.

The order is deliberately spawn then validate: a denied spawn produces no
Process and is audited by the Tool/governance path; a successful Process
submits its real `/proc/<pid>` path while validating immediately before command
dispatch. Editfs never runs a Tool on behalf of a client and therefore cannot
become an arbitrary-command confused deputy.

Alternative considered: let editfs invoke a callback under service authority.
Rejected because a client could acquire the service's ambient mounts and Tool
rights.

### 5. Process IO is canonical; bounded finite text is also materialized

All command output is appended to `/proc/<pid>/io/output`, including output
produced before Process exit. The user-space ProcessRunner bridge will accept an
incremental output sink backed by the existing ProcFS stream; this changes no
Kernel ontology.

For a finite command whose complete output is bounded UTF-8, `run` submits one
complete-document `materialize` control write carrying its `/proc/<pid>` path,
the expected body revision and append position, and the result bytes; editfs
commits on clunk, atomically appending the bytes as an ordinary body edit and
emitting the Process-linked materialization event in the same commit. A
conflict never overwrites another client's edit: a stale revision fails the
commit without side effects and `run` retries a bounded number of safe
end-appends against the newly read end. Because bytes and attribution commit
together, a Process/result link can only name bytes this evaluator actually
appended — no post-hoc range claim exists, and no aP clunk payload or protocol
change is needed. The editfs event stream records the resulting body range,
body revision, and `/proc/<pid>` path, so the raw body stays plain text while
the execution boundary remains inspectable. If materialization fails,
the command may still succeed; its complete result remains in Process output
and status distinguishes command failure from materialization failure.

Alternative considered: renderer-owned copy-on-exit. Rejected because headless,
TUI, macOS, and agent clients would behave differently. Alternative considered:
store output only in editfs events. Rejected because events are audit metadata,
not an unbounded result store.

### 6. Live `tail` remains descriptor-backed until explicitly captured

For `tail`, the evaluator Process remains running and holds the source Stream
Descriptor. Bytes flow incrementally to its own `io/output` and are rendered as
a transient projection; they are not continually copied into `body`. Interrupt
or cancel uses `/proc/<pid>/ctl`. A later attachment reads the Process output
stream from a saved offset. A client that wants editable text stops or snapshots the live
operation and explicitly runs `cat /proc/<pid>/io/output`; that finite command
uses the normal materialization path.

Alternative considered: append every live byte to `body`. Rejected because it
creates unbounded editable state, revision churn, and a hidden retention policy.

### 7. Discovery is namespace-derived and text-first

Generic discovery walks the mounted Namespace and reads `/bin`, Tool Manifests,
`/man`, `/lib/skill`, file kinds, and access rights. A renderer may project that
information into completion or a selector, but its candidate list is a cache,
not the authority. Capability-specific rich renderers may interpret the same
service tree; services are not required to publish a generic widget/form schema.

Alternative considered: require UI schemas from every service. Rejected because
it recreates the retired universal View/Command/Query framework and makes files
a veneer over an RPC-shaped UI protocol.

### 8. Host actions and namespace actions remain separate

Space/Tab/Pane/window operations remain in `alan-shell-core` and platform
action routing. Reads, writes, tails, `ctl` commands, and executable spawn
remain namespace operations. Renderers may present both planes together but do
not merge them into one universal action registry.

### 9. Rust to WASM is an explicit promotion target, not this slice's runtime

Scratch text does not become an extension by being saved. Reusable behavior is
promoted deliberately and classified as either a Tool or a File-Server Service.
The preferred portable direction is Rust compiled to a WASM Component with a
WIT boundary and explicit descriptors/access rights. WIT is an internal
component-host ABI; the Alan OS surface remains `/bin` or an aP file tree.

The first `run` Tool is native Rust because no WASM Component host exists in the
current tree. A later change owns hosting, WASI policy, WIT packages, build,
signing, installation, and Service Manager/binfs projection.

## Risks / Trade-offs

- [The current ProcessRunner buffers output until exit] → Add an incremental
  user-space output sink backed by the existing `/proc/<pid>/io/output` Stream
  and cover running-process reads before implementing live `tail` in `run`.
- [A process path written in editfs ctl is caller-asserted in the in-process v1]
  → Accept only the first-party `run` protocol in this slice, treat the path as
  correlation rather than authority, and leave authenticated request provenance
  to the broader aP credential/isolation track.
- [A command side effect can succeed before result materialization conflicts]
  → Keep Process status/output canonical, retry only safe end-appends, and report
  materialization failure separately without replaying the command.
- [A single `/mnt/edit` cannot serve a future multi-buffer UI] → Use it only for
  the first headless proof; define allocation after real renderer lifecycle
  requirements exist.
- [The small grammar may feel less programmable than `rc`] → Preserve a clean
  reusable command boundary and add language features only from dogfood evidence.
- [Native subprocesses cannot inspect Alan namespaces] → Continue projecting
  allowed mounts into the OS sandbox and do not claim mount visibility alone is
  security enforcement.

## Migration Plan

1. Land the modified contracts and glossary terms without changing Kernel
   primitives or renderer products.
2. Extract the Alan Shell command parser/executor while keeping stdio behavior
   byte-for-byte compatible.
3. Evolve the ProcessRunner/ProcFS bridge for incremental output and verify
   running Process observation and cancellation.
4. Replace editfs `ExecutionPolicy` with revision validation, Process-linked
   events, and revision-safe result append semantics.
5. Add the native `run` Tool and its manifest/manual projection in the
   namespace-native bootstrap used by the harness; name the temporary package
   projection and its binfs deletion gate.
6. Prove stale-selection rejection, caller authority, finite materialization,
   live tail, explicit capture, concurrency, and headless human/agent symmetry
   end to end.
7. Keep the existing stdio driver and direct `io/` + `ctl` agent paths working;
   rollback removes the `run` binding and restores the prior editfs adapter
   without changing stored domain data.

## Open Questions

None for the first slice. Multi-buffer allocation, a full `rc` language, native
renderers, generic promotion tooling, and the WASM Component host are explicit
follow-up changes rather than unresolved requirements here.
