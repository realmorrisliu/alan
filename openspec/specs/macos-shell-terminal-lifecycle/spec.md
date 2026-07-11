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
through the terminal host unless a visible supported shell control or an
explicit app-reserved `Command` shortcut owns that key.

#### Scenario: Vim control key reaches terminal
- **WHEN** a focused terminal pane is running a TUI such as Vim and no supported
  shell control is active
- **THEN** non-`Command` terminal keys such as Escape, Tab, Backspace, `Control-[`, `Control-W`, `Control-F`, and `Control-B` are delivered to the terminal runtime
- **AND** the shell workspace command router does not consume those keys as
  pane, tab, or removed command-input actions

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

#### Scenario: Supported shell control owns its own keys
- **WHEN** a supported transient shell control such as the Find bar, a titlebar
  control, or a menu-owned interaction is active while a terminal pane is
  focused
- **THEN** keys owned by that control, such as submit, dismiss, navigation, or
  toggle keys, are handled by that control before terminal delivery

#### Scenario: AppKit key equivalent is re-dispatched to terminal
- **WHEN** AppKit routes a focused terminal Control or Command key through `performKeyEquivalent`
- **AND** the key is not a supported shell-control key or explicit
  app/workspace shortcut
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
creation path. When no explicit or Space-bound Terminal Profile applies, terminal
startup SHALL use the built-in `Login shell` identity.

#### Scenario: Terminal content starts with profile command
- **WHEN** terminal content is created with `terminal_profile_id` `alan`
- **AND** local Terminal Profile `alan` is a `sudo_user` profile for Unix user
  `alan`
- **THEN** alan resolves the terminal boot command to the structured sudo-user
  launch for `alan`
- **AND** Ghostty surface creation still receives the command, working
  directory, and environment through the existing terminal boot profile

#### Scenario: Unbound terminal starts login shell
- **WHEN** terminal content is created without an explicit
  `terminal_profile_id`
- **AND** the selected Space has no `terminal_profile_id`
- **THEN** alan resolves terminal startup to the current user's login shell
- **AND** alan does not capture a separate global default Terminal Profile for
  that terminal content

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

### Requirement: Destructive terminal close requests are guarded
The macOS shell host SHALL guard destructive pane, tab, window, and app close
requests before mutating authoritative shell state or releasing terminal
ContentInstance runtimes.

#### Scenario: Closing a pane with active work
- **WHEN** the user requests close for a PaneSlot that mounts terminal content with a foreground command, running Alan CLI task, pending yield, or unknown live active-task state
- **THEN** Alan asks for confirmation before removing the PaneSlot or finalizing the terminal ContentInstance runtime
- **AND** cancelling the confirmation leaves shell state, workspace manifest state, and terminal runtime state unchanged

#### Scenario: Closing an idle terminal pane
- **WHEN** the user requests close for a PaneSlot whose terminal content is an idle shell prompt or an exited process
- **THEN** Alan may close the PaneSlot without an active-work confirmation

#### Scenario: Closing a tab with multiple terminal panes
- **WHEN** the user requests close for a tab containing multiple terminal ContentInstances and at least one has active work
- **THEN** Alan presents at most one confirmation for the tab close request
- **AND** the tab is removed only after the user confirms

#### Scenario: Closing a window or quitting the app
- **WHEN** the user requests window close or app quit while any affected terminal ContentInstance has active work
- **THEN** Alan presents at most one confirmation for that requested close scope
- **AND** no affected terminal runtime is finalized until the user confirms

### Requirement: Confirmed close captures terminal session snapshots
The macOS shell host SHALL attempt to capture bounded terminal transcript
snapshots for affected live terminal ContentInstances after a destructive
terminal close request is confirmed, after a bounded graceful shutdown attempt
for active terminal work, and before finalizing their runtimes.

#### Scenario: Confirmed pane close captures a snapshot
- **WHEN** the user confirms closing a terminal PaneSlot with restorable terminal history
- **THEN** Alan first requests graceful shutdown for active foreground terminal work when applicable
- **AND** Alan captures a bounded transcript snapshot for the mounted terminal ContentInstance before invoking runtime finalization
- **AND** the snapshot is associated with the terminal ContentInstance identity and close reason

#### Scenario: Graceful shutdown output is captured
- **WHEN** a confirmed close affects terminal work that can print final session or resume metadata while shutting down
- **THEN** Alan gives the runtime a bounded graceful shutdown window before forced finalization
- **AND** Alan captures transcript history after the graceful window drains or times out so final output can be restored after restart

#### Scenario: Graceful shutdown times out
- **WHEN** the affected terminal work does not exit or return to an idle prompt within the bounded graceful shutdown window
- **THEN** Alan captures the latest available transcript tail
- **AND** Alan may force-finalize the runtime after the timeout instead of blocking app quit or close indefinitely

#### Scenario: Snapshot capture fails after confirmation
- **WHEN** the user has confirmed a destructive close and snapshot capture or persistence fails
- **THEN** Alan records a diagnostic for debugging
- **AND** Alan may continue the confirmed close instead of trapping the user in the closing surface

#### Scenario: App restart restores history but not process continuity
- **WHEN** Alan restarts after terminal ContentInstances were closed or interrupted in the prior app instance
- **THEN** restored terminal panes may present saved transcript history from the prior session
- **AND** Alan creates new terminal runtimes and child processes instead of claiming continuity with the prior app instance's PTYs, child processes, or Ghostty surfaces

### Requirement: Alan-owned terminal process lifecycle is authoritative
The macOS shell host SHALL treat Alan runtime service process state as
authoritative for terminal lifecycle, close guards, control-plane delivery, and
metadata projection when a terminal ContentInstance uses the Alan-owned PTY
runtime path.

#### Scenario: Foreground process changes
- **WHEN** the Alan-owned PTY runtime observes foreground process or process-group changes
- **THEN** shell lifecycle metadata updates the corresponding terminal ContentInstance
- **AND** the update does not depend on the terminal view being visible

#### Scenario: Renderer reports stale process state
- **WHEN** renderer metadata conflicts with Alan-owned process lifecycle state
- **THEN** Alan-owned runtime state wins for child-process status, close guards, signal eligibility, and text-delivery acceptance
- **AND** renderer metadata may be retained as diagnostics

### Requirement: Terminal shutdown uses Alan-owned process control
For Alan-owned PTY runtimes, confirmed close and runtime shutdown SHALL use
Alan-owned process and process-group controls before finalizing terminal
ContentInstance state.

#### Scenario: Graceful close is confirmed
- **WHEN** a user confirms closing terminal content with active foreground work
- **THEN** Alan requests graceful shutdown through the Alan-owned PTY/process runtime
- **AND** Alan observes bounded output or exit state before force finalization policy runs

#### Scenario: Force close is required
- **WHEN** graceful shutdown times out or the process ignores the request
- **THEN** Alan may escalate through configured process-group signal policy
- **AND** the final shell state reports interrupted or forced shutdown metadata without exposing raw process handles

### Requirement: Runtime replacement does not claim cross-app continuity

Alan-owned PTY runtime ownership SHALL improve in-process terminal control, but MUST NOT claim terminal process continuity across Alan app termination.

#### Scenario: App restarts after Alan-owned PTY runtime

- **WHEN** Alan restores a terminal ContentInstance after app restart
- **THEN** Alan creates a new runtime from persisted snapshot data
- **AND** Alan does not claim that the prior PTY, process group, foreground application, or file descriptors are still live

#### Scenario: Cross-app continuity is proposed later

- **WHEN** a future change proposes PTY survival across app termination
- **THEN** that change defines the lifecycle owner, persistence semantics, security boundary, and failure behavior in OpenSpec before exposing continuity
- **AND** this cleanup does not preselect the owning service or attachment mechanism

### Requirement: Terminal delivery follows PTY readiness
For Alan-owned PTY runtimes, terminal text delivery SHALL be acknowledged only
after Alan-owned PTY input accepts or durably queues the bytes according to the
terminal ContentInstance delivery policy.

#### Scenario: PTY accepts input
- **WHEN** `terminal.send_text` targets terminal content with an input-ready Alan-owned PTY runtime
- **THEN** the response reports `applied: true`, accepted byte count, and terminal `content_id`
- **AND** the response does not depend on renderer visibility

#### Scenario: Renderer is ready but PTY is closed
- **WHEN** `terminal.send_text` targets terminal content whose renderer is still attached but whose Alan-owned PTY is closed
- **THEN** the response reports `applied: false` with a stable closed-runtime error
- **AND** no accepted bytes are claimed

### Requirement: Helper Managed User Sessions Have Truthful Lifecycle States
The macOS shell host SHALL represent helper-backed Managed User terminal
sessions with explicit lifecycle and error states for helper availability,
authorization, account readiness, PTY spawn, renderer attachment, child exit,
and cleanup.

#### Scenario: Helper is unavailable during launch
- **WHEN** a terminal launch resolves to a `managed_user` profile and the
  privileged helper is missing, outdated, invalid, or unreachable
- **THEN** the terminal ContentInstance records a non-ready helper state
- **AND** the UI and control plane do not report terminal input as accepted by a
  live managed-user process

#### Scenario: Helper rejects launch
- **WHEN** the helper rejects `startManagedUserPTY` because the account is not
  Alan managed, not ready, invalid, or not allowed for the current channel
- **THEN** the terminal ContentInstance records the sanitized helper rejection
  state
- **AND** Alan does not retry through sudoers or an unmanaged command path

#### Scenario: Renderer attachment fails after PTY starts
- **WHEN** the helper starts a managed-user PTY session but Ghostty attachment
  fails
- **THEN** Alan records renderer failure separately from helper PTY creation
- **AND** the helper session is terminated or cleaned up according to terminal
  close policy

#### Scenario: Managed user child exits
- **WHEN** the helper reports that a managed-user child process exited
- **THEN** terminal lifecycle metadata records exit status and helper session
  finality
- **AND** later text delivery does not claim success unless a new runtime is
  explicitly started

### Requirement: Helper Session Cleanup Follows Terminal Ownership
The macOS shell host SHALL close helper-backed Managed User PTY sessions through
the same terminal ContentInstance runtime finalization boundary used by ordinary
terminal runtimes.

#### Scenario: Managed user pane is closed
- **WHEN** a user closes a PaneSlot that mounts helper-backed Managed User
  terminal content
- **THEN** the runtime service finalizes the terminal ContentInstance exactly
  once
- **AND** the helper receives the corresponding terminate or cleanup request for
  the managed-user PTY session

#### Scenario: Client connection is lost
- **WHEN** Alan loses its helper connection while helper-backed terminal
  sessions are active
- **THEN** shell state records helper disconnect diagnostics for affected
  terminal ContentInstances
- **AND** the helper cleans up sessions bound to that connection when possible

### Requirement: Background terminal execution remains real-time while rendering is priority-scoped
The macOS shell host SHALL keep terminal ContentInstance processes, PTYs,
terminal state, pending input delivery, and scrollback running in real time while
controlling surface focus, occlusion, refresh, and SwiftUI publication by
terminal runtime priority.

#### Scenario: Hidden terminal produces output
- **WHEN** a terminal ContentInstance is hidden by tab selection, space
  selection, split zoom, pane movement, or window occlusion
- **THEN** the terminal child process, PTY reads, terminal state, and scrollback
  continue running without requiring that terminal to become visible
- **AND** output remains available through the same terminal ContentInstance
  runtime when the terminal is reattached

#### Scenario: Background terminal receives text
- **WHEN** `terminal.send_text` targets a live background terminal
  ContentInstance
- **THEN** Alan delivers or durably queues the text according to the existing
  delivery contract without selecting the tab, space, or pane
- **AND** the terminal runtime priority does not cause a false success response

#### Scenario: Hidden terminal becomes visible
- **WHEN** a hidden terminal ContentInstance becomes visible
- **THEN** Alan runs a catch-up path that presents current terminal state from
  the existing runtime
- **AND** the terminal process, scrollback, cwd, title, terminal mode, and
  metadata are not reset solely because visibility changed

### Requirement: Terminal visibility transitions preserve runtime continuity
The macOS shell host SHALL treat visibility, focus, split zoom, tab selection,
space selection, and window occlusion changes as render scheduling inputs rather
than terminal lifecycle finalizers.

#### Scenario: Split zoom hides sibling terminals
- **WHEN** a user zooms one terminal pane and sibling terminal panes become
  hidden
- **THEN** sibling terminal ContentInstances remain registered in the terminal
  runtime service
- **AND** their processes and scrollback continue while their rendering priority
  changes to hidden background

#### Scenario: User switches spaces with live terminals
- **WHEN** a user switches from one shell space to another
- **THEN** terminal ContentInstances in the previous space keep their runtime
  identities and background execution
- **AND** terminal ContentInstances in the new space receive visible or
  foreground priority according to selection and focus

#### Scenario: Window becomes occluded
- **WHEN** the macOS window is occluded or hidden while terminal runtimes remain
  active
- **THEN** Alan marks affected terminal surfaces hidden for rendering
  coordination
- **AND** Alan does not close, recreate, or detach their terminal runtime
  identities solely because the window is not visible
