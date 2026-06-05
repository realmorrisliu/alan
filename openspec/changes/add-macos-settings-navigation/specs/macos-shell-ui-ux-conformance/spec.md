## MODIFIED Requirements

### Requirement: Non-terminal content stays inside shell workspace chrome
Markdown、settings 和未来 content surface SHALL 继承 alan macOS shell 的 sidebar、
toolbar、tab selection、split layout 和 restrained material 视觉系统，而不是引入第二套 page
chrome、dashboard 布局、营销式页面结构或独立于 shell content area 的 settings navigation shell。

#### Scenario: Markdown tab is active
- **WHEN** 用户选择 markdown content tab
- **THEN** 主区域显示 markdown viewer
- **AND** sidebar、toolbar 和 tab row 仍保持默认 shell chrome
- **AND** UI 不显示 terminal-specific debug labels 或 raw content IDs

#### Scenario: Settings tab is active
- **WHEN** 用户选择 alan settings content tab
- **THEN** 设置内容呈现在 shell content area 中
- **AND** Settings 可在该 content area 内使用轻量的内部分组导航来组织设置内容
- **AND** 默认 UI 不增加 page-like hero、card-heavy dashboard、独立设置窗口或脱离 shell content area 的第二套 settings navigation shell

## ADDED Requirements

### Requirement: Settings Uses Internal Task Navigation
Alan macOS Settings SHALL use a compact internal navigation to separate settings
into task-oriented groups and SHALL render only the selected group in the main
Settings content area.

The default Settings navigation order SHALL be:

- General
- Terminal
- Agent
- System

#### Scenario: Settings opens on General
- **WHEN** the user opens the Settings content tab
- **THEN** alan shows the internal Settings navigation inside the Settings content area
- **AND** General is selected by default
- **AND** the main Settings content area shows General rows without showing every other settings group in one continuous scroll

#### Scenario: Settings group selection changes content
- **WHEN** the user selects a Settings navigation group
- **THEN** alan updates the main Settings content area to show that group's rows
- **AND** the outer shell sidebar, tab selection, split layout, and toolbar remain unchanged

#### Scenario: Settings group mapping stays task oriented
- **WHEN** Settings builds its navigation groups from the settings surface snapshot
- **THEN** General contains Interface preferences
- **AND** Terminal contains Terminal Profiles, Managed Terminal Account, Mac login session, and sudo behavior rows
- **AND** Agent contains the Alan agent selector, provider connection, model, credential, account action, runtime default, skill status, skill package source, and command line tool rows
- **AND** System contains app identity, install channel, daemon endpoint, updates, Alan home, shell state, shell control, and diagnostics rows

#### Scenario: Terminal identity stays separate from Agent configuration
- **WHEN** Settings renders Terminal and Agent groups
- **THEN** Terminal Profiles and Managed Terminal Accounts appear in Terminal
- **AND** provider connection profile, provider, model, credential, account action, runtime default, and skill rows appear in Agent
- **AND** alan does not label local terminal identity as an agent account or provider account

#### Scenario: Agent selector is scoped to supported agents
- **WHEN** the user opens the Agent Settings group
- **THEN** alan shows Alan as the currently configurable agent
- **AND** alan does not show Codex as a disabled option or coming-soon panel until Codex settings are supported

#### Scenario: Skill package source copy is explicit
- **WHEN** Settings renders the Agent skill source row
- **THEN** alan labels the filesystem package source as Skill Packages
- **AND** alan does not label that path as Public skills

#### Scenario: Navigation stays visually subordinate
- **WHEN** Settings renders the internal navigation
- **THEN** the navigation uses compact native-feeling rows, restrained typography, SF Symbol icons, and a subtle selected state
- **AND** alan does not present the Settings navigation as a page-like dashboard, large tab bar, marketing panel, or second app-level sidebar
