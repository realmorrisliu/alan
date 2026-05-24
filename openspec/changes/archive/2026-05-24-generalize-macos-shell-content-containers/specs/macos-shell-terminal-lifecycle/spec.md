## MODIFIED Requirements

### Requirement: Terminal runtimes survive view selection changes
The macOS shell host SHALL keep terminal process, renderer surface, runtime metadata,
and buffered control state owned by terminal ContentInstances through the terminal
runtime service rather than by the transient SwiftUI/AppKit view that happens to be visible.
Runtime continuity applies while the Tab remains part of current shell state; explicit
close operations and workspace lifecycle retirement of inactive unpinned Tabs SHALL
finalize affected terminal ContentInstances through the runtime service boundary.

#### Scenario: Switching away from a tab
- **WHEN** a user switches from one tab to another and the first tab is no longer rendered
- **THEN** each terminal ContentInstance in the first tab remains alive unless its PaneSlot, content, tab, or workspace lifecycle is explicitly closed or retired

#### Scenario: Switching back to a tab
- **WHEN** a user returns to a previously selected tab
- **THEN** the host reattaches visible terminal views to existing terminal ContentInstance runtimes instead of booting new shell processes

#### Scenario: Closing a tab
- **WHEN** a tab is explicitly closed
- **THEN** all terminal ContentInstances owned by that tab are finalized exactly once through the runtime service and their final state is reflected in shell state

#### Scenario: Retiring an inactive unpinned Tab
- **WHEN** workspace lifecycle pruning retires an inactive unpinned Tab
- **THEN** all terminal ContentInstances owned by that Tab are finalized through the same runtime service ownership boundary used by explicit close operations
- **AND** non-terminal ContentInstances in that Tab follow their content-specific finalization path without invoking terminal runtime finalizers
- **AND** retired PaneSlots and terminal ContentInstances are no longer valid terminal delivery targets

#### Scenario: Restoring a Tab after app restart
- **WHEN** alan restores a Pinned Tab or retained Unpinned Tab from the workspace manifest after app restart
- **THEN** alan materializes terminal ContentInstances from the restore snapshot
- **AND** alan creates new terminal runtimes for those ContentInstances instead of claiming continuity with processes from the prior app instance

### Requirement: Pane text delivery is truthful
The macOS shell host SHALL only acknowledge terminal text delivery as applied when the
target terminal ContentInstance runtime accepts the text or queues it in a durable
content-specific delivery buffer that will be flushed when the runtime is attached.

#### Scenario: Visible terminal content accepts text
- **WHEN** `terminal.send_text` targets a visible PaneSlot with attached terminal content and a ready runtime
- **THEN** the response reports `applied: true`, includes the accepted byte count, and identifies the terminal `content_id`

#### Scenario: Background terminal content accepts text
- **WHEN** `terminal.send_text` targets a background PaneSlot with existing terminal content and runtime state
- **THEN** the text is delivered to that terminal ContentInstance without requiring the tab to become visible

#### Scenario: Target slot cannot accept text
- **WHEN** `terminal.send_text` targets a missing, closed, non-terminal, or not-yet-bootable PaneSlot or ContentInstance
- **THEN** the response reports `applied: false` with a specific error code and does not claim accepted bytes

### Requirement: Focus and metadata follow runtime identity
The macOS shell host SHALL associate cwd, title, process status, attention,
renderer phase, and last-command metadata with stable terminal ContentInstance IDs,
while shell focus remains associated with stable PaneSlot IDs.

#### Scenario: Runtime metadata arrives for a background terminal content
- **WHEN** a background terminal ContentInstance reports cwd, title, process, or attention changes
- **THEN** the shell state for that content updates without changing the user's selected tab or focused PaneSlot

#### Scenario: Visible focus changes
- **WHEN** the user focuses a visible PaneSlot
- **THEN** shell state updates the focused PaneSlot while preserving runtime records for all terminal ContentInstances
- **AND** terminal focus side effects run only when the focused PaneSlot mounts terminal content

### Requirement: Host fallback state is user-safe
The macOS shell host SHALL make unavailable Ghostty or failed terminal runtime
states explicit and actionable without presenting a fake usable terminal.

#### Scenario: Ghostty is unavailable
- **WHEN** the app launches without a linked or loadable Ghostty runtime
- **THEN** each affected terminal ContentInstance reports a non-ready terminal state and the UI provides setup/debug information without accepting terminal input as if it succeeded

#### Scenario: Surface creation fails
- **WHEN** a terminal surface cannot be created for a terminal ContentInstance
- **THEN** the content records the failure reason and control-plane mutations against that terminal content fail or queue according to the delivery contract

### Requirement: Surface readiness is lifecycle metadata
The macOS shell host SHALL track surface readiness, input readiness, renderer
health, child process status, readonly state, and terminal mode as runtime
metadata associated with stable terminal ContentInstance IDs.

#### Scenario: Surface becomes input ready
- **WHEN** a terminal content surface finishes creation and can accept terminal input
- **THEN** terminal lifecycle metadata records input-ready state and pending delivery may flush according to the delivery contract

#### Scenario: Renderer becomes unhealthy
- **WHEN** a terminal renderer reports degraded or failed health
- **THEN** terminal content lifecycle metadata records that state and terminal input/delivery responses remain truthful

#### Scenario: Child exits
- **WHEN** the terminal child process exits
- **THEN** terminal content lifecycle metadata records exit status and later text delivery does not claim success unless a new runtime is explicitly started

### Requirement: Terminal mode changes survive view changes
The macOS shell host SHALL keep terminal mode metadata such as alternate screen,
mouse reporting, search state, and readonly state with terminal ContentInstance
runtime identity rather than with transient host views or PaneSlot layout identity.

#### Scenario: View recreated during alternate screen
- **WHEN** a terminal view is recreated while an alternate-screen application is active
- **THEN** the replacement view reflects the terminal ContentInstance's current terminal mode rather than reverting to normal-buffer assumptions

#### Scenario: Background terminal exits readonly mode
- **WHEN** background terminal content changes readonly or input readiness state
- **THEN** terminal content metadata updates without selecting that tab

### Requirement: Terminal lifecycle ownership is service backed
The macOS shell host SHALL route terminal process, renderer surface, runtime
metadata, pending delivery buffer, and teardown ownership through the terminal
runtime service keyed by terminal ContentInstance identity rather than through
transient host views or PaneSlot layout identity.

#### Scenario: Runtime survives SwiftUI reconstruction
- **WHEN** SwiftUI reconstructs the shell content view while terminal content remains mounted in shell state
- **THEN** the terminal runtime service keeps the terminal content surface alive and the new view attaches to the same `content_id` runtime identity

#### Scenario: Runtime no longer exists
- **WHEN** shell state references terminal content whose runtime has irrecoverably failed or closed
- **THEN** lifecycle metadata reports the non-ready state and the UI/control plane do not treat that terminal content as ready

### Requirement: Pane close finalizes runtime identity
The macOS shell host SHALL make PaneSlot, content, tab, and window close operations
call the runtime service finalizer for each affected terminal ContentInstance before
the terminal content is removed from authoritative runtime state.

#### Scenario: Closing a split pane
- **WHEN** a user closes one PaneSlot in a split tab
- **THEN** the runtime service finalizes the mounted terminal ContentInstance only if that PaneSlot contains terminal content
- **AND** remaining terminal ContentInstances keep their runtime identities

#### Scenario: Closing a window
- **WHEN** a shell window closes
- **THEN** every terminal ContentInstance runtime owned by that window transitions to closing or closed state before the window control identity is released

### Requirement: Reattachment preserves terminal continuity
Visible terminal views SHALL reattach to existing terminal ContentInstance runtime
handles and MUST NOT restart shell processes, clear scrollback, or reset terminal
metadata solely because selection, split layout, PaneSlot mounting, or window visibility changed.

#### Scenario: Tab selection changes repeatedly
- **WHEN** a user switches between terminal tabs several times
- **THEN** each terminal ContentInstance keeps its existing terminal process, scrollback, title, cwd, and runtime phase

#### Scenario: Split layout changes
- **WHEN** a PaneSlot with terminal content is moved, resized, or temporarily hidden by split zoom
- **THEN** its terminal ContentInstance runtime handle remains continuous and reattaches when visible again

### Requirement: Terminal-area events are owned by the terminal host
The macOS shell host SHALL route mouse events that occur inside terminal pixels
through the terminal ContentInstance's AppKit terminal host rather than through
SwiftUI tap gesture wrappers around the terminal view.

#### Scenario: First click activates and reaches the terminal
- **WHEN** a user clicks a visible terminal PaneSlot that is not currently selected
- **THEN** the shell selects that PaneSlot, makes its terminal host first responder, and forwards the same mouse-down event to the terminal renderer

#### Scenario: Terminal text selection starts on first drag
- **WHEN** a user begins a drag inside a visible terminal PaneSlot
- **THEN** the drag is handled by the terminal host and can start terminal text selection without requiring a prior selection-only click

#### Scenario: Terminal host lifetime remains content-keyed
- **WHEN** SwiftUI recreates the terminal leaf view for an existing terminal ContentInstance
- **THEN** the registry reuses the content-keyed terminal host and refreshes its weak activation boundary for the current PaneSlot without transferring terminal event ownership to the SwiftUI view

### Requirement: Terminal activation does not retain shell controllers
Registry-owned terminal host views SHALL use a weak activation boundary when
requesting PaneSlot selection from the shell controller.

#### Scenario: Host requests activation
- **WHEN** a terminal host receives a mouse-down event for terminal content mounted in a stable PaneSlot
- **THEN** it calls the weak activation boundary for that PaneSlot before requesting terminal focus

#### Scenario: Activation boundary is unavailable
- **WHEN** a terminal host has no activation delegate available
- **THEN** terminal input handling remains local to the host and the host does not keep a strong closure that can retain the shell controller

### Requirement: Split workspace mutations preserve live runtimes
The macOS shell host SHALL preserve terminal ContentInstance runtime identity across
split resize, equalize, focus, pane lift, and cross-tab PaneSlot move operations unless
the operation explicitly closes or replaces that terminal content.

#### Scenario: Resize split
- **WHEN** the user resizes a split divider
- **THEN** all terminal ContentInstances in the tab keep their existing runtime handles and metadata

#### Scenario: Equalize splits
- **WHEN** the user equalizes splits in a tab
- **THEN** all terminal ContentInstances in the tab keep their existing runtime handles and metadata

#### Scenario: Lift pane
- **WHEN** the user lifts a PaneSlot with terminal content to its own tab
- **THEN** the terminal ContentInstance keeps its runtime handle, scrollback, title, cwd, and pending delivery state

#### Scenario: Move pane to another tab
- **WHEN** the user moves a PaneSlot with terminal content to another tab within the same window
- **THEN** the terminal ContentInstance keeps its runtime handle, scrollback, title, cwd, and pending delivery state

### Requirement: Split close operations define runtime finalization
The macOS shell host SHALL define explicit terminal runtime finalization
semantics for close PaneSlot, close tab, close window, pane lift, and PaneSlot move
operations that empty containers.

#### Scenario: Close focused pane
- **WHEN** the user invokes close pane
- **THEN** alan finalizes exactly the terminal ContentInstance mounted in that PaneSlot, if any, and repairs the split tree around the removed leaf

#### Scenario: Close tab after moving last pane
- **WHEN** a move operation leaves the source tab empty and alan closes that tab
- **THEN** alan does not finalize the moved terminal ContentInstance runtime as part of source tab cleanup

## ADDED Requirements

### Requirement: Terminal lifecycle is scoped to terminal content
macOS shell 的 terminal runtime lifecycle SHALL 只适用于 `terminal` ContentInstance。非 terminal
ContentInstance MUST NOT 分配 terminal runtime、shell process、Ghostty surface 或 terminal delivery queue。

#### Scenario: Settings pane becomes visible
- **WHEN** 用户选择承载 settings content 的 pane
- **THEN** alan 渲染 settings surface
- **AND** terminal runtime registry 不为该 PaneSlot 或 ContentInstance 创建 shell process 或 Ghostty host

#### Scenario: Markdown pane receives terminal text command
- **WHEN** terminal text command 的目标 PaneSlot 承载 markdown content
- **THEN** terminal lifecycle 不接收该 delivery
- **AND** control response 报告 stable unsupported-content error

#### Scenario: Live terminal pane remains continuous after model migration
- **WHEN** 当前进程内仍属于 shell state 的 terminal pane 从旧 terminal-only model 迁移到 content-container model
- **THEN** 该 terminal ContentInstance 的 process、scrollback、metadata、pending delivery 和 reattachment 语义保持连续
- **AND** runtime continuity 绑定到 `content_id`

#### Scenario: Terminal content restored from workspace manifest
- **WHEN** alan after app restart 从 workspace manifest materialize 出 terminal ContentInstance
- **THEN** terminal lifecycle 创建新的 terminal runtime 和 renderer surface
- **AND** alan MUST NOT 声称恢复上一轮 app 进程中的 terminal process、scrollback 或 delivery queue
- **AND** runtime continuity 从本轮 materialization 之后开始绑定到该 `content_id`

### Requirement: Terminal adapter owns terminal-specific projection
Terminal content adapter SHALL 负责将 terminal runtime metadata 投影为 shell-visible title、cwd、
attention、surface readiness、alan binding 和 terminal command capabilities，并以 `content_id`
作为 runtime identity。

#### Scenario: Background terminal metadata updates
- **WHEN** 后台 terminal content 报告 cwd、title、attention 或 process status 变化
- **THEN** terminal adapter 更新该 content/pane 的 shell projection
- **AND** 用户当前聚焦的非 terminal pane 不被抢占

#### Scenario: Terminal content closes
- **WHEN** 用户关闭承载 terminal content 的 pane
- **THEN** terminal adapter 以 `content_id` 调用 terminal runtime finalizer
- **AND** shell layout 删除该 PaneSlot 后不保留可投递 terminal target

#### Scenario: Terminal content moves between pane slots
- **WHEN** terminal ContentInstance 从一个 PaneSlot 移动或重挂到另一个 PaneSlot
- **THEN** terminal runtime handle、scrollback、pending delivery 和 metadata 保持绑定到同一个 `content_id`
- **AND** terminal host focus 解析到新的 PaneSlot 位置
