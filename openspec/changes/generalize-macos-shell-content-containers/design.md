## Context

当前 macOS shell 的用户模型已经是 Space / Tab / Split / Pane，但实现模型仍然把
Pane 视为 terminal runtime 的承载点。`ShellTab.kind` 已有 `terminal`、`scratch`、
`log` 等枚举空间，但真实渲染路径仍然是 `ShellTab.paneTree -> ShellPane ->
TerminalHostView`。`ShellPane` 同时承担布局身份、terminal 进程、cwd、runtime metadata、
alan binding、viewport 摘要和 attention 状态。

这个耦合在 terminal-only 阶段是可接受的，但当 Tab 需要承载 markdown、alan 设置页、
embedded browser 或未来 agent-native 工具面板时，会产生三个问题：

- split/focus/move/close 本质是容器操作，却被 terminal lifecycle 语义污染。
- 非 terminal 内容如果绕过 shell tab，会形成第二套导航和窗口模型。
- 如果只是继续扩展 `ShellTabKind`，混合 split tab 仍然无法自然表达。

约束是：现有 terminal runtime continuity 必须保留；默认 UI 仍然 terminal-first、Arc-like、
轻量 material；control plane 仍然需要给 agent 返回可观察、稳定的 mutation 结果。

本设计选择一步到位的模型拆分：旧 `ShellPane` 不再作为新状态的核心实体，而是迁移为
`ShellPaneSlot` 加 `ShellContentInstance`。

## Goals / Non-Goals

**Goals:**

- 将 Space / Tab / Pane 建模为通用导航和布局容器。
- 允许一个 split tab 内混合不同 content。
- 将 terminal runtime identity 绑定到 terminal content，而不是绑定到布局 pane。
- 给 markdown 和 settings 定义 v1 content contract，同时让模型能在后续 change 中承载 browser。
- 让 UI、control plane、persistence、event stream 都能表达 content kind 和 content capability。
- 支持从 `persist-macos-shell-workspaces` 落地后的 terminal-only workspace manifest
  一次性迁移到 content-container state。

**Non-Goals:**

- 不在这个 change 中实现完整 markdown 编辑器、浏览器安全模型、下载管理、cookie/profile 管理或扩展系统。
- 不在这个 change 中实现 browser content kind、browser renderer、WKWebView 集成、浏览器安全策略、下载管理、cookie/profile 管理或扩展系统。
- 不重做 Space / sidebar 信息架构，也不引入第二套 window/tab model。
- 不改变 terminal app input ownership、Ghostty rendering 或现有 terminal search 细节，除非为了迁移到 content boundary 必须调整调用位置。
- 不把 alan 设置页做成 dashboard；settings 只是 shell content 的一种。

## Decisions

1. **Tab 和 PaneSlot 作为容器，ContentInstance 作为内容实体。**

   目标模型是 `Space -> Tab -> PaneLayoutTree -> PaneSlot -> ContentInstance`。PaneSlot
   拥有 split/focus/selection/close/move 的身份；ContentInstance 拥有 kind、title、icon、
   capabilities、renderer state、生命周期和持久化 payload。`ShellPaneTreeNode` 的 leaf
   指向 `pane_slot_id`，不再指向 terminal-shaped `pane_id`。

   v0.2 状态形态：

   ```json
   {
     "contract_version": "0.2",
     "focused_pane_slot_id": "slot_1",
     "spaces": [],
     "tabs": [],
     "pane_slots": [],
     "contents": []
   }
   ```

   Alternative considered: 继续扩展 `ShellTabKind`。这能快速打开 settings tab，但 split leaf
   仍然只能是 terminal pane，未来 terminal + markdown 混合布局还要再拆一次。

2. **Terminal runtime service 迁移为 content-keyed。**

   terminal 的进程、surface、metadata、pending delivery 和 teardown keyed by stable
   `content_id`。PaneSlot 只是 terminal content 当前挂载的位置。移动 terminal pane slot 到
   另一个 tab 时，slot 和 content 的绑定保持连续；未来如果支持 content detach/reattach，也不需要
   重写 terminal runtime identity。

   Alternative considered: v1 继续 pane-keyed runtime，只在 `ShellPane` 上增加 `content` 字段。
   这会降低短期迁移成本，但会长期保留“Pane 到底是布局位置还是 terminal runtime”的歧义，不符合
   一步到位目标。

3. **Content capability 决定命令合法性。**

   通用 pane 命令包括 split、focus、move、close、open content tab。content-specific 命令必须先
   通过 capability 检查。terminal input command 应改为 terminal-specific surface，例如
   `terminal.send_text`；该命令可以接受 `pane_slot_id` 作为便捷目标，但执行前必须解析到承载
   `terminal` kind 的 ContentInstance，并以 `content_id` 调用 runtime。

   Alternative considered: 让每个 renderer 自己忽略不支持的命令。那会让 agent 看到
   `applied: true` 但没有效果，违背当前 control-plane truthfulness 合约。

4. **首批非 terminal content 是 markdown viewer 和 settings。**

   v1 需要能表达并打开 markdown viewer 和 alan settings。markdown 先做 read-only viewer；
   settings 先承载 alan app 设置。Browser 不进入 v1 implementation，也不要求 browser
   ContentInstance descriptor；完整 webview 安全策略和 browser content kind 后续单独 change 细化。
   当前模型名称必须保持通用：`ShellContentKind` 表达可扩展 content kind，`ShellContentPayload`
   承载 kind-specific restore/render 数据，`ShellContentCapability` 表达 command surface。
   因此后续 browser change 只需要新增 browser kind/payload/capability/renderer，而不应把
   `PaneSlot`、`ContentInstance` 或 v0.2 container contract 重命名为 markdown/settings 专用模型。

   Alternative considered: v1 加入 browser host boundary。它能提前验证外部资源和嵌入式 host
   边界，但会把 scope 拉到 webview、安全策略、profile/cookie、下载和导航权限，不适合与模型拆分同批完成。

5. **Persisted state 采用 v0.2 单模型加 v0.1 迁移输入。**

   本 change 在 `persist-macos-shell-workspaces` 之后执行，因此 durable restore 的主要输入是
   `ShellWorkspaceManifest` 中的 terminal-only `pinSnapshot` / `liveSnapshot`，而不是旧的
   `shell-state-window_main.json`。Manifest 迁移时，每个 terminal restore leaf 迁移成
   `ShellPaneSlot` + `ShellContentInstance(kind: terminal)`，并保留 Space/Tab identity、ordering、
   selected state、pin 状态、TTL anchor 和 active-task metadata。

   旧的 `ShellPane` decode compatibility 仍然保留为 v0.1 shell-state/runtime projection 的
   诊断或兼容输入，但不能重新成为 workspace restore authority。旧字段如 cwd、process、context、
   viewport、alan binding 映射到 terminal content payload 或 projection。新 manifest 和新 shell
   state 只 encode v0.2，不长期 dual-write v0.1。迁移失败时进入可诊断 fallback，不能静默丢失
   terminal tabs、slots 或 contents。

   Alternative considered: 重置所有旧 persisted state。实现最简单，但会破坏当前 shell restore
   行为，也不利于验证 terminal continuity。

6. **UI 继续由 shell chrome 统一承载。**

   Sidebar tab rows、toolbar title、pane title bars 和 command UI 显示 content title/kind/capability
   的用户语言。非 terminal content 不拥有新的 page header、dashboard 卡片或独立导航 chrome。

   Alternative considered: 给 settings/browser 单独完整页面 shell。那会让 Tab 看似统一，实际
   交互层级割裂，违背 terminal-first 工作区方向。

## Risks / Trade-offs

- **Risk: 模型迁移范围较大。** -> 先引入兼容 descriptor 和 adapter，让 terminal 通过 adapter
  运行，再迁移 call sites；保留 v0.1 decode 路径，但新写入只使用 v0.2。
- **Risk: 泛化后 terminal 体验退化。** -> terminal continuity、send_text、search、focus 和
  close teardown 的现有 tests 必须继续通过，并新增 content-keyed runtime 回归测试。
- **Risk: control plane 命令语义膨胀。** -> 将命令分为 pane mutation、content creation、
  content-specific runtime command 三类，并为不支持的 content 返回稳定 unsupported code。
- **Risk: browser scope 过大。** -> browser 不进入 v1；只保留模型扩展性，browser content kind
  和 webview 策略后续单独设计；当前只要求 v0.2 命名和 payload/capability 边界保持可扩展。
- **Risk: settings 变成 dashboard。** -> UI spec 明确 settings 是 tab content，继承 shell
  chrome，只显示设置本身，不引入营销式页面布局。

## Migration Plan

1. 在 `persist-macos-shell-workspaces` 归档后的 accepted specs 上实现本 change，先读取已落地的
   `ShellWorkspaceManifest` schema 和 materializer 边界。
2. 增加 v0.2 content-container model，同时保留旧 `ShellPane` decode compatibility。
3. 将 workspace manifest 的 terminal-only restore snapshot 迁移为 content-aware restore
   snapshot；新写入只使用 PaneSlot / ContentInstance shape，不写回 terminal-only snapshot。
4. 将现有 terminal leaf 渲染包成 `TerminalContentRenderer`，并将 runtime registry keyed by
   `content_id`。
5. 将 split/focus/move/close mutation 改为操作 PaneSlot，并由 terminal adapter 处理 terminal
   content teardown。
6. 增加 markdown/settings 的最小 renderer 和 content descriptors。
7. 扩展 control plane DTO 和命令结果，暴露 pane slots、content kind、capabilities、
   `content_id` 和 unsupported-command 语义。
8. 添加 manifest 迁移、mixed split、terminal continuity、non-terminal command rejection 和 UI
   evidence 验证。
9. 旧状态迁移稳定后，再移除不再需要的 v0.1-only projection helpers。

## Open Questions

无。当前已确认：一个 split tab 内可以混合不同 content。
