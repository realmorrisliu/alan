# 下一步规划入口

基线：PR #921 已于 2026-09-19 合并，main 提交
`3fce4450ad0b1d0412baeec095224d48bc7937d8`。

本文件汇总跨 change 的规划待办，不替代各 change 的规范和实施 tasks。
以下复选框表示**规划完成**，不表示功能交付。现有 parked change 不因本表而
自动激活；开始实施前必须明确范围、补齐所属 capability deltas 和验收任务。
原审查报告是历史证据快照；当前决策以 ADR-0054/0055、canonical specs 和各
change 的 disposition 为准。不要重新执行已取消的 macOS 计划。

活动入口的生命周期：本 change 只是路线图的当前保管者。Step 2 完成后，归档前
必须把所有剩余跨 change 待办、顺序和激活门槛移交到下一项已激活但未完成的
change（按本顺序为 `define-alan-programmable-client-surface`），保留任务状态，
更新活动文档的入口引用，并验证剩余工作无需读取 archive 才能找到。若接收者
尚未激活，就先完成该规划交接，不提前归档当前保管者。后续保管者同样在归档前
移交剩余路线；不修改已有历史 archive，不把历史路线图当作当前执行指令。

## 推进顺序与任务归属

### A. 桌面代码退役与独立 CLI/Host 分发

归属：建议新建 `retire-macos-client-and-standalone-cli`，目前尚未创建。
本次不创建未经消费者审计的删除方案。
ADR-0054 已完成产品方向退役，**源代码、构建和发布配置尚未移除**。

- [ ] A1 盘点 Apple 客户端、shell-core/FFI、构建、CI、安装及发布入口的实际消费者，逐项标明删除、保留或迁移；不能因目录名称含 Apple 就整批删除。
- [ ] A1a 按被移除消费者反查全部 canonical requirements/scenarios，而非只按 capability 名称找 `macos-*`；逐项登记删除、改写或因存续消费者而保留的理由，避免代码已删而规范仍要求旧客户端行为。
- [ ] A2 记录 credentials、Host Mounts、sandbox、系统账户和用户数据的存续 owner；为仍需要的适配器列出迁移前后验证，禁止把删除 UI 当成删除安全边界。
- [ ] A3 提出最小独立 CLI/Host 安装和启动路径，补齐受影响的分发、平台与遗留规范 deltas，以及 build/test/install 验收任务。

规划出口：有确切路径级清单、存续消费者和安全验证、批准的删除范围；未完成
适配器接替验证的部分继续保留。不得卸载用户 App、删除数据或修改线上 feed。

### B. 混合 Machine 与执行证据

归属：本 change 的 [tasks.md](tasks.md) 第 1 节；实施和测量分别由第 2、3 节跟踪。

- [ ] B1 选择一个已有、低风险、可判定正确性的场景，定义确定性与仅生成两套基线，明确输入、候选、输出和失败边界。
- [ ] B2 定案 evaluation/generation 能力、版本化 DTO、操作生命周期及预算；补齐 llmfs、provider、Agent Machine/AgentFS 的 owning deltas。
- [ ] B3 明确 Machine state、Tape 投影、rollout/checkpoint 的写入与恢复权威；列出结构化完成、wait/resume、取消和 Unknown effect 场景。

规划出口：每种操作都有可测试的状态与恢复语义；评价只是证据，不改变权限。
不建立独立 router、全局执行管理器或固定的 System 1/System 2 Process 层级。

### C. 真实 Shell 执行路径与终端生命周期

归属：重切 `define-alan-programmable-client-surface`；同步重写
`define-alan-interaction-model`。`expose-agent-rollout-history` 只保留独立的
历史与持久性责任，不允许 renderer 获得第二条启动权威。

- [ ] C1 追踪裸 `alan` 输入到 Shell Process evaluator、runner、受控 spawn 的实际链路，形成最小共享执行切片，而不是先建设全套 editfs UI。
- [ ] C2 明确增量 Process IO、Local Entry 有界保留、取消、EOF、detach/reattach 与 Host 存续的合同及验收任务；重连不得重复执行。
- [ ] C3 重写陈旧 inline-TUI guard，以当前入口的行为证据验证终端链路；把 Herdr pane resize、窄屏、Unicode、paste、滚动历史和退出状态列入验收矩阵。

规划出口：客户端只呈现和输入，执行与生命周期仍归 Alan OS；普通终端可用，
不要求 Herdr 理解 aP，不重新建设窗口、tab、pane 或终端模拟器。

### D. 有测量的 Jev 与 Herdr 纵向闭环

依赖：B 的类型/证据合同；终端闭环还依赖 C 的真实执行与 IO 路径。
归属：本 change 交付共享能力与 Machine 合同后，建议新建
`add-jev-evaluation-adapter`（尚未创建），承接具体适配器和真实任务测量；
终端体验由 C 的 changes 承接，不重复建设运行时或 Connection 框架。

- [ ] D1 将 B1 场景落实为 shadow evaluation → 受控启用的实施方案，覆盖 no-match、格式错误、超时、不可用、取消及有界 generation fallback。
- [ ] D2 在测量前确定正确动作率、错误自动执行率、升级率、端到端 p50/p95 延迟和成本的通过标准；不足时允许维持确定性或生成基线，不预设必须上线 Jev。
- [ ] D3 完成普通终端兼容后，评估是否还有 Herdr 原生识别/通知需求；仅在有实证缺口时另立薄集成切片，不将 Alan kind 或插件系统作为前置条件。

规划出口：场景、基线、通过标准与回退策略可验证；不把模型宣传延迟当作产品验收。

## Step by step：应提交哪些 changes

顺序是主线交付顺序，不要求在设计阶段停止一切独立调查。每一步只激活当期
切片；后续新 change 的名字是建议，不表示目录或完整方案已经存在。

### Step 0 — 已完成：架构基线与旧计划收口

- Change：`close-architecture-review-baseline`，实现已合并于 #921，本轮归档。
- 已完成：ADR-0054/0055、指南纠偏、13 个旧计划的处置、5 组 canonical 修正。
- 参考：`architecture-review.md`（问题与源码证据）、`research-jev-and-fx.md`
  （外部调研）、归档 change 的 `disposition.md` / `verification.md`。
- 未完成的运行功能不计入此步：源代码删除、typed evaluation、Shell 改线、Jev 和 Herdr 验收。

### Step 1 — 新建 retire-macos-client-and-standalone-cli

- 工作：完成 A 的消费者清单，再移除批准范围内的桌面产品代码/构建/发布义务；
  保留或迁移仍被 CLI/Host 使用的平台能力，提供独立安装入口。
- 参考 ADR：0054、0032（Host/boot）、0044（channel lifetime）、0050（mount grants）、0051（secrets）。
- 审核并按实际影响写 deltas：`alan-app-distribution`、`alan-os-host-lifecycle`、
  `host-command-plane`、`provider-connection-contract`、`host-directory-mounts`、
  `os-sandbox-enforcement`、`alan-app-service-integration`、`product-brand-identity`、
  `repository-quality-gate`、`local-entry-service`、`documentation-governance`，
  以及受删除范围影响的 `macos-*`、`shell-core-*`、`shell-workspace-core-contract`。
  这只是审计起点，不是封闭白名单：App 集成中的 retained-legacy-client 场景、
  品牌中的 App/bundle 场景、质量门禁中的 Apple 检查，都要随相应源码消费者
  的移除而删除或改写；共享 aP 边界、macOS Rust 检查和平台安全合同继续保留。
  不批量删除所有 macOS 规范，也不把词匹配直接当作删除依据。
- 源码入口：`clients/apple/`、`crates/shell-core/`、`crates/shell-core-ffi/`、
  `crates/alan/`、`crates/os-host/`，以及实际引用它们的 Just/CI/安装脚本。
- 完成证据：独立 CLI 安装/启动验证、存续凭据/挂载/sandbox 回归、workspace 与
  required CI 通过；无法证明安全接替的适配器留存，而非强行删除。

### Step 2 — 重写并激活 add-cognitive-model-routing 的最小核心切片

- 工作：完成 B，先用确定性代码与可控 fixtures 证明 typed evaluation/generation、
  状态推进及恢复；不让真实 Jev 网络接入成为验证状态机的前提。
- 参考 ADR：0055、0024/0025（内核/依赖）、0019（证据）、0022（Agent 治理）、0051。
- 参考本 change：`proposal.md`、`design.md`、`tasks.md`；旧的两个生成式子 Agent
  路由方案不再适用。当前 deltas 还不齐全，必须补齐后才能实施。
- Owning specs：`llm-file-server`、`provider-connection-contract`、
  `provider-request-controls`、`agent-namespace-runtime`、`agent-file-layout-contract`、
  `evidence-retention-and-projection`、`runtime-harness-contract`；按场景复核
  `sandbox-autonomy-invariants`，不改变 Kernel 对 Agent 的无感知边界。
- 源码入口：`crates/agent-engine/src/agent_machine.rs`、`agent_machine/`、
  `rollout.rs` / `rollout/`、`crates/agentfs/`、`crates/llmfs/`、`crates/llm/`。
- 完成证据：结构化成功、no-match、wait/resume、取消、故障恢复和 Unknown
  副作用都有测试；不以自然语言结束或 text Tape 根替代完整状态证明。
- 归档交接：Step 3–6 中仍未完成的路线移交到下一活动 change，更新入口引用；
  此移交不等于后续功能交付，也不要求 Step 2 等待这些功能全部实现。

### Step 3 — 重切 define-alan-programmable-client-surface

- 工作：只激活 Shell evaluator / runner / 增量 IO / Local Entry 有界生命周期切片。
  旧 change 中更大的 editfs UI、脚本能力和 executable packaging 保持延后。
- 参考 ADR：0036、0038、0039、0045、0048、0049；参考审查 F4–F6。
- Owning specs：`alan-shell`、`process-launch-context`、`local-entry-service`、
  `local-alan-os-attachment`、`alan-renderer-host-contract`；按实际影响更新
  `editable-buffer-interaction`，不能由 renderer 模拟执行来满足场景。
- 源码入口：`crates/alan/src/cli/shell.rs`、`crates/alan/src/shell_command.rs`、
  `crates/shell/`、`crates/os-host/` 及其 Process runner/IO 调用链。
- 完成证据：裸 `alan` 经真实 Shell Process 执行，实时输出来自 Process 文件，
  取消/退出/回收可测；不新增平行 Session manager 或启动通道。

### Step 4 — 重写 define-alan-interaction-model，交付普通终端/Herdr 体验

- 工作：在 Step 3 的共享执行链路上实现安静的 shell-like 呈现，保留滚动历史、
  渐进式信息和结果检查；明确 attach、detach、Ctrl-C、EOF 和 pane 关闭语义。
- 参考 ADR：0054、0046、0047；`research-jev-and-fx.md` 的 fx/终端讨论，
  `architecture-review.md` 第 5 节。fx 是体验参考，不是授权策略或新运行时模板。
- Owning specs：`rust-inline-tui`、`alan-renderer-host-contract`、
  `alan-os-host-lifecycle`、`local-alan-os-attachment`、`tool-result-presentation`；
  重写旧交互 change 的 delta，不保留桌面窗口和 renderer launch 前提。
- 源码入口：`crates/alan/` 的实际 StdioDriver 入口与 `crates/tui/`，先确认消费者，
  不假定裸 `alan` 已使用 TUI。同步修复对应行为 guard 和验收文档。
- 完成证据：普通终端和 Herdr pane 中 resize、Unicode、paste、scrollback、
  取消/退出、detach/reattach 均有记录；关闭呈现不误杀 Host，重连不重复执行。
- 可选项：仅在此验收暴露真实缺口后，另提 Herdr 识别/通知小切片；当前不新建插件 change。

### Step 5 — 新建 add-jev-evaluation-adapter，完成真实任务闭环

- 前置：Step 2 的共享类型与证据合同已交付；终端端到端验收还要求 Step 3/4。
- 工作：沿现有 Connection adapter 接入 Jev，使用 Step 2 选定场景，从 shadow
  测量开始，再按预先确定的通过标准决定是否启用自动选择。
- 参考：ADR-0055、`research-jev-and-fx.md` 的 Jev 能力/限制/评测部分、
  Step 2 的已合并合同和测试。实施前刷新供应商 API、模型版本和可用性资料。
- Owning specs：`llm-file-server`、`provider-connection-contract`、
  `provider-request-controls` 和届时已落地的 `cognitive-model-routing`；
  仅有必要时新增 provider-specific wire contract，不复制治理或恢复逻辑。
- 完成证据：真实 API 的类型/错误/取消处理，和规则、仅生成两套基线比较的
  正确率、错误自动执行率、升级率、p50/p95 与成本；失败可以不启用 Jev。

### Step 6 — 按真实缺口重启后续能力，不作为主线完成条件

- `expose-agent-rollout-history`：需要历史浏览时重写；参考 ADR-0019/0046 和
  `evidence-retention-and-projection`、`agent-runtime-ui-file-surfaces`。Step 2 的
  持久恢复不能依赖完成历史 UI，历史 UI 也不能获得额外启动权。
- `add-proactive-memory-v2`：证据和 Store owner 稳定后再激活；参考 ADR-0006/0037、
  `runtime-memory-contract`、`runtime-memory-surfaces`、`alan-os-system-store`。
- q：只有现有 Skill 分发不能满足已选消费者时再提 executable/binfs change；
  参考 ADR-0009/0030/0041/0052、`package-management-contract`、`skill-system-contract`
  和 `docs/skills_and_tools.md`。当前不预建通用包管理重构任务。
- remote 与各领域 App：先满足各自 disposition 的需求门槛，不从本路线推导恢复开发授权。

以上 spec 名称均指 `openspec/specs/<name>/spec.md`，ADR 编号均指
`docs/adr/` 对应文档；每个 change 实施时只改有具体行为变化的 owning specs，
不为覆盖清单而制造无效 deltas。

## 延后项与不做项

- `add-proactive-memory-v2`：等待 Machine/evidence owner 稳定后重规划 Store commit；不自动学习或提升权限。
- q：维持已实现的 Skill 安装能力和 Package Service 所有权。可执行导出/binfs/system-package 需实际消费者再单独立项；不为 Jev 增加新包类型，安装不等于执行授权。
- `add-alan-anywhere-mvp`、`define-updf-product-umbrella`、`define-groove-master-alan-app`、`spike-macos-matter-controller`：保持 parked，按各自 disposition 的需求门槛重启；不恢复全线并行。
- 已取消的桌面组件、App updater、managed-user PTY 与旧 Voice MVP 保持历史归档，不同步未实现 delta。

## 交付门槛

每个激活的切片都需要范围明确的 proposal/design/deltas/tasks、与风险相称的
验证和当前 HEAD 的 required CI。PR 按 mark ready → Codex review → 根因分析、
必要修复及 resolve → 再次 review 无新问题 → 满足仓库规则后合并。
只在实际实现已合并、对应 delta 已同步后归档；不提前勾选未来交付。

独立维护风险：#921 的 Cargo Audit 报告了未改动依赖中的
RUSTSEC-2026-0285（rustls 0.23.36）。依赖修复需单独验证和交付，不计为本轮
文档或架构工作的完成项，也不将 required CI 通过描述为所有检查通过。
