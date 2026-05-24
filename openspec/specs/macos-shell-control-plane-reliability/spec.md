# macos-shell-control-plane-reliability Specification

## Purpose
Define reliability requirements for the macOS shell control plane, including
window-scoped identities, bounded IPC, authoritative mutation results, and
observable persistence or event failures.
## Requirements
### Requirement: Windows have isolated shell identities
The alan macOS shell control plane SHALL have one active primary shell window
identity per running native app instance. Duplicate window or duplicate process
launch paths MUST NOT create competing shell control directories, socket paths,
event streams, persisted shell state files, or terminal runtime registries.

#### Scenario: Opening a second window
- **WHEN** the user invokes a second-window path such as New Window or `Command-N`
- **THEN** alan focuses or reopens the existing primary shell window and does not create another `window_id`, socket path, control directory, or persisted state file

#### Scenario: Reading window state
- **WHEN** an agent queries the primary window's shell state
- **THEN** the response contains only spaces, tabs, panes, events, and focus state for the singleton primary shell window

#### Scenario: Forced duplicate process
- **WHEN** a forced second app process starts while the primary app instance owns the shell control plane
- **THEN** the second process exits without publishing a second socket, state file, event stream, or terminal runtime registry

#### Scenario: Reopening primary window
- **WHEN** the existing app process reopens the primary shell window after it was closed
- **THEN** the reopened window uses the app instance's singleton shell identity instead of allocating an independent window-scoped control plane

### Requirement: IPC requests are bounded
The local shell control socket SHALL bound request size, request duration, and
per-client work so a stalled or oversized client cannot block other control
requests indefinitely.

#### Scenario: Client never sends newline
- **WHEN** a socket client connects and does not complete a request within the configured deadline
- **THEN** the server closes that client and continues accepting later clients

#### Scenario: Client sends oversized request
- **WHEN** a socket client sends more than the maximum accepted request bytes
- **THEN** the server rejects that request, closes the client, and keeps serving later requests

#### Scenario: Main actor command handling is slow
- **WHEN** a command requires main-actor handling and the handler exceeds the response deadline
- **THEN** the server returns or records a timeout failure instead of hanging the socket loop

### Requirement: Mutations report authoritative results
Control-plane mutation commands SHALL return responses derived from authoritative
shell/runtime state after the requested mutation has been accepted or rejected.

#### Scenario: Missing target
- **WHEN** a mutation references a missing space, tab, or pane ID
- **THEN** the response reports `applied: false` with a stable error code

#### Scenario: Runtime-dependent mutation
- **WHEN** a mutation depends on terminal runtime availability
- **THEN** the response reflects whether the runtime accepted, queued, or rejected the operation

### Requirement: Persistence and event failures are observable
The shell control plane SHALL surface state, event, command, and binding file IO
failures through logs, diagnostics, or control responses instead of ignoring all
write/read errors.

#### Scenario: State file cannot be written
- **WHEN** publishing shell state fails to write the state file
- **THEN** the control plane records a diagnostic that can be inspected during debugging

#### Scenario: Command file cannot be decoded
- **WHEN** a file-command request cannot be decoded
- **THEN** the control plane records or writes a failure result rather than silently deleting the only evidence

### Requirement: Runtime-dependent commands use service state
The macOS shell control plane SHALL derive runtime-dependent terminal command
results from the terminal content runtime service after resolving the target
window, PaneSlot, and ContentInstance.

#### Scenario: Text delivery succeeds through runtime service
- **WHEN** `terminal.send_text` targets a terminal ContentInstance whose service-owned surface accepts the bytes
- **THEN** the response reports `applied: true`, the accepted byte count, the `content_id`, and the terminal runtime phase observed by the service

#### Scenario: Target slot has no terminal content
- **WHEN** a runtime-dependent terminal command targets a PaneSlot that shell state lists but that PaneSlot is empty or contains non-terminal content
- **THEN** the response reports `applied: false` with a stable unsupported-content error and does not claim delivery

#### Scenario: Target terminal content has no service handle
- **WHEN** a runtime-dependent terminal command targets a terminal ContentInstance that shell state still lists but the runtime service cannot resolve
- **THEN** the response reports `applied: false` with a stable runtime-missing error and does not claim delivery

### Requirement: Pending delivery is pane scoped and observable
If the runtime service supports queued text delivery, the queue SHALL be scoped
to one terminal ContentInstance and observable through shell diagnostics or
command responses.

#### Scenario: Text is queued for an attachable terminal content
- **WHEN** `terminal.send_text` targets an attachable terminal ContentInstance whose surface is not currently ready to accept text
- **THEN** the response reports queued state with the `content_id`, queued byte count, and delivery policy

#### Scenario: Queued text is flushed
- **WHEN** the terminal content surface becomes ready after text was queued
- **THEN** the runtime service flushes the content-specific queue and records whether the bytes were accepted or rejected

#### Scenario: Terminal content closes with queued text
- **WHEN** a terminal ContentInstance closes while text remains queued
- **THEN** the runtime service drops or fails that queue with a diagnostic tied to the closed `content_id`

### Requirement: Runtime service publishes command diagnostics
Runtime-dependent command failures SHALL be visible in control-plane responses
and diagnostics rather than only in view-local logs.

#### Scenario: Surface rejects text
- **WHEN** a service-owned surface rejects delivered text
- **THEN** the control response includes a stable error code and the service records a pane diagnostic for inspector/debug use

#### Scenario: Runtime command times out
- **WHEN** the runtime service cannot complete a runtime-dependent command inside the control-plane deadline
- **THEN** the response reports timeout without blocking later control requests for the same window

### Requirement: Pane workspace mutation commands report authoritative results
The macOS shell control plane SHALL return authoritative results for PaneSlot
split, PaneSlot close, PaneSlot lift, cross-tab PaneSlot move, and direct
PaneSlot focus commands after the mutation is accepted or rejected.

#### Scenario: Split command succeeds
- **WHEN** a control client requests a valid directional PaneSlot split
- **THEN** the response reports `applied: true` and includes the resulting focused `pane_slot_id`

#### Scenario: Split command invalid
- **WHEN** a control client requests a PaneSlot split against a missing slot or without a direction
- **THEN** the response reports `applied: false` with a stable error code and leaves shell state unchanged

#### Scenario: Move command succeeds
- **WHEN** a control client moves a PaneSlot to a valid destination tab in the same window
- **THEN** the response reports `applied: true` and the resulting focused `pane_slot_id` while preserving the PaneSlot and ContentInstance identities

#### Scenario: Close command succeeds
- **WHEN** a control client closes a PaneSlot
- **THEN** the response reflects both shell model removal and the remaining focused PaneSlot

### Requirement: Pane focus commands are observable
Direct pane focus commands SHALL report whether focus changed to the requested
PaneSlot or why the target could not be focused.

#### Scenario: Direct focus changes
- **WHEN** a control client requests focus for an existing PaneSlot
- **THEN** the response reports `applied: true` and the requested `pane_slot_id`

#### Scenario: Direct focus target missing
- **WHEN** a control client requests focus for a missing PaneSlot
- **THEN** the response reports `applied: false` with a stable missing-pane error and preserves existing focus

### Requirement: Workspace mutation events are observable
Workspace mutations SHALL emit shell events with enough detail for agents to
observe PaneSlot creation, closure, movement, content creation/closure,
terminal metadata changes, attention changes, and focus changes.

#### Scenario: Split creates a pane
- **WHEN** the user or a control client creates a split
- **THEN** the shell event stream records the created PaneSlot, mounted ContentInstance, and tab

#### Scenario: Move changes a pane tab
- **WHEN** the user or a control client moves a PaneSlot to another tab
- **THEN** the shell event stream records the previous and current tab or space identity for the moved PaneSlot

#### Scenario: Focus changes
- **WHEN** the user or a control client changes focused pane
- **THEN** the shell event stream records the previous and current focused PaneSlot IDs

### Requirement: Advanced split control commands report authoritative results
The macOS shell control plane SHALL return authoritative results for split
resize, equalize, zoom, unzoom, and spatial focus commands.

#### Scenario: Resize command succeeds
- **WHEN** a control client requests a valid split ratio change
- **THEN** the response reports `applied: true` and includes the resulting ratio and affected split or PaneSlot IDs

#### Scenario: Equalize command succeeds
- **WHEN** a control client requests equalize for a tab with split branches
- **THEN** the response reports `applied: true` and identifies the tab whose split ratios were reset

#### Scenario: Zoom command succeeds
- **WHEN** a control client zooms or unzooms a valid pane
- **THEN** the response reports `applied: true`, the `pane_slot_id`, and the tab zoom state

#### Scenario: Spatial focus has no target
- **WHEN** a control client requests spatial focus and no adjacent pane exists
- **THEN** the response reports `applied: false` with a stable no-target error and preserves existing focus

### Requirement: Advanced movement commands report source and destination
Pane move commands SHALL report enough source and destination detail for agents
to observe layout changes without inferring them from raw shell snapshots.

#### Scenario: In-tab move succeeds
- **WHEN** a control client moves a PaneSlot within a tab
- **THEN** the response reports the `pane_slot_id`, source tab, destination tab, direction or position, and preserved mounted ContentInstance identity

#### Scenario: Drag-backed move succeeds
- **WHEN** a drag/drop affordance completes through the control-plane movement path
- **THEN** the response and event stream use the same result semantics as explicit movement commands

### Requirement: Advanced command outcomes emit shell events
Advanced workspace mutations SHALL emit shell events for zoom state, split ratio
changes, equalization, spatial focus, and pane movement.

#### Scenario: Split ratio changes
- **WHEN** a split ratio changes through UI or control-plane resize
- **THEN** the shell event stream records the affected split, tab, and resulting ratio

#### Scenario: Zoom state changes
- **WHEN** a pane is zoomed or unzoomed
- **THEN** the shell event stream records the tab, pane, and resulting zoom state

### Requirement: Tab Organization Mutations Report Authoritative Results
The macOS shell control plane and local command paths SHALL return results
derived from authoritative shell state after Tab reorder, pin/unpin, and Move to
Space mutations are accepted or rejected.

#### Scenario: Reorder succeeds
- **WHEN** a Tab reorder mutation is accepted
- **THEN** the result reports `applied: true` with the Tab ID, Space ID, section,
  and resulting index

#### Scenario: Pin succeeds
- **WHEN** a Tab pin mutation is accepted
- **THEN** the result reports `applied: true`, the Tab's pinned state, and the
  resulting section/index

#### Scenario: Move to Space succeeds
- **WHEN** a Tab moves to another Space
- **THEN** the result reports `applied: true`, source Space, target Space,
  section, resulting index, and resulting focused Space/Tab when focus changes

#### Scenario: Mutation is invalid
- **WHEN** a Tab organization mutation references a missing Tab, missing Space,
  invalid section, or invalid index
- **THEN** the result reports `applied: false` with a stable error code and
  leaves shell state unchanged

### Requirement: Tab Organization Events Are Observable
Tab organization mutations SHALL emit shell events with enough detail for
diagnostics and agents to observe ordering, pin state, Space movement, and focus
outcomes.

#### Scenario: Tab reordered
- **WHEN** a Tab order changes
- **THEN** the shell event stream records the Tab ID, Space ID, previous section
  and index, and current section and index

#### Scenario: Tab moved to another Space
- **WHEN** a Tab moves to another Space
- **THEN** the shell event stream records the previous and current Space,
  section, index, and focus outcome

### Requirement: Control plane separates pane and content commands
macOS shell control plane SHALL 区分通用 pane mutation、content creation 和 content-specific
runtime command，并从 authoritative shell/content/runtime state 返回结果。

#### Scenario: Pane split creates requested content
- **WHEN** control client 请求在目标 pane 旁创建 split，并指定 `markdown` content intent
- **THEN** response 报告 `applied: true`
- **AND** response 包含新 `pane_slot_id`、tab ID、`content_id`、content kind 和 resulting shell state

#### Scenario: Terminal text targets terminal content
- **WHEN** `terminal.send_text` 目标 PaneSlot 承载 `terminal` ContentInstance 且 runtime 接受 bytes
- **THEN** response 报告 `applied: true`、`content_id`、accepted byte count 和 terminal runtime phase

#### Scenario: Terminal text targets non-terminal content
- **WHEN** `terminal.send_text` 目标 PaneSlot 承载 markdown 或 settings content
- **THEN** response 报告 `applied: false`
- **AND** response 使用 stable unsupported-content error code
- **AND** alan 不声明 accepted bytes、不创建 terminal runtime、不改变 content state

### Requirement: Shell state exposes content descriptors
Control-plane shell state responses SHALL 为每个 PaneSlot 暴露 `pane_slot_id`、挂载的
`content_id`、content kind、用户可见 title、capabilities 和必要的安全引用，使 agent 可以判断哪些命令合法。

#### Scenario: Agent reads mixed shell state
- **WHEN** agent 查询 shell state
- **THEN** response 中包含 `pane_slots` 和 `contents`
- **AND** 每个 PaneSlot 可以解析到当前挂载的 ContentInstance
- **AND** terminal-only runtime metadata 只出现在 terminal content projection 中

#### Scenario: Unsupported command is inspectable
- **WHEN** agent 对不支持的 content 执行 content-specific command
- **THEN** response 包含稳定错误码和目标 content kind
- **AND** event/diagnostic surface 可以显示该 rejected command
