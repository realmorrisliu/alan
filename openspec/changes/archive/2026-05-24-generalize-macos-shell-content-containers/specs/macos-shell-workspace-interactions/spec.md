## MODIFIED Requirements

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

## ADDED Requirements

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
