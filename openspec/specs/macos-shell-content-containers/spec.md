# macos-shell-content-containers Specification

## Purpose
Define the macOS shell content-container contract: Spaces, Tabs, and PaneSlots
own navigation and layout, while ContentInstances own terminal, markdown,
settings, and future content-specific lifecycle, capabilities, runtime identity,
and persistence.

## Requirements
### Requirement: Shell containers separate layout from content
alan 的 macOS shell SHALL 将 Space、Tab、PaneSlot 作为通用导航和布局容器，将 terminal、
markdown、settings 等具体内容作为 ContentInstance 挂载在 PaneSlot 上。新状态
MUST 使用 `contract_version = "0.2"`，并以 `pane_slots` 和 `contents` 分离表达布局与内容。

#### Scenario: Existing terminal tab is represented as terminal content
- **WHEN** alan 读取一个只包含 terminal 的现有 shell tab
- **THEN** 该 tab 以通用 pane layout tree 表达
- **AND** 每个 terminal leaf 以 PaneSlot 加 `terminal` ContentInstance 表达，而不是把 terminal 字段作为所有 pane 的默认形态
- **AND** pane layout leaf 引用 `pane_slot_id`，terminal runtime 引用 `content_id`

#### Scenario: Mixed split tab is represented
- **WHEN** 一个 tab 左侧包含 terminal content、右侧包含 markdown 或 settings content
- **THEN** 两个 leaf 共用同一个 tab 的 split layout tree
- **AND** 每个 leaf 保持独立 PaneSlot identity、ContentInstance identity、content kind、title、capabilities 和 lifecycle state

#### Scenario: Non-terminal pane receives focus
- **WHEN** 用户聚焦 markdown 或 settings pane
- **THEN** shell focus SHALL 指向该 pane slot
- **AND** alan MUST NOT 为该 pane 创建 terminal runtime，除非 content kind 是 `terminal`

### Requirement: PaneSlot and ContentInstance identities are distinct
alan 的 macOS shell SHALL 使用独立的 `pane_slot_id` 和 `content_id`。`pane_slot_id` 表示当前
layout/focus 位置，`content_id` 表示内容实例及其 runtime/persistence identity。

#### Scenario: Content moves with a slot
- **WHEN** 用户将承载 terminal、markdown 或 settings content 的 PaneSlot 移动到另一个 tab
- **THEN** 该 PaneSlot 保持同一个 `pane_slot_id`
- **AND** 挂载的 ContentInstance 保持同一个 `content_id`

#### Scenario: Content is replaced in a slot
- **WHEN** alan 在现有 PaneSlot 中用 markdown content 替换 terminal content
- **THEN** PaneSlot 保持同一个 `pane_slot_id`
- **AND** old terminal ContentInstance lifecycle 进入 closed/finalized
- **AND** new markdown ContentInstance 获得新的 `content_id`

#### Scenario: Future multi-view content is not implied
- **WHEN** v0.2 state 中一个 ContentInstance 被挂载
- **THEN** alan SHALL treat it as attached to one active PaneSlot unless a future capability explicitly introduces multi-view attachment

### Requirement: Content descriptors declare identity and capabilities
每个 shell ContentInstance SHALL 暴露稳定 `content_id`、content kind、用户可见标题、可选图标、
capabilities、持久化 payload、lifecycle state 和 renderer state。

#### Scenario: Terminal content declares terminal capabilities
- **WHEN** control plane 或 UI 查询 terminal content
- **THEN** response 包含 `content_id`、terminal content kind 和可用能力，例如 terminal input、terminal search、paste 和 runtime metadata

#### Scenario: Markdown content declares viewer capabilities
- **WHEN** alan 打开 markdown 文件 content
- **THEN** content descriptor 包含文件 URL 或授权引用、用户可见文件名和 read-only viewer capability
- **AND** terminal-only capabilities 不出现在该 content descriptor 中

#### Scenario: Settings content declares settings capabilities
- **WHEN** alan 打开 settings content
- **THEN** content descriptor 表达设置 surface identity 和设置页标题
- **AND** settings content 的可用命令通过 settings-specific capabilities 暴露

### Requirement: Content lifecycle is pane-slot aware
ContentInstance SHALL 在 PaneSlot 创建、移动、关闭和恢复时保持明确生命周期，并且不得让
content-specific cleanup 破坏通用 shell layout。

#### Scenario: Pane with non-terminal content closes
- **WHEN** 用户关闭承载 markdown 或 settings content 的 PaneSlot
- **THEN** alan 移除该 PaneSlot 并 finalize 对应 ContentInstance
- **AND** terminal runtime finalizer MUST NOT 被错误调用

#### Scenario: Pane with terminal content moves
- **WHEN** 用户将 terminal ContentInstance 所在的 PaneSlot 移动到另一个 tab
- **THEN** PaneSlot identity、ContentInstance identity 和 terminal runtime identity 保持连续
- **AND** ContentInstance descriptor 跟随该 PaneSlot 的新 tab/space membership

#### Scenario: Pane with markdown content moves
- **WHEN** 用户将 markdown ContentInstance 所在的 PaneSlot 移动到另一个 tab
- **THEN** markdown content 的文件引用、scroll/viewer state 和用户可见标题保持连续
- **AND** 目标 tab split tree 保持有效

### Requirement: Content state persists across app restore
alan SHALL 通过 current-schema workspace manifest 持久化通用 container state、PaneSlots 和
ContentInstances，使 app restore 后能恢复 tab、split 和 content kind，而不会把非 terminal
content 误恢复为 terminal。Content restore SHALL NOT decode or convert historical terminal-only
workspace manifests or persistent shell-state files.

#### Scenario: App restores mixed content tab
- **WHEN** alan 重新打开之前包含 terminal、markdown 和 settings pane 的窗口
- **THEN** shell state 恢复同一 tab、split tree、PaneSlot IDs 和每个 ContentInstance 的 kind
- **AND** terminal content 从 manifest 中的 terminal restore payload 创建新的 terminal runtime，而不是恢复上一轮 app 进程中的 OS process
- **AND** markdown/settings content 恢复为各自的 viewer/settings surface

#### Scenario: Historical persistence input does not materialize content
- **WHEN** restore encounters a terminal-only workspace manifest or persistent
  `shell-state-window_main.json`
- **THEN** alan does not decode or convert that input into PaneSlots or ContentInstances
- **AND** unsupported workspace-manifest bytes follow the current corrupt-evidence path

### Requirement: V1 non-terminal surfaces are bounded
V1 content-container implementation SHALL 支持 terminal、read-only markdown viewer、alan settings
surface；超出边界的编辑器、browser content、浏览器权限和扩展能力 SHALL 留给后续 change。

#### Scenario: Markdown file opens in shell tab
- **WHEN** 用户或 control client 请求打开 markdown 文件
- **THEN** alan 在当前 space 中创建或聚焦一个 markdown content tab/pane
- **AND** 该 surface 以 read-only viewer 呈现文件内容

#### Scenario: Settings opens in shell tab
- **WHEN** 用户打开 alan app 设置
- **THEN** alan 将设置页作为 shell tab content 呈现
- **AND** 设置页继承 shell sidebar、toolbar 和 tab selection 模型

#### Scenario: Browser content is deferred but not blocked
- **WHEN** 维护者检查 v0.2 content-container 模型
- **THEN** v1 SHALL NOT 要求 browser ContentInstance kind、browser renderer、WKWebView host、browser profile/cookie policy、download manager 或 browser permission model
- **AND** 模型 SHALL 保持通用 `ContentInstance`、content kind、payload、capability、renderer、lifecycle 和 PaneSlot 命名，使后续 browser change 可以新增 browser-specific descriptors，而不需要重命名 container contract
