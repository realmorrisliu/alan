## ADDED Requirements

### Requirement: Programmable Client Surface belongs to Alan Shell
Alan Shell SHALL provide the Programmable Client Surface as its text-first
interaction contract over the caller's mounted Namespace. The surface SHALL
compose Files, Streams, editable buffers, and executable Processes without
creating a separate client service, app runtime, authority store, generic UI
framework, or top-level namespace root.

#### Scenario: Surface ownership is reviewed
- **WHEN** a programmable client interaction is implemented or rendered
- **THEN** its domain truth remains in the owning mounted service tree, its
  editable interaction state remains in editfs, and its execution truth remains
  in `/proc`
- **AND** no ClientSurface object, client manager, opaque surface id, `/client`,
  or `/mnt/client` authority is introduced

### Requirement: Alan Shell reuses one explicit command grammar
Alan Shell SHALL expose one reusable headless parser and executor for its
existing `ls`, `cat`, `tail`, `write`, `echo`, and `spawn` grammar. The stdio
driver and editable-text execution SHALL use that same parser and executor.
Unknown text SHALL fail explicitly and SHALL NOT be inferred as a Path, Tool,
script language expression, or side-effecting action.

#### Scenario: Stdio and editable text run the same command
- **WHEN** the same valid command text is submitted through `StdioDriver` and
  through an editable buffer selection
- **THEN** both paths resolve the same Alan Shell command and aP operations
- **AND** neither path uses a renderer-local or editfs-local parser

#### Scenario: Arbitrary prose is selected
- **WHEN** selected text does not match the bounded Alan Shell grammar
- **THEN** execution fails without reading, writing, tailing, or spawning based
  on a heuristic interpretation of the prose

### Requirement: The run Tool creates Alan Shell Evaluator Processes
Alan Shell SHALL provide a first-party Tool named `run`, bound at `/bin/run`,
with a Tool Manifest under `/lib/exec/run/manifest` and a manual under
`/man/1/run`. Each selected-text execution SHALL spawn one ordinary Alan Shell
Evaluator Process; `/proc/<pid>` SHALL be its execution identity, status,
output, cancellation, and exit surface. Tool governance and policy escalation
SHALL apply to every command the evaluator dispatches — including `spawn` of
another Tool — identically to the same operation invoked directly by the
caller; `run` SHALL NOT bypass, launder, or pre-approve a policy decision that
direct invocation would raise.

#### Scenario: Selected text is executed
- **WHEN** a client invokes `run` with an editable-buffer Path or bounded
  descriptors and an expected body/address revision snapshot
- **THEN** it spawns a Process under the caller's delegated Namespace and the
  Process validates that snapshot before dispatching the shared command
  executor
- **AND** editfs does not create an execution object or run the command under
  service authority

#### Scenario: Selection validation fails
- **WHEN** the body revision, address revision, or selected range no longer
  matches when the evaluator Process validates it
- **THEN** the Process exits failed without executing the selected command
- **AND** its failure is inspectable through `/proc/<pid>`

#### Scenario: Inner spawn keeps governance parity
- **WHEN** the evaluator executes `spawn` for a Tool whose direct invocation
  would raise a policy escalation or approval requirement
- **THEN** the identical governance decision applies to the evaluator-initiated
  spawn, and any escalation surfaces through the normal request path before the
  Tool runs
- **AND** the policy audit records the inner Tool identity, so executing it
  through `run` is indistinguishable from direct spawn to governance

### Requirement: Programmable discovery derives from the Namespace
Alan Shell SHALL derive generic programmable-client discovery from resources
visible in the caller's Namespace, including directory entries, file kinds,
access rights, `/bin`, Tool Manifests, `/man`, and `/lib/skill`. Renderer
completion or selector models SHALL remain projections of those files and SHALL
NOT become private capability registries. Alan Shell SHALL NOT require services
to publish a generic widget, form, card, view, command, or query schema.

#### Scenario: A renderer offers completion
- **WHEN** a renderer presents Paths, Tools, Skills, or commands for completion
- **THEN** the candidates are derived from the mounted Namespace and its package
  metadata
- **AND** withholding a mount or executable removes the corresponding candidate
  without a second renderer-specific deny list

#### Scenario: A service has no rich renderer schema
- **WHEN** a conforming mounted service exposes only its documented file layout
- **THEN** the Programmable Client Surface can still inspect, observe, and invoke
  allowed behavior through text-first file operations
- **AND** the service is not required to describe a generated UI

### Requirement: Mature surface behavior promotes explicitly to classified extensions
Alan Shell SHALL treat scratch editable text as interaction state, not as an
installed extension. Reusable behavior SHALL be promoted explicitly and
classified as either a Tool or a File-Server Service. Rust to WASM Component
with a WIT boundary and explicit descriptors/access rights SHALL remain the
portable extension direction, while `/bin` executables and aP file trees remain
the Alan OS surfaces.

#### Scenario: Scratch text is saved
- **WHEN** a user saves or retains editable buffer text
- **THEN** Alan does not automatically install it as a Tool, Skill, WASM
  Component, or service

#### Scenario: Reusable behavior is promoted later
- **WHEN** a later workflow compiles Rust behavior into a WASM Component
- **THEN** it classifies the component as a Tool or File-Server Service and
  grants only explicit descriptors and access rights
- **AND** WIT does not replace aP as the client-facing Alan OS contract
