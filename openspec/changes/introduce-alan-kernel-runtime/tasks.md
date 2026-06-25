## 0. Alan OS Alignment

- [x] 0.1 Declare this change as an Alan Kernel implementation slice
  under `programmable-environment-product`, not the complete product or middleware
  between the current Agent Execution Engine and Alan Shell.
- [x] 0.2 Record which constitution criteria the first slice proves:
  files, processes, stream files, namespaces, mounts, credentials, descriptors,
  access rights, app/service operation descriptors, service-owned stream files,
  native references, no-side-effect replay, and renderer-host snapshots.
- [x] 0.3 Record deferred constitution criteria: broad first-launch local work
  discovery, complete Alan App workflow, SwiftUI host implementation,
  WASM extension loading, and universal resource addressing.
- [x] 0.4 Keep the Alan Shell compatibility path as the first vertical slice
  while documenting that the current Agent Execution Engine and current
  service/transport implementation are implementation details, not Alan OS or
  Alan Kernel.
- [x] 0.5 Record the durable crate naming direction: `alan-kernel` for the
  substrate, future `alan-agent` for the built-in Alan Agent app module,
  future `alan-shell` for the Alan Shell interaction path,
  `alan-runtime` -> `alan-agent-engine` if renamed, and
  `alan-protocol` -> `alan-agent-protocol` if it remains session-specific.
- [x] 0.6 Record this change as the first Alan OS spine slice in the
  roadmap, with Alan Agent app projection, Alan Shell host integration, Alan for
  macOS host migration, Groove Master, and UPDF gated on usable Kernel contracts.
- [x] 0.7 Align with `define-agent-process-os-model`'s
  `agent-process-os-model` by keeping Agent Process anchors in Kernel and
  deferring AgentFS schemas, tape, request/action files, Tool manifests, Skill
  packages, memory stores, policy evaluation, and Agent Runtime Service
  execution above Kernel.
- [x] 0.8 Reframe Alan Kernel around file-tree UNIX primitives: paths, files,
  processes, stream files, namespaces, mounts, credentials, descriptors, and
  access rights, with capability/object/task/view names treated as V1
  app/service surfaces rather than durable Kernel ontology.
- [x] 0.9 Reframe commands, queries, and subscriptions as typed operation
  surfaces over paths, files, processes, stream files, descriptors, access
  rights, and namespaces, with registries treated as V1 app/service
  discovery/cache surfaces rather than durable Kernel ontology.
- [x] 0.10 Reframe durable Kernel identity around namespace-qualified paths,
  mounts, process-table entries, stream files, credentials, descriptors, access
  rights, and native authority, with opaque ids treated as runtime references
  or V1 compatibility ids.
- [x] 0.11 Reframe streams as named file kinds that can be read, tailed,
  watched, resumed from offsets, and referenced by Agent/App evidence
  interpretations, with subscriptions treated as watch operation surfaces
  rather than a separate event system.
- [x] 0.12 Downgrade the former Activity Ledger / Kernel Journal concept into
  service- and app-owned stream files for activity, audit, recovery, replay,
  and projection rebuilds; Alan Kernel owns no universal semantic journal.
- [x] 0.13 Move Evidence out of Alan Kernel ontology: Kernel owns paths,
  runtime references, stream offsets, process-table entries, service-owned
  stream file offsets, app artifact paths, and native selectors;
  Agent/App layers interpret those pointers as evidence.
- [x] 0.14 Move Artifact out of Alan Kernel ontology: Kernel owns ordinary
  files, stream files, and output pointers linked to producing processes and
  native authority; Agent/App layers interpret those outputs as artifacts.

## 1. Core Crate Skeleton

- [x] 1.1 Add `alan-kernel` to the Cargo workspace with no dependency on `alan-protocol`, Ratatui, Crossterm, AppKit, SwiftUI, or Tokio task handles.
- [x] 1.2 Define typed opaque ids for V1 surfaces such as actors, objects,
  buffers, views, commands, queries, subscriptions, tasks, compatibility
  artifact adapter references, compatibility evidence adapter references, and
  events.
- [x] 1.3 Define descriptor types for V1 surfaces such as actors, objects,
  buffers, views, commands, queries, subscriptions, tasks, compatibility
  artifact adapter surfaces, and compatibility evidence adapter surfaces.
- [x] 1.4 Add native-reference descriptor support so files, Git repositories, agent sessions, terminal handles, and domain-owned resources keep external authority outside Alan Kernel ids.
- [x] 1.5 Add compile-time or focused tests proving `alan-kernel` remains independent from Alan protocol and renderer-host crates.
- [x] 1.6 Ensure the substrate crate/package uses the durable `alan-kernel`
  name before adding new durable app or renderer crates.
- [ ] 1.7 Add first-class file-tree UNIX primitive descriptors for paths,
  mounts, process-table entries, files, stream files, credentials, descriptors,
  access rights, and namespaces.
- [ ] 1.8 Migrate V1 surface descriptors such as objects, buffers, views,
  tasks, commands, queries, and subscriptions so their opaque ids are runtime
  references that resolve to namespace paths, process-table entries, stream
  files, descriptors/access rights, or native authority where durable semantics
  are required.
- [ ] 1.9 Add first-class stream file descriptors with namespace path,
  stream kind, backing process/file/native authority, offset shape,
  read/tail/watch semantics, and durable offset/range reference metadata.
- [ ] 1.10 Move current V1 `EvidenceId` / evidence descriptor compatibility
  surfaces out of durable Alan Kernel API or wrap them as Alan Agent/App adapter
  compatibility until the agent module owns evidence semantics.
- [ ] 1.11 Move current V1 `ArtifactId` / artifact descriptor compatibility
  surfaces out of durable Alan Kernel API or wrap them as Alan Agent/App adapter
  compatibility over produced files until the app module owns artifact
  semantics.

## 2. Operation Surface Discovery

- [x] 2.1 Implement command descriptor and invocation types as executable
  app/service operation surfaces with target, args schema, Credential/actor,
  required Access Rights, optional app capability descriptor, optional agent
  governance metadata, undo or recovery, and invocation-hint metadata.
- [x] 2.2 Implement query descriptor and invocation types as read-only
  inspection surfaces with result references and required Access Rights or app
  capability metadata.
- [x] 2.3 Implement subscription descriptors and update or invalidation messages
  as watch surfaces for file/resource, process, stream file, object, buffer,
  view, task, query, and command-availability dependencies.
- [x] 2.4 Add registry traits or lightweight in-memory registries as V1
  descriptor discovery/cache surfaces for commands, queries, and subscriptions.
- [x] 2.5 Add tests proving mutation routes through command invocation while
  queries and subscriptions remain read-only or observational.
- [ ] 2.6 Ground command, query, and subscription descriptors in namespace,
  path, file, process, stream file, descriptor, access-right, and app capability semantics so
  registries remain
  discovery/cache surfaces rather than source-of-truth ontology.
- [ ] 2.7 Ground subscription descriptors in named stream files or watched
  file/process paths, proving subscriptions are watch operation surfaces rather
  than an independent event system.

## 3. Service Streams, Projection, And Task Runtime State

- [x] 3.1 Define a V1 service/app event envelope with schema version, event id,
  sequence, timestamp, Credential id, causation id, correlation id, and typed event
  kind; `KernelEvent` naming is compatibility-only if present in early code.
- [x] 3.2 Define task events for started, progress, output appended, yielded,
  resumed, side-effect planned, side-effect committed, produced file created,
  Agent/App artifact/evidence adapter attachment, completed, failed, and
  cancelled states.
- [ ] 3.2a Migrate task output chunks and subscription messages toward named
  stream file records with offsets rather than treating them as standalone
  event transport.
- [x] 3.3 Implement in-memory service/app event stream replay with no side
  effects; activity-ledger naming is V1 compatibility-only if present in early
  code.
- [x] 3.4 Implement JSONL service/app event stream append and replay behind the
  same compatibility trait as the V1 shape for event-stream persistence.
- [x] 3.5 Implement an in-memory projection store for current objects, buffers, views, tasks, produced-file indexes, Agent/App artifact/evidence adapter projections, command availability, and dirty-view invalidation.
- [x] 3.6 Add replay tests proving projections rebuild from service/app event
  stream / V1 ledger state without rerunning shell commands, agent turns, file
  writes, network calls, terminal input, or imports.
- [ ] 3.7 Rename or wrap `ActivityLedger` APIs as service/app event stream APIs
  and expose the stream with offset/replay semantics instead of a separate
  Kernel-owned journal abstraction.
- [ ] 3.8 Keep ordinary app, process, file/resource, and host stream files
  separate from the service/app stream files that own their records; Alan Kernel
  must not absorb them into a universal journal.

## 4. Semantic View Snapshots

- [x] 4.1 Define `ViewSnapshot` with view id, buffer id, version, view kind, semantic model, actions, diagnostics, selection, and focus state.
- [x] 4.2 Define strongly typed built-in view models for conversation, task tree, command palette, form, object list, text document read/review, diff, and log stream.
- [x] 4.3 Define schema-versioned dynamic extension view payload support using JSON for unknown or domain-specific views.
- [x] 4.4 Separate semantic view state from host render state in the type model and tests.
- [x] 4.5 Add snapshot tests for conversation, form, task tree, and command palette models as the first implementation surface.

## 5. Alan Agent App Module

- [x] 5.1 Add `alan-agent` to the workspace as the built-in Alan Agent app
  module/projection layer, with dependencies on `alan-kernel` and the agent
  protocol plus compatibility projection surfaces needed for internal
  adaptation.
- [x] 5.2 Map Alan session metadata into an agent session object, conversation buffer, and initial conversation view descriptor.
- [x] 5.3 Register agent commands for submit turn, resume yielded task, interrupt or cancel active work, compact context, and rollback turn history.
- [x] 5.4 Map `alan_protocol::EventEnvelope` turn, text, thinking, tool, plan, warning, error, and yield events into Alan Kernel events and projections.
- [x] 5.5 Map Alan child-run records or lifecycle events into Alan Kernel child task descriptors and task events.
- [x] 5.6 Project Alan confirmation, structured input, and dynamic tool yields into yielded task state and semantic form or approval snapshots.
- [x] 5.7 Add fixture tests using representative Alan event envelopes to verify conversation, form, task tree, artifact, and evidence projections.
- [x] 5.8 Add Agent Process compatibility fixtures proving current Agent
  Execution Engine behavior can project into status, IO, request/action, and
  machine surfaces without moving provider execution, compatibility transport,
  memory stores, or sandbox execution into Alan Kernel.

## 6. Alan Shell Host

- [x] 6.1 Implement Alan Shell renderer/input adaptation over `alan-kernel`
  inside the Alan Shell path, using `crates/tui` during compatibility.
- [x] 6.2 Implement Ratatui renderers for conversation, form, task tree, and command palette semantic snapshots.
- [x] 6.3 Translate Crossterm key, paste, resize, and mouse events into host-local layout changes, semantic input intents, view-local input, or command invocations.
- [x] 6.4 Keep renderer-only state such as line wrapping, terminal cell cache, geometry, and frame timing out of Alan Kernel semantic state.
- [x] 6.5 Add Ratatui snapshot or test-backend coverage for the first built-in semantic view renderers.

## 7. Existing TUI Compatibility Integration

- [x] 7.1 Integrate the Agent Process projection and the Alan Shell path into
  `crates/tui` behind a compatibility-first path while preserving existing
  service/transport wiring.
- [x] 7.2 Preserve session creation or attach, hydration, replay cursor handling, event stream reconnect, submission, resume, interrupt, compact, rollback, and pending-yield behavior.
- [x] 7.3 Run the semantic projection path in parallel with the existing reducer where useful and add parity tests before removing old reducer behavior.
- [x] 7.4 Migrate supported conversation, form, task-tree, and command-palette rendering to semantic snapshots after focused tests pass.
- [x] 7.5 Leave unsupported surfaces on the current TUI path until a semantic model and renderer are implemented.

## 8. Verification And Review

- [x] 8.1 Run focused `cargo test` coverage for `alan-kernel`, `alan-agent`,
  Alan Shell rendering code, and affected `crates/tui` tests.
- [x] 8.2 Run `cargo fmt --all` and the relevant clippy or workspace check target, or document any environment-blocked gate with focused passing tests.
- [x] 8.3 Run `openspec validate introduce-alan-kernel-runtime --strict`.
- [x] 8.4 Run `git diff --check -- openspec/changes/introduce-alan-kernel-runtime crates`.
- [x] 8.5 Perform a PR review pass against the Alan Kernel contracts, app projection
  boundaries, replay no-side-effects rule, and existing TUI compatibility.
- [ ] 8.6 After implementation is merged, sync accepted delta specs into `openspec/specs/` and prepare archive-readiness notes before archiving the change.
