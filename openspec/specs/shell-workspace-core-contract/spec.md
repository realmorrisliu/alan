# shell-workspace-core-contract Specification

## Purpose
Defines the platform-neutral shell workspace core: model and reducer semantics,
versioned manifests, actions, control commands, Terminal Profiles, constrained
bindings, and the adapter boundary for UI and OS effects.
## Requirements
### Requirement: Workspace model is platform neutral
The shell core SHALL define platform-neutral workspace model types for Spaces,
Tabs, PaneSlots, ContentInstances, split trees, attention, lifecycle state,
content kinds, terminal metadata, restore identity, and stable IDs.

#### Scenario: macOS and GTK represent the same workspace
- **WHEN** macOS and a future GTK client load the same portable workspace model
- **THEN** Space, Tab, PaneSlot, ContentInstance, split tree, focus, selection,
  attention, and lifecycle fields have the same domain meaning on both clients
- **AND** platform-specific window, view, renderer, process, and widget handles
  are not part of the shell core model

### Requirement: Reducers are pure state transitions
The shell core reducer SHALL apply workspace operations as pure state
transitions that return a next state, domain events, runtime intents, manifest
sync hints, and stable result or error data.

#### Scenario: Reducer accepts a valid split operation
- **WHEN** a valid PaneSlot split operation is applied to a workspace state
- **THEN** the reducer returns the updated workspace state
- **AND** it identifies the created PaneSlot and ContentInstance
- **AND** it returns runtime intents needed by the platform adapter without
  creating terminal processes directly

#### Scenario: Reducer rejects an invalid operation
- **WHEN** a reducer operation references a missing Space, Tab, PaneSlot, split,
  or unsupported content kind
- **THEN** the reducer leaves the workspace state unchanged
- **AND** it returns a stable error code that platform control surfaces can
  report without inferring failure from raw state diffs

### Requirement: Manifest semantics are portable and versioned
The shell core SHALL own portable workspace manifest semantics for the current
schema, including schema validation, default manifest creation, materialization
into workspace state, TTL pruning, and pinned and live restore snapshots. It
SHALL NOT decode or upgrade terminal-only manifests or tolerate a legacy
`quick_terminal` field.

The manifest SHALL store restorable workspace intent and SHALL NOT store
terminal process handles, PTY file descriptors, renderer objects, Ghostty
surface pointers, platform window handles, delivery queues, or unbounded
scrollback.

#### Scenario: Current manifest materializes workspace state
- **WHEN** a valid current workspace manifest is materialized by shell core
- **THEN** the resulting workspace state preserves Space, Tab, PaneSlot,
  ContentInstance, selection, split tree, lifecycle, profile reference, and
  restore snapshot semantics
- **AND** platform adapters create new terminal runtimes from returned runtime
  intents rather than restoring renderer or process objects from the manifest

#### Scenario: Unsupported manifest shape is submitted
- **WHEN** a terminal-only manifest, a manifest containing `quick_terminal`, or
  an unsupported schema version is submitted to shell core
- **THEN** shell core rejects it as unsupported
- **AND** no compatibility decoder, discard pass, or upgrade output is produced

#### Scenario: Manifest pruning runs
- **WHEN** shell core prunes unpinned inactive Tabs outside the configured
  lifecycle TTL
- **THEN** pinned Tabs and Tabs protected by active-task metadata are retained
- **AND** empty Spaces remain durable until explicitly deleted

### Requirement: Action registry is shared by platform surfaces
The shell core SHALL define stable action IDs, target resolution rules,
availability, shortcut metadata, and action-to-effect mapping for reusable shell
actions.

#### Scenario: Different surfaces resolve one action
- **WHEN** a menu item, keyboard shortcut, context menu, or future GTK action
  invokes the same registered shell action
- **THEN** action availability and target resolution come from the shell core
- **AND** the platform surface owns only presentation, input gesture handling,
  and platform-specific menu or shortcut rendering

#### Scenario: Removed quick terminal action is requested
- **WHEN** a platform surface requests a `shell.quick_terminal.*` action ID
- **THEN** the shell core reports it as unsupported
- **AND** the request is not remapped to a replacement app/window command or any
  reusable workspace action

### Requirement: Control command reducer returns authoritative outcomes
The shell core SHALL define reusable shell control command validation, stable
error codes, domain reducer dispatch, and authoritative response projection for
workspace-domain commands.

The shell core SHALL NOT own socket serving, file polling, response deadlines,
or terminal runtime execution.

#### Scenario: Control command mutates workspace state
- **WHEN** a control command such as `space.create`, `tab.open`, `pane.split`,
  `tab.pin`, or `tab.move_to_space` is valid
- **THEN** the shell core returns an authoritative applied response derived from
  the updated state
- **AND** it returns any runtime intents that the platform adapter must execute

#### Scenario: Control command depends on runtime
- **WHEN** a control command requires terminal runtime execution such as sending
  text to terminal content
- **THEN** the shell core validates domain target semantics and returns a
  runtime intent
- **AND** the platform runtime adapter owns the actual terminal delivery result

### Requirement: Terminal Profile domain resolves launch intents
The shell core SHALL own portable Terminal Profile document shape, editor and
validation logic, deterministic resolution order, missing or unavailable profile
states, and construction of terminal launch intents.

The shell core SHALL NOT spawn processes, inspect platform-specific GUI state,
edit sudoers files, invoke AppleScript, or apply privileged account changes.

#### Scenario: Profile resolves to launch intent
- **WHEN** terminal creation references a valid Terminal Profile
- **THEN** the shell core resolves the profile according to the documented order
- **AND** it returns a `TerminalLaunchIntent` that the platform terminal adapter
  can translate into a macOS or Linux terminal startup operation

#### Scenario: Profile is missing
- **WHEN** terminal startup references a missing Terminal Profile id
- **THEN** the shell core preserves the missing reference in the result
- **AND** it returns fallback launch intent information without deleting the
  reference from workspace state or manifest data

### Requirement: Platform adapters own UI and OS effects
Platform adapters SHALL own presentation, windowing, terminal runtime
attachment, PTY/process handles, renderer objects, file-system persistence
locations, IPC transport, clipboard, file picker, diagnostics presentation, and
privileged OS effects.

#### Scenario: Runtime intent is emitted
- **WHEN** the shell core emits an intent to start, close, send input to, or
  capture a snapshot from terminal content
- **THEN** the platform adapter executes the intent using platform terminal
  runtime facilities
- **AND** the shell core receives only portable runtime metadata or command
  outcomes in response

### Requirement: Cross-language bindings use a constrained facade
Cross-language bindings for platform clients SHALL use a coarse-grained,
versioned facade over the Rust shell core before exposing fine-grained typed
model APIs.

The first binding facade MUST NOT expose async callbacks, foreign traits,
platform callbacks, or long-lived Rust workspace objects.

#### Scenario: Swift calls shell core through bindings
- **WHEN** Swift integrates a Rust-backed shell core operation
- **THEN** it calls a versioned facade entrypoint
- **AND** the facade returns a structured success or error envelope
- **AND** ABI or schema version mismatches are reported as explicit errors
  rather than causing silent state corruption

### Requirement: Rust contracts and FFI adapter checks gate Swift replacement
Each Swift shell domain module replacement SHALL be gated by Rust contract tests
and FFI adapter checks that prove Rust shell core behavior and Swift projection.

#### Scenario: Swift reducer branch is replaced
- **WHEN** a Swift workspace reducer branch is removed or replaced by a Rust
  shell core call
- **THEN** equivalent Rust contract cases exist
- **AND** Rust tests pass for those cases
- **AND** Swift adapter tests verify encode, decode, error mapping, and version
  handling for the replacement path

### Requirement: Rust shell core owns platform-neutral workspace domain logic

Alan SHALL provide a platform-neutral Rust shell workspace core that owns reusable Space, Tab, split, focus, lifecycle, and action semantics shared by host clients. The core SHALL depend only on portable domain types and explicit adapter contracts.

#### Scenario: Platform client mutates workspace state

- **WHEN** a platform client requests a reusable workspace mutation
- **THEN** the Rust shell core returns the next state, domain events, and adapter intents
- **AND** platform UI code does not reimplement the mutation semantics

#### Scenario: Shell core is built independently

- **WHEN** the shell core crate is compiled in isolation
- **THEN** it requires no Apple or GTK framework, terminal renderer, socket transport, privileged executor, clipboard, or file picker
- **AND** platform and OS effects remain behind adapters
