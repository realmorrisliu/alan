# macos-shell-workspace-interactions Specification

## Purpose
Define alan's native macOS shell workspace interactions for terminal splits,
spatial focus, pane lift or cross-tab movement, and shared menu, keyboard, and
command UI routing.
## Requirements
### Requirement: Split layout stores durable ratios
alan's macOS shell SHALL store split branch direction, child PaneSlot identity, and
divider ratio in the shell model so split layouts survive rendering changes and
app state persistence.

#### Scenario: Existing equal split loads
- **WHEN** a tab with an older equal split tree is loaded
- **THEN** the shell model interprets each branch as equal ratios and preserves stable structural identity

#### Scenario: Divider is resized
- **WHEN** the user drags a split divider
- **THEN** the branch ratio updates within usable minimum bounds and terminal ContentInstances keep their runtime identities

#### Scenario: Window resizes
- **WHEN** the window size changes after ratios were set
- **THEN** pane frames are recalculated from stored ratios without resetting the split tree

### Requirement: Split operations are native and reversible
The macOS shell SHALL provide native split operations for creating directional
PaneSlots, closing PaneSlots, resizing panes, and equalizing panes.

#### Scenario: Create directional split
- **WHEN** the user invokes split right, left, up, or down from a menu, shortcut, command UI, or control command
- **THEN** alan inserts a new PaneSlot in the requested direction and focuses the intended PaneSlot according to the command semantics

#### Scenario: Equalize splits
- **WHEN** the user invokes equalize for a tab
- **THEN** all split branches in that tab return to equal usable ratios without restarting terminal runtimes

#### Scenario: Close focused pane
- **WHEN** the user invokes close pane while a tab has multiple panes
- **THEN** alan removes the focused PaneSlot, repairs the split tree, and keeps remaining terminal ContentInstance runtimes alive

### Requirement: Spatial focus is first class
The macOS shell SHALL allow users to move focus spatially between visible PaneSlots
using left, right, up, and down directions.

#### Scenario: Focus adjacent pane
- **WHEN** the user invokes focus right from a focused PaneSlot with a visible neighbor to the right
- **THEN** shell focus moves to that neighboring PaneSlot
- **AND** terminal focus follows only when the neighboring PaneSlot mounts terminal content

#### Scenario: Preserve perpendicular position
- **WHEN** a tab contains a two-by-two split layout and the lower-left PaneSlot is focused
- **THEN** invoking focus right selects the lower-right PaneSlot rather than the upper-right PaneSlot

#### Scenario: No adjacent pane
- **WHEN** a spatial focus command has no valid target in the requested direction
- **THEN** focus remains unchanged and the command reports a no-target result where a response is required

### Requirement: Pane lift and cross-tab moves preserve runtime identity
alan's macOS shell SHALL support PaneSlot lift and cross-tab PaneSlot move operations
that preserve PaneSlot identity, mounted ContentInstance identity, and any terminal
runtime handle, scrollback, metadata, and pending delivery state owned by terminal content.

#### Scenario: Lift pane to a new tab
- **WHEN** the user lifts a PaneSlot out of a split tab
- **THEN** alan creates a new tab for that PaneSlot and the mounted ContentInstance keeps the same identity
- **AND** terminal runtime identity remains continuous when the mounted content is terminal

#### Scenario: Move pane to another tab in the same window
- **WHEN** the user moves a PaneSlot to another tab in the same shell window
- **THEN** the PaneSlot and mounted ContentInstance keep their identities and the source and target tab split trees remain valid

#### Scenario: Move would empty a tab
- **WHEN** a PaneSlot move would leave a tab without panes
- **THEN** alan either closes the empty tab through normal tab-close semantics or rejects the move with a stable reason

### Requirement: Split zoom is reversible
The macOS shell SHALL let users zoom and unzoom the focused PaneSlot without
mutating the canonical split tree or closing sibling PaneSlots or terminal content.

#### Scenario: Zoom focused pane
- **WHEN** the user zooms a focused split PaneSlot
- **THEN** the focused PaneSlot fills the shell content area and sibling PaneSlots remain alive and restorable

#### Scenario: Unzoom focused pane
- **WHEN** the user exits zoom
- **THEN** the previous split layout, divider ratios, PaneSlot identities, and mounted ContentInstance identities are restored

### Requirement: In-tab pane movement is explicit and reversible
The macOS shell SHALL support explicit PaneSlot movement within the same tab while
preserving PaneSlot identity, mounted ContentInstance identity, and split tree validity.

#### Scenario: Move pane within current tab
- **WHEN** the user moves a PaneSlot to a valid position in the current tab
- **THEN** alan updates the split-tree placement and keeps the moved PaneSlot and mounted ContentInstance identities

#### Scenario: Move target invalid
- **WHEN** the requested in-tab move would create an invalid split tree or move a PaneSlot onto itself
- **THEN** alan rejects the move with a stable reason and leaves the current layout unchanged

### Requirement: Drag/drop movement has a terminal-selection quality gate
Pane drag/drop SHALL only be enabled by default after it uses the same controller
mutation path as explicit moves and preserves terminal text selection behavior.

#### Scenario: Drag starts inside terminal text
- **WHEN** the user drags inside terminal-rendered text
- **THEN** alan treats the drag as terminal selection or terminal input rather than pane movement

#### Scenario: Drag uses a movement affordance
- **WHEN** the user drags a supported PaneSlot movement affordance to another valid target
- **THEN** alan runs the same move mutation used by explicit move commands

### Requirement: Copy paste and search commands route consistently
Copy, paste, and terminal search SHALL resolve the same focused terminal target
across native menus, keyboard shortcuts, command UI, and terminal host surfaces.

#### Scenario: Copy focused terminal selection
- **WHEN** the user invokes Copy while focused terminal content owns a selection
- **THEN** the command is delivered to that terminal host rather than to shell debug text

#### Scenario: Paste into focused terminal
- **WHEN** the user invokes Paste while a PaneSlot that mounts terminal content is focused
- **THEN** the command is delivered to that terminal ContentInstance host

#### Scenario: Search focused terminal
- **WHEN** the user invokes terminal search from a native command surface
- **THEN** the search UI is scoped to the focused terminal ContentInstance

### Requirement: Commands use native Mac surfaces
Workspace actions SHALL be available through native menu/command routing,
keyboard shortcuts, command input, and any restrained toolbar affordances that
call the same shell controller mutations where the action is shared. Menu bar,
context menu, and keyboard shortcut paths SHALL resolve shared shell actions
through the macOS shell action registry. The default `Command-P` command input
SHALL accept typed commands without showing persistent candidate action lists;
this registry change SHALL NOT add new Command UI behaviors.

#### Scenario: Menu command
- **WHEN** the user selects New Terminal Tab, New alan Tab, Split, Focus Pane,
  Equalize Splits, Close Pane, or Close Tab from the menu bar
- **THEN** alan executes the registered shell action used by matching keyboard
  and context paths where that behavior is shared

#### Scenario: Keyboard command
- **WHEN** the user invokes a supported command-key shortcut
- **THEN** the responder chain routes it to alan's shell action registry or
  terminal surface command handler as appropriate

#### Scenario: Context command
- **WHEN** the user invokes a supported Tab or Space context menu command
- **THEN** alan resolves the registry action with the context Tab or Space
  target rather than first changing shell selection

#### Scenario: Command input opens
- **WHEN** the user opens `Go to or Command...`
- **THEN** alan focuses a single command input field instead of presenting
  default action, routing, or attention candidate lists
- **AND** this registry change does not add new Tab or Space organization
  commands to the Command UI

#### Scenario: Command input shortcut toggles
- **WHEN** the user presses `Command-P` while the command input is focused or
  visible
- **THEN** alan dismisses the command input instead of opening a duplicate
  surface

#### Scenario: Typed command resolves
- **WHEN** the user submits a typed command that alan can resolve to an existing
  workspace action or routing target
- **THEN** alan executes the existing command input behavior and dismisses the
  command input

#### Scenario: Typed command is unresolved
- **WHEN** the user submits a typed command that alan cannot resolve
- **THEN** alan leaves the command input open and communicates the unresolved
  state without exposing raw pane IDs or debug routing details

### Requirement: Sidebar swipe previews spaces without moving the workspace
Horizontal swipe gestures that originate inside the macOS sidebar SHALL drive
a sidebar-local, finger-tracked space content pager. The moving page SHALL
include only the active-space tab content; the top Space slider, titlebar
controls, sidebar material surface, sidebar chrome, macOS traffic-light
placement, and workspace terminal surface SHALL remain visually fixed while the
gesture is active. alan SHALL avoid mutating durable shell selection until the
gesture commits. The pager SHALL keep the current space centered in a bounded
five-page rendering window: up to two previous spaces, the current space, and
up to two next spaces.

#### Scenario: Gesture-tracked sidebar content pager
- **WHEN** a user horizontally swipes inside the sidebar and an adjacent space exists
- **THEN** the current sidebar tab content moves with the gesture while the adjacent space content previews from the side
- **AND** the tab content uses the full sidebar content page width for horizontal offsets
- **AND** movement is rendered directly from horizontal finger translation instead of being amplified, quantized, or shaped by the commit threshold
- **AND** the pager keeps previous, current, and next page slots stable while direction changes instead of replacing the rendered target page based only on current drag sign
- **AND** visual drag is clamped to one page plus a small overdrag gap that can reveal part of the second adjacent page for physical feedback
- **AND** the sidebar pager avoids static left or right padding gaps while pages move
- **AND** the top Space slider remains fixed as the stable space navigation control
- **AND** the workspace terminal surface remains visually stable on the original selected space
- **AND** visible terminal panes keep their runtime identities instead of being restarted, duplicated, or horizontally offset as a side effect of the drag
- **AND** vertical tab-list scrolling does not move while horizontal intent is locked
- **AND** later vertical finger movement during the same horizontal swipe does not move the tab list vertically

#### Scenario: Undecided axis buffers mixed deltas
- **WHEN** a sidebar scroll gesture has not yet crossed the horizontal or vertical intent threshold
- **THEN** alan buffers the initial mixed deltas instead of applying partial vertical tab-list scrolling or horizontal pager movement
- **AND** the gesture is routed only after horizontal or vertical intent is locked

#### Scenario: Content pager reaches sequence edge
- **WHEN** a user swipes past the first or last available space
- **THEN** alan applies bounded edge resistance to the moving sidebar content rather than wrapping unexpectedly or showing a nonexistent space page
- **AND** releasing before a valid target is selected returns the content pager to the current space

#### Scenario: Commit updates focus at the authoritative transition point
- **WHEN** the user releases a space swipe past the commit threshold or with sufficient release velocity toward an adjacent space
- **THEN** alan commits the target space through the shell controller selection and focus path
- **AND** the sidebar content pager settles smoothly to the committed space without being reverted by concurrent runtime updates
- **AND** the workspace terminal surface and terminal focus follow the committed space after shell selection commits
- **AND** release is honored even when the macOS ended or momentum-start event carries zero scroll delta
- **AND** a single release commits at most the immediately adjacent previous or next space, never multiple spaces

#### Scenario: Cancel preserves focus and layout
- **WHEN** the user releases a space swipe before the commit threshold
- **THEN** alan animates the sidebar content pager back to the original space
- **AND** selected space, selected tab, terminal focus, split tree, and divider ratios remain unchanged
- **AND** release is honored even when the macOS ended or momentum-start event carries zero scroll delta

#### Scenario: Phaseful gesture waits for real release
- **WHEN** a user pauses a phaseful horizontal trackpad swipe while their fingers remain on the trackpad
- **THEN** alan keeps the sidebar content pager at the current drag offset
- **AND** alan does not commit or cancel until the gesture ends, is cancelled, or enters momentum

#### Scenario: Release uses last effective velocity
- **WHEN** a phaseful horizontal trackpad swipe ends or enters momentum
- **THEN** alan evaluates commit using current pager progress and the last effective finger velocity before release
- **AND** alan does not replace that velocity with a zero-delta ended event

#### Scenario: Fast flick can commit
- **WHEN** a user performs a fast horizontal flick inside the sidebar
- **THEN** alan recognizes the dominant horizontal release or momentum handoff as a space switch
- **AND** alan may commit from velocity even when the gesture produced only a short visible translation before release

#### Scenario: Phase-less gesture settles
- **WHEN** a horizontal sidebar swipe comes from a scroll device that does not provide gesture phases
- **THEN** alan may treat a short idle gap as release to avoid leaving the content pager stuck
- **AND** shell selection follows the same commit threshold as other sidebar swipes

#### Scenario: Vertical scroll is not captured
- **WHEN** a user's gesture is primarily vertical in the sidebar tab list
- **THEN** the native vertical tab-list scroll receives the gesture and the workspace space transition does not begin
- **AND** horizontal sidebar content pager movement is not applied while vertical intent is locked

### Requirement: Sidebar split indicators can focus panes
Split topology indicators in the macOS sidebar SHALL route PaneSlot focus through
the same shell controller focus model used by split interactions.

#### Scenario: Two-pane segment clicked
- **WHEN** a user clicks a segment in a two-pane tab row split indicator
- **THEN** alan selects that PaneSlot
- **AND** terminal focus follows only if the selected PaneSlot mounts terminal content
- **AND** the action does not change the split tree or divider ratios

#### Scenario: Complex split indicator clicked
- **WHEN** a user clicks a compact indicator for a tab with three or more panes
- **THEN** alan performs a predictable PaneSlot-focus action or opens a compact pane picker, and the action does not mutate the split tree

#### Scenario: Split indicator keyboard access
- **WHEN** a split tab row or its split indicator has keyboard focus
- **THEN** keyboard or accessibility activation can focus PaneSlots without relying on pointer-only interaction

### Requirement: Sidebar selection commits authoritative shell focus
Sidebar tab and space selection SHALL update the authoritative shell focused
pane through the same shell controller focus model used by terminal activation,
so sidebar selection, focused space, focused tab, focused pane, and terminal
runtime focus converge.

#### Scenario: Tab row clicked
- **WHEN** a user clicks a tab row in the active sidebar space
- **THEN** alan resolves the target tab's preferred pane and updates shell focus to that pane through the shell controller focus path
- **AND** later terminal runtime metadata, state publication, or selection synchronization does not restore the previously focused tab

#### Scenario: Space slider target clicked
- **WHEN** a user clicks a Space target in the top sidebar Space slider
- **THEN** alan selects that space, resolves the target tab and pane for that space, and updates shell focus through the shell controller focus path
- **AND** terminal focus follows the selected pane when the pane runtime is available

#### Scenario: Selected tab contains multiple panes
- **WHEN** a sidebar selection targets a tab with multiple panes
- **THEN** alan prefers the tab's currently focused pane when that pane belongs to the selected tab
- **AND** alan otherwise chooses a stable pane from the tab's pane tree without changing split structure or divider ratios

#### Scenario: Runtime update races selection
- **WHEN** terminal runtime metadata or control-plane state publication occurs immediately after sidebar selection
- **THEN** the committed sidebar selection remains on the selected tab and space because the shell focused pane already matches the selection

### Requirement: New terminal tabs inherit focused pane cwd
The macOS shell SHALL create user-requested terminal tabs in the focused pane's
current working directory unless the caller supplies an explicit working
directory or no valid focused-pane cwd exists.

#### Scenario: Runtime cwd is current
- **WHEN** the user invokes New Terminal Tab from a focused pane whose runtime metadata reports cwd `/repo/app`
- **THEN** the new tab's initial pane starts in `/repo/app`
- **AND** the new tab's pane snapshot records `/repo/app` as its cwd

#### Scenario: Snapshot cwd is fallback
- **WHEN** the focused pane has no runtime cwd metadata but its shell pane snapshot records cwd `/repo/app`
- **THEN** the new terminal tab starts in `/repo/app`

#### Scenario: Explicit cwd wins
- **WHEN** a control-plane or command path opens a new terminal tab with an explicit cwd `/tmp/work`
- **THEN** alan starts the new tab in `/tmp/work` even if the focused pane has a different cwd

#### Scenario: Missing focused cwd falls back
- **WHEN** a new terminal tab is requested and alan cannot resolve a valid cwd from the focused pane or request
- **THEN** alan falls back to the workspace default working directory or the user's home directory

#### Scenario: Split and tab creation agree
- **WHEN** a user creates a split pane and a new terminal tab from the same focused pane
- **THEN** both new terminal runtimes use the same cwd resolution order

### Requirement: Keyboard Shell Commands Route Through The Action Registry
Keyboard-triggered macOS shell commands SHALL resolve and execute through the
shell action registry so keyboard shortcuts, menus, and context menus share
action availability and handler semantics.

#### Scenario: Keyboard shortcut invokes Tab action
- **WHEN** the user presses a Tab-related shell shortcut
- **THEN** alan resolves the registered Tab action and applies it to the current
  selected Tab

#### Scenario: Keyboard shortcut invokes Space action
- **WHEN** the user presses a Space-related shell shortcut
- **THEN** alan resolves the registered Space action and applies it to the
  current selected Space context

#### Scenario: Keyboard shortcut invokes pane action
- **WHEN** the user presses a pane-related shell shortcut
- **THEN** alan resolves the registered pane action and applies it to the
  focused pane

### Requirement: First-Version Space Shortcuts Are Navigation Only
The first version of macOS shell Space shortcuts SHALL cover Space navigation
only and SHALL NOT provide default shortcuts for Space creation, rename, or
deletion.

#### Scenario: Next Space shortcut
- **WHEN** the user presses the default Next Space shortcut
- **THEN** alan selects the next Space in workspace order

#### Scenario: Previous Space shortcut
- **WHEN** the user presses the default Previous Space shortcut
- **THEN** alan selects the previous Space in workspace order

#### Scenario: Numeric Space shortcut
- **WHEN** the user presses a numeric Space selection shortcut for an existing
  Space index
- **THEN** alan selects that Space

#### Scenario: Numeric Space target is missing
- **WHEN** the user presses a numeric Space selection shortcut for a missing
  Space index
- **THEN** alan leaves the current Space selected and reports a stable
  unavailable reason for diagnostics where appropriate

#### Scenario: Create Space has no default shortcut
- **WHEN** the first-version Space action registry exposes create Space
- **THEN** alan exposes the action without a default keyboard shortcut

#### Scenario: Rename or delete Space has no default shortcut
- **WHEN** the first-version Space action registry exposes rename or delete Space
- **THEN** alan exposes those actions without default keyboard shortcuts

### Requirement: Tabs Are Organized Into Per-Space Pinned And Unpinned Sections
The macOS shell SHALL organize Tabs inside each Space into a Pinned section and
an Unpinned section. Pinning is scoped to the owning Space and SHALL NOT create
a global pinned Tab shelf.

#### Scenario: Space contains pinned and unpinned Tabs
- **WHEN** a Space has both Pinned and Unpinned Tabs
- **THEN** alan presents those Tabs as two ordered sections within that Space

#### Scenario: Pinned Tab moves to another Space
- **WHEN** a Pinned Tab is moved to a different Space
- **THEN** alan keeps the Tab pinned and inserts it at the end of the target
  Space's Pinned section

#### Scenario: Unpinned Tab moves to another Space
- **WHEN** an Unpinned Tab is moved to a different Space
- **THEN** alan keeps the Tab unpinned and inserts it at the end of the target
  Space's Unpinned section

### Requirement: Tab Rows Support Direct Reorder And Pin State Changes
The macOS shell SHALL allow users to drag Tab rows to reorder Tabs within a
section and to change pin state by dragging across the Pinned and Unpinned
section boundary.

#### Scenario: Short click selects Tab
- **WHEN** the user clicks a Tab row without crossing the drag threshold
- **THEN** alan selects that Tab normally

#### Scenario: Drag reorders inside a section
- **WHEN** the user drags a Tab row to another position inside the same section
- **THEN** alan reorders the Tab within that section without changing its pin
  state

#### Scenario: Drag pins Tab
- **WHEN** the user drags an Unpinned Tab into the Pinned section and drops it
- **THEN** alan pins the Tab using its current restorable state and inserts it
  at the previewed Pinned position

#### Scenario: Drag unpins Tab
- **WHEN** the user drags a Pinned Tab into the Unpinned section and drops it
- **THEN** alan unpins the Tab and inserts it at the previewed Unpinned position

#### Scenario: Drag shows insertion preview
- **WHEN** the user drags a Tab row within or across sections
- **THEN** alan shows a realtime insertion preview before mutating durable Tab
  order

### Requirement: Move Tab To Space Is Explicit In The First Version
The macOS shell SHALL support Move Tab to Space through menu and Tab context
actions. The first version SHALL NOT require dragging a Tab to the Space
switcher to move it across Spaces.

#### Scenario: Move selected Tab follows target
- **WHEN** the user moves the current selected Tab to another Space
- **THEN** alan selects the target Space and keeps the moved Tab selected

#### Scenario: Move non-selected Tab stays put
- **WHEN** the user moves a non-selected Tab to another Space through its
  context menu
- **THEN** alan keeps the current Space, selected Tab, and focused pane
  unchanged

#### Scenario: Move target missing
- **WHEN** the user or a control path requests moving a Tab to a missing Space
- **THEN** alan rejects the move with a stable reason and leaves Tab order,
  Space ownership, and focus unchanged

### Requirement: Tab Context Menus Use Context Targets
Tab context menu actions SHALL target the Tab that opened the menu without first
changing selected Space, selected Tab, or focused pane.

#### Scenario: Context pin targets clicked Tab
- **WHEN** the user opens a context menu on a non-selected Tab and chooses Pin
- **THEN** alan pins the clicked Tab without selecting it first

#### Scenario: Context move targets clicked Tab
- **WHEN** the user opens a context menu on a non-selected Tab and chooses Move
  Tab to Space
- **THEN** alan moves the clicked Tab and keeps the current selection unchanged
  unless the clicked Tab was already selected

### Requirement: Quick Terminal Summon And Dismiss Are Shell Commands
Quick terminal summon, dismiss, focus, and close operations SHALL route through
Alan's shared shell command/controller paths so keyboard shortcuts, menu
commands, command input, and control surfaces converge on the same behavior.
Alan SHALL expose a configurable global toggle shortcut for quick terminal; the
draft default shortcut is `Option+Space`.

#### Scenario: Quick terminal command opens
- **WHEN** the user invokes quick terminal from a keyboard shortcut, menu,
  command input, or supported control command
- **THEN** Alan summons the same quick terminal target through the shared shell
  controller path and focuses terminal input

#### Scenario: Quick terminal global shortcut toggles
- **WHEN** the quick terminal is visible and the user invokes the quick terminal
  toggle command again
- **THEN** Alan hides the quick terminal presentation without closing the
  underlying terminal runtime

#### Scenario: Quick terminal does not use Escape as hide
- **WHEN** the quick terminal owns focus and the user presses `Esc`
- **THEN** Alan treats the key as terminal input unless an Alan-owned nested
  quick-terminal menu or picker is currently open

#### Scenario: Quick terminal close is explicit
- **WHEN** the user invokes close while the quick terminal owns focus
- **THEN** Alan distinguishes hiding the quick terminal presentation from
  closing the underlying terminal session

### Requirement: Pane layout operations are content-agnostic
The macOS shell SHALL treat split, focus, resize, equalize, pane lift, cross-tab move, and close pane as PaneSlot operations over the split layout tree, not as terminal-only operations.

#### Scenario: Split terminal pane with markdown target
- **WHEN** 用户在 terminal pane 旁创建 markdown split
- **THEN** alan 在同一个 tab 中插入新的 pane slot
- **AND** 新 PaneSlot 承载 markdown ContentInstance
- **AND** 原 terminal ContentInstance 的 runtime identity 保持连续

#### Scenario: Focus moves from terminal to settings pane
- **WHEN** 用户从 terminal pane 空间聚焦到同一 tab 内的 settings pane
- **THEN** shell focus 更新到 settings PaneSlot
- **AND** terminal runtime 保持后台存活，不接收 settings pane 的键盘输入

#### Scenario: Move mixed pane between tabs
- **WHEN** 用户将 markdown 或 settings pane 移动到另一个 tab
- **THEN** alan 保持该 PaneSlot 和 ContentInstance identity 连续
- **AND** source 和 target tab 的 split tree 都保持有效

### Requirement: Tab creation accepts content intent
创建 tab 或 split pane 时，macOS shell SHALL 接受 content intent，并在 intent 缺省时保持现有
terminal tab 行为。

#### Scenario: New terminal tab remains default
- **WHEN** 用户执行现有 New Terminal Tab 行为
- **THEN** alan 创建承载 `terminal` ContentInstance 的 tab
- **AND** 现有 keyboard/menu/command 行为保持兼容

#### Scenario: New settings tab opens
- **WHEN** 用户执行 Open Settings in Tab 行为
- **THEN** alan 在当前 space 创建或聚焦承载 canonical `settings` ContentInstance 的 tab
- **AND** sidebar tab row 使用用户可见设置标题，而不是 raw content ID

#### Scenario: New markdown tab opens
- **WHEN** 用户请求打开 markdown 文件为 tab
- **THEN** alan 创建承载 `markdown` ContentInstance 的 tab
- **AND** tab title 从文件名或 content title 派生

#### Scenario: Settings tab is singleton
- **WHEN** 用户再次执行 Open Settings in Tab 行为
- **THEN** alan 聚焦已存在的 settings ContentInstance 所在 PaneSlot
- **AND** alan MUST NOT 创建重复 settings tabs，除非未来 capability 明确引入多实例 settings

### Requirement: Sidebar and command routing understand content kind
Sidebar、toolbar、command input 和 menu routing SHALL 使用 content kind、title 和 capabilities
来展示和执行 tab/pane 操作，而不是把所有 PaneSlots 视为 terminal target。

#### Scenario: Sidebar lists mixed content tabs
- **WHEN** 一个 space 中存在 terminal、markdown 和 settings tabs
- **THEN** sidebar 使用各自用户可见标题和 restrained content affordance
- **AND** 默认 UI 不暴露 raw pane IDs、content IDs 或 renderer implementation names

#### Scenario: Command input resolves content-aware target
- **WHEN** 用户通过 command input 跳转到 markdown 或 settings pane
- **THEN** alan 聚焦对应 PaneSlot
- **AND** 不执行 terminal-specific focus side effect，例如请求 terminal host first responder

### Requirement: Spaces Own Default Terminal Profiles
The macOS shell SHALL allow each Space to reference a default Terminal Profile
used for new terminal content created in that Space.

#### Scenario: New Space with profile
- **WHEN** the user creates a Space and selects Terminal Profile `alan`
- **THEN** alan binds the new Space to `terminal_profile_id` `alan`
- **AND** the Space's first terminal content is created with `terminal_profile_id`
  `alan`

#### Scenario: New tab inherits Space profile
- **WHEN** the selected Space is bound to Terminal Profile `univer`
- **AND** the user creates a new terminal tab without an explicit profile
- **THEN** alan creates the new terminal content with `terminal_profile_id`
  `univer`

#### Scenario: New tab explicit profile override
- **WHEN** the selected Space is bound to Terminal Profile `alan`
- **AND** the user creates a new terminal tab explicitly using Terminal Profile
  `root`
- **THEN** alan creates the new terminal content with `terminal_profile_id`
  `root`

### Requirement: Splits Inherit Current Pane Terminal Profile
The macOS shell SHALL create split terminal content using the current pane's
Terminal Profile by default, so split workflows remain within the same Unix
identity unless the user explicitly overrides them.

#### Scenario: Split inherits current pane profile
- **WHEN** the focused terminal pane was created with Terminal Profile `alan`
- **AND** the user creates a split without an explicit profile
- **THEN** alan creates the new split terminal content with
  `terminal_profile_id` `alan`

#### Scenario: Split falls back to Space profile
- **WHEN** the focused pane has no terminal profile reference
- **AND** the selected Space is bound to Terminal Profile `univer`
- **AND** the user creates a split without an explicit profile
- **THEN** alan creates the new split terminal content with
  `terminal_profile_id` `univer`

#### Scenario: Split explicit profile override
- **WHEN** the focused pane was created with Terminal Profile `alan`
- **AND** the user creates a split explicitly using Terminal Profile `root`
- **THEN** alan creates the new split terminal content with
  `terminal_profile_id` `root`

### Requirement: Space Profile Binding Is Not Retroactive
The macOS shell SHALL treat a Space Terminal Profile binding as a default for
future terminal creation, not as a command to migrate existing terminal content.

#### Scenario: Space binding changes
- **WHEN** a Space changes its Terminal Profile binding from `alan` to `univer`
- **THEN** existing terminal content in that Space keeps its stored
  `terminal_profile_id`
- **AND** new terminal content created after the change uses `univer` by default
