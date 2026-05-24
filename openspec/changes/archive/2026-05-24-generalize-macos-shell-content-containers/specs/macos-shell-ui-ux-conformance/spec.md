## ADDED Requirements

### Requirement: Non-terminal content stays inside shell workspace chrome
Markdown、settings 和未来 content surface SHALL 继承 alan macOS shell 的 sidebar、
toolbar、tab selection、split layout 和 restrained material 视觉系统，而不是引入第二套 page
chrome、dashboard 布局或营销式页面结构。

#### Scenario: Markdown tab is active
- **WHEN** 用户选择 markdown content tab
- **THEN** 主区域显示 markdown viewer
- **AND** sidebar、toolbar 和 tab row 仍保持默认 shell chrome
- **AND** UI 不显示 terminal-specific debug labels 或 raw content IDs

#### Scenario: Settings tab is active
- **WHEN** 用户选择 alan settings content tab
- **THEN** 设置内容呈现在 shell content area 中
- **AND** 默认 UI 不增加 page-like hero、card-heavy dashboard 或独立 settings navigation shell

### Requirement: Content labels are user-facing
默认 UI SHALL 使用 content title、file name、settings section 或未来 content title 等用户可见信息
展示 tab/pane，不得把 implementation IDs 作为主要标签。

#### Scenario: Mixed split pane title bars render
- **WHEN** 一个 split tab 同时显示 terminal、markdown 和 settings panes
- **THEN** 每个 pane title bar 显示对应用户可见标题和必要的 compact status
- **AND** terminal-only 状态只出现在 terminal pane 上

#### Scenario: Command results include non-terminal content
- **WHEN** command input 列出 markdown 或 settings target
- **THEN** 结果使用用户可见 content title 和 type hint
- **AND** 不以 raw pane ID、content ID 或 renderer class name 作为 primary label
