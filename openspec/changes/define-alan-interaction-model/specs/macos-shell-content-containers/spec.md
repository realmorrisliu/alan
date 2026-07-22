## ADDED Requirements

### Requirement: Workspace home content declares home capabilities
alan SHALL 提供 `workspace_home` content kind 作为可序列化、可恢复的 ContentInstance
kind,呈现 `alan-interaction-model` 定义的 workspace home surface(active agents、
recent work、installed services)。Workspace home content 的 descriptor SHALL 暴露稳定
`content_id`、`workspace_home` content kind、用户可见标题和 home-specific capabilities,
并且 SHALL NOT 暴露 terminal-only capabilities。

#### Scenario: Workspace home content declares home capabilities
- **WHEN** control plane 或 UI 查询 workspace home content
- **THEN** response 包含 `content_id`、`workspace_home` content kind 和 home capabilities
- **AND** terminal input、terminal search、paste 等 terminal-only capabilities 不出现在该
  content descriptor 中

#### Scenario: App restores workspace home tab
- **WHEN** alan 重新打开之前包含 workspace home pane 的窗口
- **THEN** shell state 恢复同一 tab、split tree、PaneSlot IDs 和 `workspace_home`
  content kind
- **AND** workspace home content 从挂载的 Alan OS 文件状态渲染,不创建 terminal runtime
- **AND** workspace home content 不会被误恢复为 terminal 或其他 viewer surface
