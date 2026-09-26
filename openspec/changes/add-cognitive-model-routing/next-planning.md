# 下一步规划入口：Tracer bullet

基线：main `53b862b0`（PR #935，2026-09-24；包含 TUI 交付、可靠性修复、alan9 命名迁移及规范同步）。
2026-09-20 用户确认改为纵向 tracer bullet：尽快交付可用 agent，通过真实终端任务反馈推进架构。
本路线替代原 Step 2 → 3 → 4 的逐层交付顺序；ADR-0054/0055 的所有权边界不变。
ADR-0056 记录裸 `alan` 直接附着 Root Agent 的入口决策，并取代旧 Shell-first 指引。
ADR-0058 后续接受同一 Machine 组合确定性命令与 Agent 推理；命令/输入语义由
[unify-agent-command-input](../unify-agent-command-input/disposition.md) 接管，当前代码仍未交付该行为。

## 已完成与当前状态

- 架构基线已收口：ADR-0054 退役桌面产品，ADR-0055 接受同一 Machine 组合确定性计算、typed evaluation 和 generation。
- 独立 CLI/Host 分发已完成并归档：
  [retire-macos-client-and-standalone-cli](../archive/2026-09-20-retire-macos-client-and-standalone-cli/)。
  源码删除已由 [remove-retired-desktop-source 归档记录](../archive/2026-09-23-remove-retired-desktop-source/)
  完成，App 和 shell-core/FFI 已移除；平台安全能力继续保留。
- 首切片已由 PR #929 实施并合并，已同步实现的 delta 并归档为
  [define-alan-programmable-client-surface](../archive/2026-09-24-define-alan-programmable-client-surface/)；后续可靠性/体验缺口留在切片 2。
- 原路线所列 rustls 风险已由 PR #923 中的依赖修复处理，当前锁定 0.23.45，不再作为待办。

## 切片 1：一个可用 agent 闭环

主负责 change：[已归档的 define-alan-programmable-client-surface](../archive/2026-09-24-define-alan-programmable-client-surface/tasks.md)。
必要的终端呈现也在此切片交付；interaction change 只承接后续体验，不重复维护同一 delta。

固定验收任务：在普通终端或 Herdr pane 启动 Alan，通过现有 Connection，
让它检查一个明确授权的测试项目目录，调用只读工具并给出带路径依据的结果。
测试目录包含已知内容和不可访问的边界，便于判断回答和授权是否正确。

- [x] 追踪裸 `alan` → LocalAttachment → TTY 分支的 file-backed renderer → 现有 `/agent/root` → generation → 受控 Tool → AgentFS IO；重定向 stdin 提交一次 Agent task。
- [x] 明确当时的输入边界：TTY composer 提交 Agent task，不本地解析 shell 语法；当前 `!` 仍作为 Agent 输入，不是已交付的确定性命令路由；管道输入只提交一次 Agent task。
- [x] 已重写本 change 的 proposal/design/deltas：只改 `alan-shell` 与 `alan-renderer-host-contract`；旧 editfs/run/binfs 前置条件不再属于本切片。
- [x] 已接通裸 `alan` 的 TTY → 现有 `/agent/root` file-backed renderer 和重定向 stdin 的 one-shot Agent 路径；无 Connection 时给出准确错误，普通 TTY、Herdr pane 与管道输出均已验证。
- [x] 配置 dev Connection 后，交付只读工具任务、AgentFS 增量输出、完成、Ctrl-C 取消和取消后的成功后续任务；验收记录见主 change 的 tasks.md。
- [x] 验证工具失败、未授权路径和取消后不继续派发动作；不可用 Connection 的清晰错误已在两种终端中验证。
- [x] 在普通终端和 Herdr 记录同一构建的成功任务、输入/输出与退出结果，证明连续任务和取消后仍可使用；记录构建 `bef854e3`，该提交是当前分支祖先。

当前状态（2026-09-24）：上述固定验收已在构建 `bef854e3` 的普通终端和 Herdr pane 完成并记录；PR #929 合并了重连边界与路径翻译回归修复，并通过当前 HEAD CI。此记录不表示 dev Host 仍在运行，也不宣称后续 detach/reattach 输出 offset 与缺口提示已交付。

复用已有 generation 能力；typed evaluation/Jev 不作为启动条件。
权限、credentials、sandbox、Process 生命周期和证据写入继续走现有 owner。
首切片不承诺跨 Host 重启恢复，但不得新增重连自动重发、权限绕过或重复执行路径。

参考：ADR-0036/0038/0045/0049/0054/0056，
`alan-shell`、`local-alan-os-attachment`、`alan-renderer-host-contract`、
`agent-file-layout-contract`、`provider-connection-contract`、
`host-directory-mounts` 与 `host-mount-escalation`。真实入口从
`crates/alan/src/main.rs`、`crates/alan/src/cli/host.rs`、
`crates/os-host/src/local.rs`、`crates/service-manager/src/agent_runtime.rs`、
`crates/shell/` 和 `crates/tui/src/file_backed.rs` 追踪；本切片不新增
Shell evaluator Process。

## 切片 2：同一任务的执行可靠性与 shell-like 终端体验

此切片已完成并归档为
[define-alan-interaction-model](../archive/2026-09-24-define-alan-interaction-model/)。
PR #931 实现 inline transcript/prompt、临时 completion、可见错误与安全 detach；
PR #932 修复该 CI 路径暴露的 AgentFS 测试同步竞态。PR #932 的 macOS/Ubuntu
测试、覆盖率、质量、安全与 OpenSpec 检查均通过。Codex 对当前提交未发现 major，
原 PR 的 review threads 已全部 resolve。规范只同步实际交付的 renderer 行为；
append-only Stream 不承诺 retention-gap recovery，Process/Agent Runtime/证据所有权
不进入 renderer。输入路由保持既有行为。

参考 ADR-0019/0044/0046/0047/0055，以及
`evidence-retention-and-projection`、`alan-os-host-lifecycle`、
`rust-inline-tui`、`tool-result-presentation`。
fx 体验参考见 [research-jev-and-fx.md](research-jev-and-fx.md)。
历史浏览 UI、完整 editfs UI、脚本系统、通用 executable packaging 不作为前置。

## 下一个实施顺序

1. 已完成 [rename-alan-os-to-alan9](../archive/2026-09-25-rename-alan-os-to-alan9/)：PR #934 迁移
   README、AGENTS、指南和活动 OpenSpec 的解释性命名，PR #935 同步 canonical
   spec；保留 Alan 产品、`alan` CLI、crate/type/protocol/storage 标识、路径和历史决定。
2. 按 ADR-0058 实施
   [unify-agent-command-input](../unify-agent-command-input/) 的显式输入阶段：
   `!` 原样交给受治理宿主 shell，`:` 强制 Agent，无前缀暂走 Agent；共享 cwd、
   排队/取消、证据关联与恢复不重放复用现有 Process/Machine/Host Mount/sandbox。
   不引入第二个 executor/router，不改写命令路径文本，也不把 aP 暴露为用户协议。
3. 自动分类、typed evaluation、模型列表/选择与 Jev 仍是后续独立资格评估；
   不因当前两个 change 的文档完成而视为已实现或已启用。

## 切片 3：在已跑通任务上加入混合 Machine

归属：[add-cognitive-model-routing](tasks.md)。
从切片 1 的真实任务中选一个低风险、可判定的候选选择或评价点，
保留确定性与仅生成两套基线；具体选择须有调用链证据，不预建全局 router。
ADR-0058 的显式 `!`/`:` 交付归属
[unify-agent-command-input](../unify-agent-command-input/tasks.md)；后续消费 typed evaluation
及 unprefixed shadow qualification 已移交活动后继
[qualify-agent-input-routing](../qualify-agent-input-routing/tasks.md)；
自动分类的 side-effecting route 仍须通过 false-execution 门槛并经明确启用，不能由 renderer 或候选 UI 暗中激活。

- [ ] 定案 evaluation/generation 能力、版本化 DTO、操作生命周期、预算和 fallback。
- [ ] 补齐 llmfs/provider、Agent Machine/AgentFS、执行证据 owning deltas。
- [ ] 用可控 fixtures 验证 typed success、no-match、格式错误、超时、取消、结构化完成、wait/resume 和 Unknown 处理。
- [ ] 复用切片 2 已交付的恢复合同；明确 Machine state、Tape 投影、rollout/checkpoint 的写入权威。
- [ ] 为 TUI model picker 定案 Connection ownership：自动刷新权威 catalog；已选模型覆盖自动选型；无选择时优先选 catalog 中可用的 `gpt-6-luna`，不可用时明确显示提供方默认/失败；model change 必须实际绑定到下一 turn，不能只改 UI。

参考 ADR-0055、0024/0025、0019、0022、0051，以及本 change 的 proposal/design/tasks。
相关 specs：`llm-file-server`、`provider-connection-contract`、
`provider-request-controls`、`agent-namespace-runtime`、
`agent-file-layout-contract`、`evidence-retention-and-projection`、
`runtime-harness-contract`，按风险复核 `sandbox-autonomy-invariants`。
fixtures 证明合同和恢复行为，不作为真实模型延迟或成本收益证据。

## 切片 4：Jev shadow evaluation 与受控启用

建议新建 `add-jev-evaluation-adapter`，当前尚未创建。
复用切片 3 的 Connection、类型、治理和恢复合同。

- [ ] 刷新供应商 API、模型版本及可用性，接入真实 adapter。
- [ ] 在同一任务上先 shadow evaluation，覆盖错误结果、no-match、超时、取消和有界 fallback。
- [ ] 测量前确定正确率、错误自动执行率、升级率、端到端 p50/p95 和成本的通过标准。
- [ ] 与确定性、仅生成基线比较，再决定是否启用；未达标可继续使用原基线。

参考 ADR-0055、[Jev 调研](research-jev-and-fx.md)及切片 3 已合并合同。
不新增平行运行时、全局 router 或认知包类型。

## Herdr 开发与验收循环

使用 herdr skill，先确认 `HERDR_ENV=1` 并读取当前 CLI 帮助。
在当前 cwd 创建相邻 pane，保留用户焦点；记录返回的 pane ID。
当前 pane 开发，兄弟 pane 运行当前构建的 Alan，通过输入、读取输出、
取消和重跑验证同一个任务。普通 pane 命令足够，不要求 Herdr 原生识别 Alan。

自动化测试检查执行、权限和恢复不变量；实际终端操作检查输入和呈现。
日志不能单独证明布局、滚动或 resize 体验。普通终端也必须通过。
仅操作自己创建的调试 pane，不触碰其他 pane、稳定用户数据或主 Herdr 服务。
有真实缺口时再规划 Herdr 通知/识别薄集成。

## 延后项

- `expose-agent-rollout-history`：按需增加历史浏览，不获得启动权。
- `add-proactive-memory-v2`：等 Machine/Store owner 稳定后再重规划。
- q：维持 Skill 分发，有实际消费者再规划 executable/binfs。
- `product-brand-identity` allowlist 校验：规范要求与当前 quality/CI 实际行为不一致；作为独立后续 OpenSpec 项目处理，不纳入 alan9 命名迁移。
- 通用 executable/binfs/WASM 打包不列入本轮交付或承诺后续建设；需要真实消费者时重新论证。它与已退役的 macOS App 打包不同。
- 原生桌面 GUI 计划取消，不是延后；旧 change 中的桌面窗口、原生面板和 macOS UI 验收不再恢复。源码删除见上述独立 change；保留 Rust 平台安全 owner 和用户数据。
- Anywhere、UPDF、Groove、Matter 保持 parked。
- 已取消的 macOS 组件、updater、managed-user PTY、Voice MVP 保留历史，不同步未实现规范。

## 交付与路线交接

每个切片先完成必要的 proposal/design/deltas/tasks，再实施；按实际行为变化
更新 owning specs，不要求先完成整层架构。一个 requirement 只由一个活动 delta 负责。
涉及多个 change 时记录依赖与交付证据，不把一个 change 的测试代替另一项完成。

PR：mark ready → Codex review → 根因分析、必要修复及 resolve →
当前 HEAD 无新问题且 required CI 通过 → 合并 → 同步已实现 deltas → 归档。
只同步已实现范围，延后内容须显式移交，不能整包同步旧 deltas。

本文件暂由 cognition change 保管；归档前必须把所有未完成路线移交给
当时已激活且未完成的 change，并更新所有活动入口引用。
接收者按实际状态选择，不再固定为可能已先完成的 programmable-client change。
如果没有接收者，先完成交接安排，不将剩余路线沉入 archive。
