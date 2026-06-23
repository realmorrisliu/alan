# macos-shell-workspace-persistence Specification

## Purpose
Defines macOS shell workspace persistence, including workspace manifest restore
authority, corrupt-manifest recovery, durable Spaces, pinned Tab snapshots,
unpinned Tab lifecycle, active-task retention, and shell-state projection.
## Requirements
### Requirement: Workspace manifest is the restore authority
The macOS shell SHALL use a versioned workspace manifest as the authoritative source for restoring Spaces, Tabs, pin snapshots, Tab lifecycle metadata, and the last selected Space/Tab across app restarts.

#### Scenario: Manifest is present
- **WHEN** Alan for macOS starts and a valid workspace manifest exists for `window_main`
- **THEN** alan loads Spaces, Tabs, pin snapshots, lifecycle metadata, and the last selected Space/Tab from that manifest
- **AND** alan materializes the current shell state from the manifest rather than bootstrapping a fresh default state

#### Scenario: Manifest is missing
- **WHEN** Alan for macOS starts and no workspace manifest exists for `window_main`
- **THEN** alan creates a default manifest with one default Space and one default unpinned terminal Tab
- **AND** alan uses that manifest as the restore authority for the launched shell state

#### Scenario: Legacy shell state exists without manifest
- **WHEN** `shell-state-window_main.json` exists but no workspace manifest exists
- **THEN** alan does not migrate that legacy shell state into the workspace manifest
- **AND** alan creates a default manifest instead

### Requirement: Corrupt workspace manifests fail open safely
The macOS shell SHALL preserve evidence of a malformed workspace manifest and start with a default workspace rather than failing to launch or silently overwriting the only copy.

#### Scenario: Manifest cannot be decoded
- **WHEN** Alan for macOS starts and the workspace manifest cannot be decoded
- **THEN** alan preserves the bad manifest as a timestamped corrupt file
- **AND** alan creates a fresh default manifest
- **AND** alan starts with the default workspace

#### Scenario: Fresh manifest is written after corruption
- **WHEN** alan creates a default manifest after detecting corruption
- **THEN** future workspace mutations write to the fresh manifest path
- **AND** the corrupt file remains available for diagnostics

### Requirement: Spaces persist until explicit deletion
The macOS shell SHALL treat Spaces as durable user-created containers that remain visible until the user explicitly deletes the Space.

#### Scenario: Empty Space remains visible
- **WHEN** the last Tab in a Space is closed or retired
- **THEN** alan keeps the Space in the workspace manifest
- **AND** the sidebar continues to show that Space
- **AND** selecting that Space shows an empty workspace state instead of deleting the Space

#### Scenario: Tab retirement does not delete Space
- **WHEN** automatic Tab lifecycle retirement removes every Tab from a Space
- **THEN** alan keeps the Space record and its ordering in the workspace manifest

#### Scenario: Space is explicitly deleted
- **WHEN** the user invokes a delete-space action for a Space
- **THEN** alan removes that Space and its Tabs from the workspace manifest
- **AND** alan chooses a remaining Space or creates a default Space if no Spaces remain

### Requirement: Pinned Tabs restore from explicit snapshots
The macOS shell SHALL persist Pinned Tabs by saving an explicit restore snapshot at pin or update-pin time, and SHALL restore from that snapshot rather than from later transient Tab mutations.

#### Scenario: Single-pane Tab is pinned
- **WHEN** the user pins a Tab that contains one terminal pane
- **THEN** alan saves a pin snapshot with that pane's cwd, launch target, title, and Tab identity
- **AND** future app launches restore that Pinned Tab as a new terminal pane at the pinned cwd

#### Scenario: Split Tab is pinned
- **WHEN** the user pins a Tab that contains a split layout
- **THEN** alan saves the split tree and each leaf pane's cwd and launch target in the pin snapshot
- **AND** future app launches restore the Pinned Tab with that split layout and pane cwd mapping

#### Scenario: Pinned Tab changes after pinning
- **WHEN** a Pinned Tab is split, moved, resized, or cd'd after the pin snapshot was saved
- **THEN** alan does not update the pin snapshot automatically
- **AND** future app launches restore the Tab from the saved pin snapshot

#### Scenario: User updates the pin snapshot
- **WHEN** the user explicitly updates or re-applies pinning for an already Pinned Tab
- **THEN** alan replaces the prior pin snapshot with the Tab's current restorable layout and cwd state

### Requirement: Unpinned Tabs restore until inactive TTL expiry
The macOS shell SHALL retain Unpinned Tabs across app restarts until they are inactive and older than the configured lifecycle TTL.

#### Scenario: Unpinned Tab is inside TTL
- **WHEN** an Unpinned Tab has `max(lastActivatedAt, lastActivityAt)` within 12 hours
- **THEN** alan keeps that Tab in the workspace manifest
- **AND** app restart restores it as a new terminal runtime at its latest restorable cwd or layout

#### Scenario: Unpinned Tab expires while inactive
- **WHEN** an Unpinned Tab is not pinned
- **AND** it has no active task
- **AND** `now - max(lastActivatedAt, lastActivityAt)` is greater than 12 hours
- **THEN** alan retires that Tab from the workspace manifest during lifecycle pruning

#### Scenario: Selected Tab expires
- **WHEN** the selected Tab is retired during startup pruning
- **THEN** alan selects the first remaining Tab in the selected Space
- **AND** if the selected Space has no remaining Tabs, alan keeps the Space selected with no selected Tab

### Requirement: Active tasks prevent unpinned Tab retirement
The macOS shell SHALL protect Unpinned Tabs from lifecycle retirement when terminal-aware metadata indicates that user work is actively running or waiting for input.

#### Scenario: Foreground command is running
- **WHEN** an Unpinned Tab contains a terminal pane with an active foreground command
- **THEN** alan treats that Tab as having an active task
- **AND** lifecycle pruning does not retire it solely because its TTL anchor is older than 12 hours

#### Scenario: alan session is active
- **WHEN** an Unpinned Tab contains an alan session that is running, waiting for input, or pending yield
- **THEN** alan treats that Tab as having an active task
- **AND** lifecycle pruning does not retire it solely because its TTL anchor is older than 12 hours

#### Scenario: Shell is idle
- **WHEN** an Unpinned Tab contains only an idle shell prompt
- **THEN** alan does not treat `processExited == false` by itself as an active task
- **AND** the Tab can be retired after TTL expiry

### Requirement: Shell state remains a runtime snapshot
The macOS shell SHALL keep `ShellStateSnapshot` as the current UI/control-plane/runtime projection while using the workspace manifest as the durable restore authority.

#### Scenario: Runtime metadata changes
- **WHEN** terminal title, cwd, renderer state, attention, or alan binding metadata changes
- **THEN** alan updates current shell state for UI and control-plane publication
- **AND** alan writes only restorable workspace intent and lifecycle metadata back to the manifest

#### Scenario: App restarts
- **WHEN** Alan for macOS restarts after publishing a shell state file in the previous process
- **THEN** alan restores Spaces and Tabs from the workspace manifest
- **AND** terminal runtimes are newly created rather than restored from the old shell state process snapshot

### Requirement: Tab Organization Mutations Persist Immediately
The macOS shell SHALL persist Tab reorder, pin/unpin, and Move to Space
mutations to the workspace manifest immediately after the mutation is accepted.

#### Scenario: Tab is reordered
- **WHEN** the user reorders a Tab inside a Space section
- **THEN** alan writes the new per-Space Tab order to the workspace manifest

#### Scenario: Tab is pinned by drag
- **WHEN** the user drags an Unpinned Tab into the Pinned section
- **THEN** alan writes the pinned state and the current pin snapshot to the
  workspace manifest

#### Scenario: Tab is unpinned by drag
- **WHEN** the user drags a Pinned Tab into the Unpinned section
- **THEN** alan writes the unpinned state and updated section order to the
  workspace manifest

#### Scenario: Tab moves to another Space
- **WHEN** a Tab is moved to a different Space
- **THEN** alan writes the source Space order, target Space order, Tab Space
  ownership, pin state, and selected Space/Tab outcome to the manifest

### Requirement: Organization Preserves Runtime Identity
The macOS shell SHALL preserve Tab, pane, split tree, and terminal runtime
identity across reorder, pin/unpin, and Move to Space mutations.

#### Scenario: Tab is reordered
- **WHEN** a Tab changes order inside its Space
- **THEN** its Tab ID, pane IDs, split tree, terminal runtime handles, scrollback,
  metadata, and queued delivery state remain attached to the same Tab

#### Scenario: Tab changes pin state
- **WHEN** a Tab is pinned or unpinned
- **THEN** alan changes organization metadata without restarting terminal
  runtimes or recreating pane identities

#### Scenario: Tab moves across Spaces
- **WHEN** a Tab moves to another Space
- **THEN** the moved Tab keeps its Tab ID, pane IDs, split tree, terminal
  runtime handles, scrollback, metadata, and queued delivery state

### Requirement: Workspace manifest stores content-container restore snapshots
After `generalize-macos-shell-content-containers`, the macOS shell workspace manifest SHALL
remain the workspace restore authority and SHALL store restorable PaneSlot / ContentInstance
snapshots instead of terminal-only pane snapshots.

#### Scenario: Terminal-only manifest upgrades to content-container shape
- **WHEN** alan 读取 `persist-macos-shell-workspaces` 产生的 terminal-only workspace manifest
- **THEN** alan 将每个 terminal restore leaf 升级为 PaneSlot 加 `terminal` ContentInstance restore payload
- **AND** Space/Tab IDs、ordering、selected Space/Tab、pin 状态、TTL anchor 和 active-task metadata 保持一致
- **AND** 后续 manifest 写入只使用 content-container restore shape

#### Scenario: Pinned mixed tab snapshot is saved
- **WHEN** 用户 pin 或 update-pin 一个包含 terminal、markdown 或 settings content 的 split tab
- **THEN** workspace manifest 保存 split tree、PaneSlot restore identity、ContentInstance kind 和每个 content 的 restorable payload
- **AND** terminal payload 保存 cwd、launch target 和用户可见 title
- **AND** terminal payload MAY save a bounded terminal transcript snapshot as session-continuity metadata when one is available
- **AND** markdown/settings payload 保存对应文件引用或 settings surface identity
- **AND** manifest 不保存 terminal process、PTY、renderer object、Ghostty surface object、unbounded scrollback 或 delivery queue

#### Scenario: Unpinned mixed tab live snapshot is saved
- **WHEN** 未 pin tab 包含 terminal、markdown 或 settings content 且仍在 TTL 内
- **THEN** workspace manifest 的 live snapshot 保存 content-aware restore state
- **AND** terminal content MAY include a bounded transcript snapshot with visible history, viewport, cwd, title, dimensions, process summary, and capture metadata
- **AND** lifecycle pruning 继续使用原有 `max(lastActivatedAt, lastActivityAt)`、pin 状态和 active-task metadata
- **AND** 非 terminal content 不会被误判为 terminal active task

#### Scenario: ShellStateSnapshot stays a runtime projection
- **WHEN** content-container migration 已经完成且 app 重新启动
- **THEN** alan 从 workspace manifest materialize v0.2 shell state
- **AND** `ShellStateSnapshot` 只发布当前 UI、control-plane 和 runtime projection
- **AND** `shell-state-window_main.json` 不重新成为 workspace restore authority

### Requirement: Workspace Manifest Stores Terminal Profile References
The macOS shell workspace manifest SHALL persist Terminal Profile references for
Spaces and terminal content without embedding machine-local Terminal Profile
definitions.

#### Scenario: Space profile reference is saved
- **WHEN** a Space is bound to Terminal Profile `alan`
- **THEN** the workspace manifest stores `terminal_profile_id` `alan` on that
  Space record
- **AND** the manifest does not store the `alan` profile command, Unix user,
  color, icon, or default working directory definition

#### Scenario: Terminal content profile reference is saved
- **WHEN** a terminal content instance is created using Terminal Profile `univer`
- **THEN** the terminal content restore payload stores `terminal_profile_id`
  `univer`
- **AND** restore can explain which Terminal Profile the content was created
  with

#### Scenario: Old manifest decodes without profile fields
- **WHEN** alan reads a workspace manifest created before Terminal Profiles
- **THEN** alan decodes the manifest successfully
- **AND** missing `terminal_profile_id` fields are treated as absent profile
  references

#### Scenario: Missing local profile does not rewrite manifest
- **WHEN** alan restores a manifest that references Terminal Profile `lab` but
  the local profile store does not define `lab`
- **THEN** alan preserves the `lab` reference in the workspace manifest
- **AND** alan does not delete the Space, terminal content, or missing reference
  during normal restore

### Requirement: Workspace Manifest Keeps Profile Reference Ownership Narrow
The macOS shell workspace manifest SHALL treat `terminal_profile_id` as a local
startup reference and SHALL NOT make Terminal Profile definitions portable
workspace state.

#### Scenario: Workspace is shared to another Mac
- **WHEN** a workspace manifest containing `terminal_profile_id` values is used
  on a Mac with different local Terminal Profiles
- **THEN** alan resolves matching local ids when available
- **AND** alan shows missing-profile fallback for unmatched ids
- **AND** alan does not attempt to synthesize profile definitions from the
  workspace manifest

### Requirement: Workspace Manifest Stores Space Presentation Icons
The macOS shell workspace manifest SHALL persist optional Space presentation
icon metadata separately from Terminal Profile definitions so the top Space
slider can render stable Space icons across launches without broadening profile
ownership.

#### Scenario: Space icon metadata is saved
- **WHEN** a Space has explicit presentation icon metadata
- **THEN** the workspace manifest stores that icon metadata on the Space record
- **AND** the `ShellSpace` projection exposes the same icon metadata for
  sidebar rendering
- **AND** the manifest does not treat the Space icon as a Terminal Profile icon,
  terminal content icon, command icon, or provider configuration field

#### Scenario: Old manifest decodes without Space icon metadata
- **WHEN** alan reads a valid workspace manifest created before Space
  presentation icons existed
- **THEN** alan decodes the manifest successfully
- **AND** each Space without icon metadata receives a deterministic default
  presentation icon for UI display
- **AND** alan does not rewrite the manifest solely because the default icon was
  applied for display

#### Scenario: Invalid Space icon metadata falls back safely
- **WHEN** alan reads a Space record whose presentation icon metadata is absent,
  empty, or unsupported by the local icon renderer
- **THEN** alan keeps the Space record and its Tabs intact
- **AND** alan displays the deterministic default Space icon for that Space
- **AND** alan preserves the original manifest evidence unless the user later
  explicitly changes the Space icon

#### Scenario: Terminal Profile reference ownership remains narrow
- **WHEN** a Space has both `terminal_profile_id` and Space presentation icon
  metadata
- **THEN** alan uses `terminal_profile_id` only as the Space's default terminal
  launch profile reference
- **AND** alan uses the Space presentation icon only for Space navigation
  surfaces
- **AND** changing one field does not silently rewrite the other

### Requirement: Legacy Quick Terminal Restore Data Is Discarded
The macOS shell workspace manifest loader SHALL tolerate old `quick_terminal`
restore records while discarding them during materialization and omitting them
from future manifest writes.

#### Scenario: Manifest records visible quick terminal
- **WHEN** Alan materializes shell state from a workspace manifest whose
  `quick_terminal` record has visible presentation
- **THEN** Alan discards the quick-terminal record
- **AND** Alan does not create a hidden or visible quick-terminal slot
- **AND** Alan does not create a terminal runtime, tab, pane, or detached panel
  from that record

#### Scenario: Manifest records hidden quick terminal
- **WHEN** Alan materializes shell state from a workspace manifest whose
  `quick_terminal` record has hidden presentation
- **THEN** Alan discards the quick-terminal record
- **AND** Alan restores only normal Spaces, Tabs, PaneSlots, ContentInstances,
  selected Space, and selected Tab from the manifest

#### Scenario: Manifest is written after legacy quick terminal data is read
- **WHEN** Alan writes a workspace manifest after reading a manifest that
  contained `quick_terminal`
- **THEN** the new manifest omits `quick_terminal`
- **AND** no quick-terminal transcript snapshot, last working directory, or
  presentation state is preserved

### Requirement: Workspace Manifest Stores Space-Local Tab Selection
The macOS shell workspace manifest SHALL persist each Space's remembered
selected Tab in addition to the globally selected Space and active Tab. Manifest
load, pruning, materialization, and writeback SHALL repair invalid Space-local
selected Tab references without deleting durable Spaces or fabricating Tabs for
empty Spaces.

#### Scenario: Manifest write stores each Space selection
- **WHEN** the user selects a non-first Tab in Space A
- **AND** the user selects a different Tab in Space B
- **THEN** the workspace manifest stores Space A's remembered selected Tab on the Space A record
- **AND** the workspace manifest stores Space B's remembered selected Tab on the Space B record
- **AND** the manifest still records the globally selected Space and active Tab for restart focus

#### Scenario: Restart restores inactive Space selections
- **WHEN** alan restarts from a workspace manifest with per-Space selected Tab records
- **THEN** the globally selected Space and active Tab are restored as the current shell focus
- **AND** every inactive Space keeps its remembered selected Tab for later Space switching
- **AND** switching to an inactive Space after restart selects that Space's remembered Tab instead of its first Tab

#### Scenario: Old manifest without Space-local selection decodes
- **WHEN** alan loads a valid workspace manifest that has global `selected_space_id` and `selected_tab_id` but no per-Space selected Tab fields
- **THEN** alan decodes the manifest successfully
- **AND** alan seeds the globally selected Space's remembered selected Tab from the global selected Tab when valid
- **AND** alan falls back to the first Tab for other Spaces until the user selects a Tab in those Spaces

#### Scenario: Selected Tab is pruned
- **WHEN** lifecycle pruning removes a Tab that a Space remembered as selected
- **THEN** alan repairs that Space's remembered selected Tab to the first retained Tab in the same Space
- **AND** if no Tabs remain in that Space, alan clears that Space's remembered selected Tab while keeping the Space record

#### Scenario: Tab moves between Spaces
- **WHEN** a Tab moves from one Space to another
- **THEN** alan repairs the source Space's remembered selected Tab if the moved Tab was remembered there
- **AND** alan preserves the destination Space's remembered selected Tab unless the move follows current selection or explicitly focuses the moved Tab
- **AND** the persisted manifest records the repaired source and destination Space selection outcomes

### Requirement: Terminal transcript snapshots restore the prior visible session context
The macOS shell workspace manifest SHALL persist terminal transcript snapshots
as bounded session-continuity state that can seed newly created terminal
runtimes after app restart.

#### Scenario: App closes with visible terminal output
- **WHEN** Alan closes or quits while a retained terminal ContentInstance has visible output and restorable transcript history
- **THEN** the workspace manifest stores a bounded transcript snapshot for that terminal content
- **AND** the snapshot preserves enough text, dimensions, title, cwd, focus, and viewport context to show the prior terminal state after restart

#### Scenario: App restarts after transcript snapshot
- **WHEN** Alan restores a terminal ContentInstance from a workspace manifest that contains a terminal transcript snapshot
- **THEN** Alan materializes the terminal with the saved transcript history before or during new runtime startup
- **AND** the restored terminal remains usable by starting a new shell in the restored cwd
- **AND** the normal terminal UI does not show an additional restored-session banner or warning surface solely because the transcript was restored

#### Scenario: Transcript snapshot is too large
- **WHEN** terminal history exceeds the configured row or encoded-byte snapshot limit
- **THEN** Alan stores a bounded tail snapshot and records truncation metadata
- **AND** manifest persistence remains bounded in size and time

### Requirement: Pinned tab templates are not silently rewritten by session snapshots
Pinned Tab structural restore SHALL remain governed by explicit pin snapshots,
while close-time terminal transcript snapshots MAY provide last-session history
without silently changing the pinned template.

#### Scenario: Pinned tab has matching live transcript
- **WHEN** a pinned Tab restores from its pin snapshot and a close-time transcript snapshot exists for matching terminal ContentInstance identity
- **THEN** Alan may seed the restored terminal runtime with that transcript history
- **AND** the pin snapshot's split layout, launch target, and explicit cwd template are not replaced by transient live-session state

#### Scenario: Pinned tab live structure no longer matches the pin snapshot
- **WHEN** a close-time transcript snapshot refers to terminal content that is not present in the pinned restore snapshot
- **THEN** Alan preserves the pinned restore snapshot behavior and ignores the unmatched transcript for that restored tab

### Requirement: macOS delegates portable manifest semantics to shell core
Alan for macOS SHALL delegate portable workspace manifest semantics to the Rust
shell core after the manifest module has Rust contract tests and adapter tests.

The macOS platform layer SHALL continue to own Application Support path
selection, file reads and writes, atomic persistence, corrupt-file evidence, and
diagnostics presentation.

#### Scenario: macOS loads a workspace manifest
- **WHEN** Alan for macOS reads a workspace manifest from disk after shell core
  manifest integration
- **THEN** macOS passes the manifest bytes and platform context to the shell
  core for decode, upgrade, materialization, and pruning semantics
- **AND** macOS remains responsible for preserving corrupt evidence and choosing
  the file path used for persistence

#### Scenario: Manifest output is persisted
- **WHEN** the shell core returns an updated manifest or manifest sync hint
- **THEN** the macOS platform layer writes the result through its persistence
  store
- **AND** the shell core does not directly access the user's file system

### Requirement: Manifest compatibility is preserved during migration
Rust-backed manifest behavior SHALL preserve compatibility with existing macOS
workspace manifest JSON unless a later spec explicitly changes the manifest
schema.

#### Scenario: Existing manifest is read by Rust-backed path
- **WHEN** Alan for macOS reads a manifest written by the current Swift
  implementation
- **THEN** the Rust-backed path decodes or upgrades it according to the existing
  manifest contract
- **AND** Space, Tab, PaneSlot, ContentInstance, selection, pin/live snapshot,
  lifecycle, Terminal Profile reference, and legacy `quick_terminal` discard
  semantics remain intact

### Requirement: Workspace manifest algorithms are shell-core authoritative at runtime
The macOS shell SHALL use Rust shell core for workspace manifest defaulting,
legacy migration, lifecycle pruning, and materialization into current shell
state. Swift SHALL own manifest file IO, corrupt-file preservation, and
platform diagnostics, but SHALL NOT retain a runtime Swift implementation of
the same portable manifest algorithms after shell core covers them.

#### Scenario: Missing manifest creates default through core
- **WHEN** Alan for macOS starts and no workspace manifest exists
- **THEN** Swift asks shell core to create the default workspace manifest
- **AND** Swift writes that manifest to the macOS manifest path
- **AND** Swift does not call a separate `ShellContentWorkspaceManifest`
  defaulting algorithm as a fallback

#### Scenario: Valid manifest materializes through core
- **WHEN** Alan for macOS loads a valid workspace manifest
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
- **AND** Swift does not restore from legacy shell-state snapshots as a domain
  fallback

#### Scenario: Core manifest authority fails
- **WHEN** shell core cannot create, prune, migrate, or materialize a workspace
  manifest
- **THEN** Alan for macOS reports an explicit shell-core manifest failure
- **AND** it does not silently launch from a Swift-computed workspace state for
  the same manifest

### Requirement: Shell persistence does not block the main thread
The macOS shell SHALL NOT perform any synchronous main-thread disk write on the
terminal metadata or runtime callback path. Every state file it persists on that
path — the workspace manifest, the shell-state snapshot, the control-plane
`state.json` mirror, and the control-plane change-event log — SHALL have its
encode + write deferred to a debounced flush and/or run off the main thread.

#### Scenario: High-output terminal does not stall the UI
- **WHEN** one or more terminals produce sustained high-frequency output
- **THEN** alan does not perform a synchronous main-thread disk write of the workspace manifest, the shell-state snapshot, or the control-plane state file on the terminal metadata or runtime callback path

#### Scenario: Encode and write run off the main thread
- **WHEN** alan persists the workspace manifest or the control-plane shell-state file
- **THEN** the JSON encode and atomic file write run on a background executor rather than blocking the main actor

#### Scenario: Control-plane in-memory publication stays prompt
- **WHEN** shell state changes on the terminal callback path
- **THEN** alan publishes the in-memory control-plane state promptly without waiting on a disk write

### Requirement: Workspace persistence cadence is separated by durability class
The macOS shell SHALL persist workspace state on cadences matched to each class
of state rather than rewriting and disk-writing every file on every runtime event:
- **Structural state** (Spaces, Tabs, order, pin state, pin snapshots, selected
  Space/Tab) SHALL be persisted when its mutation is accepted.
- **Restore content and runtime snapshot** (per-Tab terminal transcript snapshots
  in the manifest, and the control-plane shell-state file) driven by terminal
  callbacks SHALL be persisted on a bounded debounced cadence and SHALL be
  force-flushed on app background/resign-active and on quit.
- A change to transient runtime state (such as a Tab's active-task state) SHALL
  NOT by itself trigger a synchronous disk write.

#### Scenario: Structural mutation persists promptly
- **WHEN** the user creates, closes, reorders, pins, unpins, or moves a Tab or Space
- **THEN** alan persists the structural change for that mutation

#### Scenario: Active-task change is not a write trigger
- **WHEN** a Tab's terminal-aware active-task state changes
- **THEN** alan does not write the workspace manifest solely because of that change

#### Scenario: Transcript is flushed on background and quit
- **WHEN** Alan for macOS resigns active, is backgrounded, or is asked to quit
- **THEN** alan force-flushes pending transcript snapshots to disk before completing the transition

#### Scenario: Recent transcript persists within the bounded window
- **WHEN** a terminal's transcript changes and the app keeps running
- **THEN** alan persists the latest transcript snapshot within the configured debounce window
- **AND** a hard crash may lose at most that window of the most recent scrollback
