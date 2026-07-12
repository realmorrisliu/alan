## MODIFIED Requirements

### Requirement: Workspace manifest is the restore authority
The macOS shell SHALL use the current versioned content-container workspace
manifest as the sole authoritative source for restoring Spaces, Tabs, pin
snapshots, Tab lifecycle metadata, and the last selected Space/Tab across app
restarts. It SHALL NOT restore from a persistent shell-state snapshot or migrate
an earlier manifest shape.

#### Scenario: Manifest is present
- **WHEN** Alan for macOS starts and a valid current workspace manifest exists
  for `window_main`
- **THEN** Alan loads Spaces, Tabs, pin snapshots, lifecycle metadata, and the
  last selected Space/Tab from that manifest
- **AND** Alan materializes the current shell state from the manifest rather
  than another persisted snapshot

#### Scenario: Manifest is missing
- **WHEN** Alan for macOS starts and no workspace manifest exists for
  `window_main`
- **THEN** Alan creates a default current manifest with one default Space and
  one default unpinned terminal Tab
- **AND** Alan uses that manifest as the restore authority for the launched
  shell state

#### Scenario: Unsupported manifest schema exists
- **WHEN** the manifest path contains a terminal-only, `quick_terminal`, or
  otherwise unsupported schema
- **THEN** Alan preserves it as corrupt or unsupported evidence and creates a
  default current manifest
- **AND** Alan does not invoke a legacy decoder, upgrade, or fallback

#### Scenario: Obsolete shell-state file exists
- **WHEN** an Application Support `shell-state-*.json` file remains on disk
- **THEN** Alan does not discover or read it during startup
- **AND** the file cannot become workspace restore authority

### Requirement: Shell state remains a runtime snapshot
The macOS shell SHALL keep `ShellStateSnapshot` as an in-memory
UI/control-plane/runtime projection while using the workspace manifest as the
durable restore authority. It MAY publish current state to the temporary
control-plane `state.json` mirror for current IPC clients, but SHALL NOT persist
an Application Support `shell-state-*.json` snapshot.

#### Scenario: Runtime metadata changes
- **WHEN** terminal title, cwd, renderer state, attention, or Alan binding
  metadata changes
- **THEN** Alan updates the in-memory shell state and current control-plane
  publication
- **AND** Alan writes only restorable workspace intent and lifecycle metadata
  back to the manifest

#### Scenario: App restarts
- **WHEN** Alan for macOS restarts after publishing transient control-plane state
  in the previous process
- **THEN** Alan restores Spaces and Tabs from the workspace manifest
- **AND** terminal runtimes and live projections are newly created rather than
  restored from a prior process snapshot

#### Scenario: Persistent shell-state path is inspected
- **WHEN** repository validation inspects current Apple source and tests
- **THEN** no `ShellStatePersistenceStore`, `restorePrevious`, or Application
  Support `shell-state-*.json` writer or reader remains
- **AND** the transient control-plane `state.json` path remains separately
  identified as live IPC state

### Requirement: Workspace manifest stores content-container restore snapshots
The macOS shell workspace manifest SHALL remain the workspace restore authority
and SHALL store current restorable PaneSlot and ContentInstance snapshots. The
loader SHALL accept only the current content-container schema and SHALL NOT
upgrade terminal-only manifests.

#### Scenario: Current content-container manifest is loaded
- **WHEN** Alan reads a workspace manifest with the current schema and content
  contract version
- **THEN** it materializes Space, Tab, PaneSlot, ContentInstance, selection,
  pin, lifecycle, and restore data directly from that manifest
- **AND** no terminal-only conversion path runs

#### Scenario: Pinned mixed tab snapshot is saved
- **WHEN** the user pins or updates a pinned split Tab containing terminal,
  markdown, or settings content
- **THEN** the workspace manifest saves the split tree, PaneSlot restore
  identity, ContentInstance kind, and each content's restorable payload
- **AND** a terminal payload saves cwd, launch target, and user-visible title
- **AND** a terminal payload MAY save a bounded transcript snapshot as
  session-continuity metadata when one is available
- **AND** markdown/settings payloads save the corresponding file reference or
  settings surface identity
- **AND** the manifest does not save a terminal process, PTY, renderer object,
  Ghostty surface object, unbounded scrollback, or delivery queue

#### Scenario: Unpinned mixed tab live snapshot is saved
- **WHEN** an unpinned Tab contains terminal, markdown, or settings content and
  remains inside its TTL
- **THEN** the manifest's live snapshot saves content-aware restore state
- **AND** terminal content MAY include a bounded transcript snapshot with
  visible history, viewport, cwd, title, dimensions, process summary, and
  capture metadata
- **AND** lifecycle pruning continues to use
  `max(lastActivatedAt, lastActivityAt)`, pin state, and active-task metadata
- **AND** non-terminal content is not treated as a terminal active task

#### Scenario: ShellStateSnapshot stays a runtime projection
- **WHEN** Alan materializes current shell state and publishes it to UI or IPC
- **THEN** `ShellStateSnapshot` remains an in-memory/runtime projection
- **AND** durable restart state remains only in the workspace manifest

### Requirement: macOS delegates portable manifest semantics to shell core
Alan for macOS SHALL delegate current portable workspace manifest validation,
materialization, and pruning semantics to the Rust shell core after the manifest
module has Rust contract tests and adapter tests. The live path SHALL NOT expose
a legacy manifest-upgrade operation.

The macOS platform layer SHALL continue to own Application Support path
selection, file reads and writes, atomic persistence, corrupt-file evidence, and
diagnostics presentation.

#### Scenario: macOS loads a workspace manifest
- **WHEN** Alan for macOS reads a current workspace manifest from disk after
  shell-core manifest integration
- **THEN** macOS passes the current manifest bytes and platform context to shell
  core for validation, materialization, and pruning
- **AND** macOS remains responsible for preserving corrupt evidence and choosing
  the persistence path

#### Scenario: macOS loads an unsupported manifest
- **WHEN** Alan for macOS reads bytes that do not match the current schema and
  content contract version
- **THEN** shell core rejects the manifest without upgrading it
- **AND** macOS preserves evidence and requests a current default manifest

#### Scenario: Manifest output is persisted
- **WHEN** shell core returns an updated current manifest or manifest sync hint
- **THEN** the macOS platform layer writes the result through its persistence
  store
- **AND** shell core does not directly access the user's file system

### Requirement: Workspace manifest algorithms are shell-core authoritative at runtime
The macOS shell SHALL use Rust shell core for current workspace manifest
defaulting, validation, lifecycle pruning, and materialization into current
shell state. Swift SHALL own manifest file IO, corrupt-file preservation, and
platform diagnostics, but SHALL NOT retain either a parallel current algorithm
or a legacy migration implementation.

#### Scenario: Missing manifest creates default through core
- **WHEN** Alan for macOS starts and no workspace manifest exists
- **THEN** Swift asks shell core to create the default workspace manifest
- **AND** Swift writes that manifest to the macOS manifest path
- **AND** Swift does not call a separate `ShellContentWorkspaceManifest`
  defaulting algorithm as a fallback

#### Scenario: Valid manifest materializes through core
- **WHEN** Alan for macOS loads a valid current workspace manifest
- **THEN** Swift asks shell core to materialize the current shell state
- **AND** the launched shell state is derived from the shell-core result
- **AND** Swift does not materialize an alternate state through a platform
  `ShellWorkspaceMaterializer`

#### Scenario: Startup pruning runs through core
- **WHEN** Alan for macOS prunes expired unpinned Tabs during startup
- **THEN** Swift asks shell core to apply the pruning semantics
- **AND** Swift persists the returned manifest when it differs from the loaded
  manifest
- **AND** Swift does not apply a separate pruning algorithm after a core failure

#### Scenario: Corrupt manifest recovery preserves evidence
- **WHEN** Alan for macOS detects an unreadable or unsupported manifest file
- **THEN** Swift quarantines the corrupt file and records recovery diagnostics
- **AND** Swift asks shell core for the replacement default manifest
- **AND** Swift does not restore from or inspect shell-state snapshots as a
  domain fallback

#### Scenario: Core manifest authority fails
- **WHEN** shell core cannot create, validate, prune, or materialize a workspace
  manifest
- **THEN** Alan for macOS reports an explicit shell-core manifest failure
- **AND** it does not silently launch from a Swift-computed workspace state for
  the same manifest

### Requirement: Shell persistence does not block the main thread
The macOS shell SHALL NOT perform a synchronous main-thread disk write on the
terminal metadata or runtime callback path. Every current file persisted on
that path—the workspace manifest, temporary control-plane `state.json` mirror,
and control-plane change-event log—SHALL have its encode and write deferred to a
debounced flush and/or run off the main thread. No Application Support
shell-state snapshot SHALL be written.

#### Scenario: High-output terminal does not stall the UI
- **WHEN** one or more terminals produce sustained high-frequency output
- **THEN** Alan does not perform a synchronous main-thread disk write of the
  workspace manifest, control-plane state file, or event log on the terminal
  metadata or runtime callback path

#### Scenario: Encode and write run off the main thread
- **WHEN** Alan persists the workspace manifest or temporary control-plane state
- **THEN** JSON encoding and atomic file writing run on a background executor
  rather than blocking the main actor

#### Scenario: Control-plane in-memory publication stays prompt
- **WHEN** shell state changes on the terminal callback path
- **THEN** Alan publishes the in-memory control-plane state promptly without
  waiting on a disk write

### Requirement: Workspace persistence cadence is separated by durability class
The macOS shell SHALL persist workspace state on cadences matched to each class
of state rather than rewriting every file on every runtime event:

- Structural state (Spaces, Tabs, order, pin state, pin snapshots, selected
  Space/Tab) SHALL be persisted when its mutation is accepted.
- Restorable terminal transcript snapshots in the manifest and transient
  control-plane state driven by terminal callbacks SHALL be persisted on a
  bounded debounced cadence and force-flushed on app background/resign-active
  and quit.
- A change to transient runtime state such as a Tab's active-task state SHALL
  NOT by itself trigger a synchronous disk write.
- No separate durable shell-state snapshot SHALL participate in any cadence.

#### Scenario: Structural mutation persists promptly
- **WHEN** the user creates, closes, reorders, pins, unpins, or moves a Tab or
  Space
- **THEN** Alan persists the structural change for that mutation

#### Scenario: Active-task change is not a write trigger
- **WHEN** a Tab's terminal-aware active-task state changes
- **THEN** Alan does not write the workspace manifest solely because of that
  change

#### Scenario: Transcript is flushed on background and quit
- **WHEN** Alan for macOS resigns active, is backgrounded, or is asked to quit
- **THEN** Alan force-flushes pending transcript snapshots and the transient
  control-plane mirror before completing the transition

## REMOVED Requirements

### Requirement: Legacy Quick Terminal Restore Data Is Discarded
**Reason**: The hard cut removes the legacy field and decoder rather than
keeping load-tolerant discard behavior in the current manifest contract.

**Migration**: Run the bounded pre-cut inventory/cleanup if the old local file
must be removed; current Alan accepts only the current content-container
manifest and does not read `quick_terminal` data.

### Requirement: Manifest compatibility is preserved during migration
**Reason**: Shell-core extraction is complete enough to make the current schema
the sole contract; terminal-only and `quick_terminal` compatibility is no
longer supported.

**Migration**: No runtime migration is provided. Preserve unsupported bytes as
corrupt evidence if encountered, then create a current default manifest.
