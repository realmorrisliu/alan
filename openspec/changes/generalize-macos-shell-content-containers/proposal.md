## Why

alan 的 macOS shell 现在把 Tab、Pane、Split 和 Terminal runtime 绑定在一起：
用户看到的是通用工作区结构，但模型和渲染路径事实上只支持 terminal 这一种内容。随着
markdown 文件、alan 设置页、浏览器内核和其他原生工具面板进入同一个工作区，Tab 和
Pane 需要成为通用内容容器，而 Terminal 需要降级为一种 Content surface。

这个 change 采用一步到位的真分离模型，而不是在现有 `ShellPane` 上追加
`content` 字段后继续保留 terminal 历史语义。

## Sequencing

本 change 以 `persist-macos-shell-workspaces` 先完成并归档为前提。完成顺序很重要：
workspace restore authority 先由 `ShellWorkspaceManifest` 接管；随后本 change 将该 manifest
的 terminal-only restore snapshot 升级为 content-container restore snapshot。实现时不得重新把
`shell-state-window_main.json` 或 `ShellStateSnapshot` 作为 workspace restore authority。

## What Changes

- **BREAKING**: 将 macOS shell state 升级为 `contract_version = "0.2"`，新状态以
  `pane_slots` 和 `contents` 分离表达布局与内容；`persist-macos-shell-workspaces` 产生的
  terminal-only manifest snapshot 和旧 `panes: [ShellPane]` 只作为迁移输入。
- 引入通用 content-container 合约：Space / Tab / PaneSlot 负责导航、布局、焦点、移动和关闭；
  ContentInstance 负责具体内容类型、生命周期、标题、图标、命令能力、渲染和安全边界。
- 允许一个 split tab 内混合不同 content，例如左侧 terminal、右侧 markdown 或 settings。
- 将 terminal runtime identity 从 pane-keyed 迁移为 content-keyed：`terminal` content 是
  runtime owner，PaneSlot 只是当前承载位置。
- 为首批非 terminal content 定义 v1 范围：markdown viewer 和 alan settings；browser 作为
  后续 change 单独设计。
- v0.2 采用 `ContentInstance`、content kind、capability 和 payload 这组通用命名；
  v1 不新增 browser kind，但后续 browser change 可以在同一模型中添加 browser
  descriptor、capability、payload 和 renderer，而不需要重命名 PaneSlot / ContentInstance。
- 区分通用 pane/control-plane 命令和 content-specific 命令：split、focus、move、close 是
  PaneSlot 命令；terminal input 使用 terminal-specific command，并先解析到 `terminal`
  ContentInstance。
- 更新 UI 合约，要求非 terminal content 仍然出现在 Arc-like shell 工作区内，不能退化成
  dashboard/page chrome 或第二套导航模型。

## Capabilities

### New Capabilities

- `macos-shell-content-containers`: 定义 macOS shell 的通用内容容器、PaneSlot /
  ContentInstance 分离、content kind、content 生命周期、能力声明和首批非 terminal
  surface 行为。

### Modified Capabilities

- `macos-shell-workspace-interactions`: 将 split、focus、pane lift、cross-tab move 等交互从
  terminal-only pane 扩展为 content-agnostic PaneSlot 操作。
- `macos-shell-terminal-lifecycle`: 明确 terminal lifecycle 只适用于 terminal ContentInstance，
  并将 terminal runtime 连续性绑定到 `content_id`。
- `macos-shell-control-plane-reliability`: 区分通用 pane mutation、content creation 和
  content-specific runtime commands 的结果语义，并更新 text delivery command 的目标模型。
- `macos-shell-ui-ux-conformance`: 约束 markdown、settings 等非 terminal content
  在默认 shell 中的视觉层级和 progressive disclosure。
- `macos-shell-workspace-persistence`: 将 workspace manifest 的 pin/live restore snapshot
  从 terminal-only pane shape 升级为 content-aware PaneSlot / ContentInstance restore shape。
- `macos-terminal-runtime-foundation`: 将 terminal runtime service、surface handle、metadata
  projection 和 text delivery 从 pane-keyed 迁移为 terminal ContentInstance keyed。
- `macos-shell-build-test-contract`: 增加通用 content-container 模型、渲染注册和混合 split
  场景的验证要求。

## Impact

- Affected Swift model code: `ShellTab`, new `ShellPaneSlot`, new `ShellContentInstance`,
  `ShellPaneTreeNode`, `ShellStateSnapshot`, state mutation helpers, persistence migration,
  and sidebar projections.
- Affected Swift rendering code: `ShellWorkspaceView`, `TerminalPaneView`, split layout leaves,
  terminal host attachment, and markdown/settings host views.
- Affected runtime/control plane code: shell DTOs, local/socket command handling, event projection,
  command result codes, terminal-only command validation, and legacy v0.1 state migration.
- Affected tests/scripts: shell model mutation tests, fake terminal runtime service tests,
  terminal runtime continuity checks, control-plane contract tests, and screenshot/manual
  verification for mixed content tabs.
