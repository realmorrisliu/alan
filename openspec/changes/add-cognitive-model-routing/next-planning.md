# 下一步规划入口

基线：PR #921 已于 2026-09-19 合并，main 提交
`3fce4450ad0b1d0412baeec095224d48bc7937d8`。

本文件汇总跨 change 的规划待办，不替代各 change 的规范和实施 tasks。
以下复选框表示**规划完成**，不表示功能交付。现有 parked change 不因本表而
自动激活；开始实施前必须明确范围、补齐所属 capability deltas 和验收任务。
原审查报告是历史证据快照；当前决策以 ADR-0054/0055、canonical specs 和各
change 的 disposition 为准。不要重新执行已取消的 macOS 计划。

## 推进顺序与任务归属

### A. 桌面代码退役与独立 CLI/Host 分发

归属：下一轮建立独立退役 change；本次不创建未经消费者审计的删除方案。
ADR-0054 已完成产品方向退役，**源代码、构建和发布配置尚未移除**。

- [ ] A1 盘点 Apple 客户端、shell-core/FFI、构建、CI、安装及发布入口的实际消费者，逐项标明删除、保留或迁移；不能因目录名称含 Apple 就整批删除。
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
归属：Jev 用本 change 的实施及验收 tasks；终端体验由 C 的 changes 承接。

- [ ] D1 将 B1 场景落实为 shadow evaluation → 受控启用的实施方案，覆盖 no-match、格式错误、超时、不可用、取消及有界 generation fallback。
- [ ] D2 在测量前确定正确动作率、错误自动执行率、升级率、端到端 p50/p95 延迟和成本的通过标准；不足时允许维持确定性或生成基线，不预设必须上线 Jev。
- [ ] D3 完成普通终端兼容后，评估是否还有 Herdr 原生识别/通知需求；仅在有实证缺口时另立薄集成切片，不将 Alan kind 或插件系统作为前置条件。

规划出口：场景、基线、通过标准与回退策略可验证；不把模型宣传延迟当作产品验收。

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
