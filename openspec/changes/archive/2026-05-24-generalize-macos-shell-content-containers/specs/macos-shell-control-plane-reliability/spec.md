## MODIFIED Requirements

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

## ADDED Requirements

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
