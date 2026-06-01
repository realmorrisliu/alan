# macos-shell-terminal-lifecycle Specification

## Purpose
Define the native macOS shell terminal lifecycle contract for pane-owned
terminal runtimes, truthful text delivery, stable runtime metadata, and
user-safe fallback states.
## Requirements
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

### Requirement: Zoom preserves sibling runtimes
The macOS shell host SHALL implement split zoom as view state that does not
close, recreate, or detach sibling terminal ContentInstance runtimes unnecessarily.

#### Scenario: Zoom hides siblings
- **WHEN** a PaneSlot with terminal content is zoomed
- **THEN** sibling terminal ContentInstances remain registered in the terminal runtime service and keep their scrollback, title, cwd, and pending delivery state

#### Scenario: Unzoom reattaches siblings
- **WHEN** the user exits zoom
- **THEN** sibling PaneSlots reappear by reattaching terminal views to their existing terminal ContentInstance runtime handles

### Requirement: Pane movement preserves runtime continuity
In-tab pane movement and drag/drop-backed movement SHALL move PaneSlot placement
without replacing the mounted ContentInstance identity or any terminal ContentInstance
runtime identity.

#### Scenario: In-tab movement
- **WHEN** a PaneSlot moves to another split position in the same tab
- **THEN** the PaneSlot keeps its mounted ContentInstance
- **AND** terminal content keeps its runtime handle, scrollback, title, cwd, and pending delivery state

#### Scenario: Drag/drop movement
- **WHEN** a PaneSlot moves through an enabled drag/drop affordance
- **THEN** the PaneSlot and mounted ContentInstance keep the same identities as the equivalent explicit move command

### Requirement: Terminal commands target the runtime owner
Copy, paste, and terminal search SHALL resolve the focused PaneSlot to mounted
terminal content and deliver to that terminal ContentInstance runtime or host
surface rather than to transient shell chrome.

#### Scenario: Copy terminal selection
- **WHEN** Copy is invoked and the focused terminal host owns a selection
- **THEN** the terminal host handles the copy operation without changing terminal ContentInstance runtime state

#### Scenario: Paste terminal input
- **WHEN** Paste is invoked for a focused PaneSlot that mounts terminal content
- **THEN** the paste operation is delivered through that terminal ContentInstance input path

#### Scenario: Search terminal content
- **WHEN** terminal search is invoked for a focused PaneSlot that mounts terminal content
- **THEN** search state follows that terminal ContentInstance runtime identity across view reconstruction

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

### Requirement: Terminal keyboard input is terminal-host owned
The macOS shell host SHALL route keyboard events for the focused terminal pane
through the terminal host unless a visible alan command surface or an explicit
app-reserved `Command` shortcut owns that key.

#### Scenario: Vim control key reaches terminal
- **WHEN** a focused terminal pane is running a TUI such as Vim and no alan command surface is visible
- **THEN** non-`Command` terminal keys such as Escape, Tab, Backspace, `Control-[`, `Control-W`, `Control-F`, and `Control-B` are delivered to the terminal runtime
- **AND** the shell workspace command router does not consume those keys as pane, tab, or command-input actions

#### Scenario: Printable physical keyboard input uses Ghostty key events
- **WHEN** a focused terminal pane receives printable physical keyboard input such as `a` or `:`
- **THEN** alan first lets AppKit text interpretation process the key so IME composition can start
- **AND** alan delivers committed printable input through the Ghostty key-event path
- **AND** alan does not bypass Ghostty's key encoder by sending that physical key through programmatic text injection

#### Scenario: IME composition can start from printable input
- **WHEN** a focused terminal pane uses a Chinese/Japanese/Korean input method
- **AND** the user types the first printable key of a composition
- **THEN** alan lets AppKit `interpretKeyEvents` create or update marked text
- **AND** alan updates Ghostty preedit state from the resulting marked text

#### Scenario: IME marked text owns composing backspace
- **WHEN** a focused terminal pane has active AppKit `NSTextInputClient` marked text from a Chinese/Japanese/Korean input method
- **AND** the user presses Backspace or an equivalent composing control key
- **THEN** alan lets AppKit `interpretKeyEvents` update or clear the marked text before terminal delivery
- **AND** alan updates Ghostty preedit state from the resulting marked text
- **AND** alan MUST NOT forward the composing control character to the terminal as a deletion of already-committed terminal input

#### Scenario: Ghostty binding wins for focused terminal
- **WHEN** Ghostty reports that a focused terminal key event is a terminal binding
- **THEN** alan sends the key event to the terminal runtime instead of treating it as an unresolved native command

#### Scenario: App-reserved command shortcut remains native
- **WHEN** a focused terminal pane receives an explicit app or workspace `Command` shortcut such as New Terminal Tab or Close Tab
- **THEN** alan executes the native workspace command and does not send that shortcut as terminal text

#### Scenario: Visible command surface owns its own keys
- **WHEN** alan's command input is visible while a terminal pane is focused
- **THEN** command-input keys such as submit, dismiss, and command-input toggle are handled by that surface before terminal delivery

#### Scenario: AppKit key equivalent is re-dispatched to terminal
- **WHEN** AppKit routes a focused terminal Control or Command key through `performKeyEquivalent`
- **AND** the key is not a visible command-surface key or explicit app/workspace shortcut
- **THEN** alan preserves Ghostty's key-equivalent state machine and allows AppKit to continue to `doCommand`
- **AND** `doCommand` re-dispatches the same event back through the terminal host
- **AND** the re-dispatched event is delivered to the terminal runtime exactly once

#### Scenario: Control slash is encoded like Ghostty
- **WHEN** a focused terminal pane receives `Control-/`
- **THEN** alan converts the key-equivalent text to `Control-_` before terminal delivery
- **AND** the event does not become an AppKit beep or an unresolved native command

#### Scenario: Focus-only split click is not injected into Vim
- **WHEN** the app and window are already active
- **AND** the user clicks a terminal split pane that is selected in the shell model but is not the AppKit first responder
- **THEN** alan focuses that terminal host and consumes the focus-transfer mouse down
- **AND** matching left mouse drags are suppressed until the focus-transfer mouse up
- **AND** the matching left mouse up is suppressed
- **AND** Vim mouse mode does not receive a stray click or selection drag from the focus transfer

#### Scenario: Terminal input router owns primary pointer sequence policy
- **WHEN** terminal pointer routing is evaluated for a focused or focus-transfer terminal pane
- **THEN** the macOS terminal surface controller owns the sequence policy for focus-only primary button events, normal-buffer selection drags, alternate-screen mouse delivery, mouse-reporting delivery, and unready-surface ignores
- **AND** the AppKit host view only normalizes events and executes the returned focus, deliver, consume, or fallthrough decision
- **AND** focus-transfer suppression state MUST NOT be split between separate host-view drag guards and surface pointer routing

#### Scenario: Modifier changes follow Ghostty semantics
- **WHEN** modifier keys change while IME marked text is active
- **THEN** alan does not forward the modifier transition to the terminal runtime
- **AND** outside IME composition alan preserves caps-lock and right-side modifier bits when building Ghostty key events

### Requirement: Shell child exit drives pane lifecycle
The macOS shell host SHALL treat terminal child-process exit as a lifecycle
event for the owning pane rather than as a request to clear, refresh, or
implicitly restart the terminal runtime.

#### Scenario: Exit closes split pane
- **WHEN** a pane in a split tab receives a normal shell child-exit signal from user input such as `exit`
- **THEN** alan closes only that pane through the normal pane-close path
- **AND** sibling panes keep their terminal runtime identities, scrollback, cwd, and focus eligibility

#### Scenario: Exit closes single-pane tab
- **WHEN** the only pane in a tab receives a normal shell child-exit signal and the tab can be closed
- **THEN** alan closes that tab through the normal tab-close path
- **AND** focus moves to the shell model's next valid tab or empty-space state

#### Scenario: Close-surface after child exit preserves exited metadata
- **WHEN** Ghostty reports a close-surface callback for a terminal surface whose child process is no longer alive
- **THEN** alan forwards a non-confirming close request from the surface host to the shell owner
- **AND** the shell owner closes the owning pane or tab through the normal close path
- **AND** alan preserves exited runtime metadata long enough for observers to see the terminal lifecycle transition
- **AND** releasing the Ghostty surface MUST NOT rewrite the pane metadata back to a running state before the controller observes the exit

#### Scenario: Final pane cannot close safely
- **WHEN** the final visible terminal pane receives a shell child-exit signal and closing it would leave the shell in an unsupported state
- **THEN** alan keeps an explicit exited pane state with terminal input disabled
- **AND** alan does not create a replacement shell runtime unless the user explicitly starts one

#### Scenario: Final pane closes into empty space
- **WHEN** the final visible terminal pane receives a shell child-exit signal and the shell model supports an empty focused space
- **THEN** alan closes the owning pane and tab through the normal close path
- **AND** the focused space remains available without creating a replacement terminal runtime

#### Scenario: Text delivery after exit is rejected
- **WHEN** text delivery targets a pane whose child process has exited and no replacement runtime was explicitly started
- **THEN** the runtime response reports failure with a stable child-exited reason

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

### Requirement: Terminal Startup Uses Resolved Terminal Profile
The macOS terminal lifecycle SHALL launch terminal content using the resolved
Terminal Profile while preserving the existing Ghostty-backed terminal surface
creation path.

#### Scenario: Terminal content starts with profile command
- **WHEN** terminal content is created with `terminal_profile_id` `alan`
- **AND** local Terminal Profile `alan` is a `sudo_user` profile for Unix user
  `alan`
- **THEN** alan resolves the terminal boot command to the structured sudo-user
  launch for `alan`
- **AND** Ghostty surface creation still receives the command, working
  directory, and environment through the existing terminal boot profile

#### Scenario: Profile metadata is projected to terminal environment
- **WHEN** terminal content starts with a resolved Terminal Profile
- **THEN** alan exposes non-secret profile metadata such as profile id and launch
  kind through terminal environment variables
- **AND** alan does not expose provider credentials or secret values through
  those variables

#### Scenario: Custom command startup is marked active
- **WHEN** terminal content starts with a `custom_command` Terminal Profile
- **THEN** alan treats the terminal startup as a foreground command until the
  terminal runtime reports completion or a shell-integration state update

### Requirement: Terminal Restore Reuses Stored Profile Reference
The macOS terminal lifecycle SHALL restore terminal content using its stored
Terminal Profile reference when one exists.

#### Scenario: Restored terminal uses stored profile
- **WHEN** alan restores terminal content from a workspace manifest with
  `terminal_profile_id` `univer`
- **THEN** alan launches the restored terminal using the current local
  definition of Terminal Profile `univer`

#### Scenario: Edited profile affects future restore
- **WHEN** terminal content stores `terminal_profile_id` `alan`
- **AND** the local `alan` Terminal Profile definition changes before app
  restart
- **THEN** the restored terminal uses the updated local `alan` profile
  definition

#### Scenario: Missing restored profile falls back
- **WHEN** alan restores terminal content with `terminal_profile_id` `lab`
- **AND** local Terminal Profile `lab` is missing
- **THEN** alan launches the restored terminal with the login-shell fallback
- **AND** alan reports the missing profile state in shell metadata
