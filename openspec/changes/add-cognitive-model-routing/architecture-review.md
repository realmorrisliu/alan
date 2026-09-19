# Alan 整体架构审查与重构建议

审查日期：2026-09-19。

状态：非规范性审查报告。本文记录证据、用户已明确的产品方向，以及待进入正式 OpenSpec delta / ADR 的建议；不修改现有规范的效力，不授权执行旧 change 的任务。与本目录 `research-jev-and-fx.md` 配套阅读。

## 1. 结论

Alan 值得继续的核心是 **file-native、可编程、有明确权限边界的个人计算环境**，而不是一个自有终端 App，也不是两层聊天模型包装器。

建议收敛为：

```text
Herdr：窗口 / workspace / pane / PTY / 终端滚动历史
    │ 标准终端输入输出；可选、薄的状态与通知集成
Alan CLI + 终端 renderer
    │ aP attachment；呈现与控制，不拥有运行真相
Alan Shell：普通 Process，显式命令与 Agent 入口
    │ spawn / descriptors / namespace
Agent Runtime Service：Agent Machine
    ├─ 确定性推进
    ├─ typed evaluation，例如 Jev
    ├─ generation，例如复杂规划、内容与参数生成
    ├─ 已授权的 Tool / file effects
    └─ wait / resume / complete / fail
Alan OS：Kernel + Service Manager + File-Server Services
    ├─ /proc：Process 生命周期
    ├─ /agent：Agent IO、控制、Machine 视图
    ├─ Memory Stores：跨 Process 的连续性
    └─ Package Service / q：安装、版本、只读内容与引用
```

这不是建议新增一个总控层。上述认知推进应收敛到现有 Agent Machine，生命周期继续归普通 Process。

核心判断：

1. **保留 Plan 9 底座，重写生成中心的认知合约。** System 1 / System 2 是能力与工作方式，不应固定对应两个 Agent Process。
2. **保留 Turing Machine 思考模型，解除“transition = 一次 LLM generation”的限制。** 模型是 Machine 可调用的能力，不是唯一的状态推进方式。
3. **按用户最新决定退役 Alan for macOS。** 不再建设自己的窗口、tab、pane、终端模拟器、桌面组件系统和 App updater；以 Herdr 为首选终端宿主。
4. **保留独立 Alan OS Host。** 客户端退役不等于把 Alan OS 的生命周期交给 Herdr，也不等于取消 macOS 平台支持。
5. **q 暂时维持真实的 Skill 分发能力。** 不为 Jev 发明新的包类型；通用 executable / system-package 分发是另一项尚未完成的基础能力。
6. **先修 Shell 的真实执行路径与状态所有权，再做 fx 风格体验。** 否则只会在新 renderer 中重建第二套执行系统。

## 2. 审查基线与证据边界

本地 HEAD：`576fb4752e098e93f076c10f9daba1d200cc3b7a`，分支 `codex/define-host-mount-request-grant-terms`。

本次获取的 `origin/main`：`1be3a5d15d7513b6452908591ca348c3cd7d8a15`。本地 / 上游独有提交数为 `1 / 28`。两者树差异仅为 `Cargo.lock`、根 `Cargo.toml`、`crates/llm/Cargo.toml`；设计文档与 Rust 源码相同。9 月仍有依赖维护，不能说仓库完全停止，但“核心设计仍停留在约两个月前”成立。

资料清单：50 份 ADR、75 个 canonical capability、13 个活动 change。完成了全量目录/引用扫描，重点深读认知、Process、Host、Shell、package、memory、授权和恢复链路。不是对全部 75 个规范每个场景的形式化证明，也不是对全部 Apple 实现的运行验收。

实现结论来自源码追踪。未运行完整 Rust 测试、Apple 构建、真实 Jev API 或 Herdr 集成端到端测试。本轮只新增审查文档，不更改运行代码、规范、系统账户、发布配置或历史 archive。

优先级：P0 表示继续设计前必须定案的架构前提，不表示已确认的安全漏洞；P1 表示近期必须解决的合同/实现偏差；P2 表示独立后续能力或文档收尾。

## 3. 四个核心概念应如何重新对齐

| 概念 | 应保留的含义 | 应删除的隐含前提 |
| --- | --- | --- |
| System 1 / System 2 | 对既有候选快速评价，以及开放式生成/规划，两类能力可组合 | 小模型/大模型就是两种 Process；System 1 必须只读；System 2 天然拥有更大权限 |
| Agent Machine | 当前状态 + 输入事件 + 能力结果 → 下一状态及受控操作 | 每一步都生成 token；无 Tool call 即停止；最终一定输出自然语言 |
| Plan 9 式 Alan OS | 文件、descriptor、namespace、普通 Process、mountable service 是组合和权限基础 | Kernel 需要理解认知模式；模型可自行授予权限；renderer 是执行管理器 |
| Quartermaster | 安装内容、版本、引用和生命周期，由 Package Service 持有 | 安装等于授权；Skill 等于 executable；q 升级即可升级当前所有系统服务 |

仓库目前未找到 `alan9` 名称。正式文档宜继续使用 Alan OS / Alan Kernel / aP；“Plan 9-like”描述设计来源，不额外建立一个与 Alan OS 重叠的产品对象。

### 3.1 System 1 不再是一个便宜的聊天代理

Jev 应首先作为 typed evaluation 能力使用：对受限、已解析的候选作选择或评分，返回结构化结果。它不负责凭空生成任意命令参数，不直接拥有 Tool authority，也不负责决定自己的判断是否足够安全。

System 1 可以选择一个已授权的动作，所以没有必要按模型类别强制只读；System 2 可以提出复杂方案，但不因此自动得到执行权限。隔离与授权取决于动作和 Process 的能力，而不是模型速度或名称。

最小认知循环应允许：

```text
事件 → 确定性检查 → [直接完成 | evaluation | generation | 等待]
                               ↓
                         结构化决策/提议
                               ↓
                   Agent Runtime 授权与 effect lifecycle
                               ↓
                       结果证据 → 下一次推进
```

不是每次输入都先请求 Jev；明确命令、明确输入模式、已有确定结果的步骤直接按规则处理。需要隔离生命周期、预算或能力时才 spawn 子 Process。不要建立“每个判断必有一个子 Agent”的固定税。

### 3.2 Turing Machine 保留为抽象，不保留生成式限制

目前 `AGENTS.md:60–65` 和 `CLAUDE.md:39` 把 transition / halt 写成 generation / final text。实际代码已经有比聊天 Tape 更丰富的 `MachineTransitionState`，包括 pending、yield、plan、replay、deferred 状态：`crates/agent-engine/src/agent_machine/transition_state.rs:79`。

建议定义 Machine 的受控状态推进，而不是宣称某个概率模型本身就是完整的图灵机转移函数。模型能力可以视作外部、可能失败且不确定的计算调用；文件与效果仍有自己的确定性语义。这是工程抽象，不是新的可计算性证明。

必须分别定义：工作完成、等待输入、等待外部结果、暂停、失败、Process exit。Root Agent Process 可在完成一次工作后继续等待；一次成功也可以只有结构化结果。

### 3.3 不要将事件驱动扩张成另一个 OS

沿用已有流、offset、文件服务、Process 控制和 Machine 状态。ADR-0015/0017 已避免新增全局 Subscription primitive；ADR-0019/0020 将证据与产物放在 Kernel 之上。不要另建全局 Task / Conversation / CognitiveEventBus / SchedulerManager 来代替已有所有者。

长期规则、定时器、watcher 的持久化和触发拥有者尚需专门定义；“Machine 能处理事件”不自动等于“完整主动 Agent 产品已经存在”。

## 4. 关键发现及更新要求

### F1 · P0：活动 cognition change 仍在规定旧架构

`openspec/changes/add-cognitive-model-routing/specs/cognitive-model-routing/spec.md:28–103` 仍要求 S1/S2 独立 Process、S1 只读、固定深度升级及 S1 生成 escalation 记录。

此前更新的只有研究报告。**proposal / design / tasks / deltas 仍不能直接执行。** 应整体重写为混合 Machine 与 typed model operations；否则实现会与已经讨论清楚的方向相反。

### F2 · P0：Connection 与 llmfs 只有生成合约

证据：

- `openspec/specs/agent-namespace-runtime/spec.md:35`：打开 Connection、提交 request、读 token stream。
- `openspec/specs/llm-file-server/spec.md:28`：模型调用一律为 Generation。
- `crates/llm/src/provider.rs:19,43`：`generate` / `generate_stream`。
- `crates/llmfs/src/request_wire.rs:12–29,109–112`：v2 wire request 以 messages/tools 为核心，messages 不可为空。
- `crates/llmfs/src/generation.rs:15–45`：事件覆盖 text/thinking/tool_call 等，不是 typed evaluation answers。

应增加 provider-neutral、版本化的 evaluation request/result 与 capability discovery，明确候选、无匹配/拒答、分数含义、错误、取消、预算、版本与 provenance。复用现有 clone / commit / events / status / ctl 生命周期即可；一次非流式评价可以产生一个 typed result 和终结事件。

不应把 Jev 私有 schema 藏进 `extra_params` 当长期接口，也不应将评分伪装成 assistant 文本或 token。operation discriminator 或独立 operation path 二选一，在该 change 内定案，不同时维护两套完整调用栈。

### F3 · P0：Tape 的“真相”承诺与恢复实现不一致

ADR-0024:53–55 和 `agent-file-layout-contract/spec.md:189–191` 称 `machine/tape` 为 truth。但 `namespace_environment/agent_files.rs:404–414` 的 `TapeRecordV1` 仅有 version/kind/role/content；`:140–149` 写 user/assistant 文本投影，`turn_execution.rs:574–583` 仅投影非空响应文本。

Tool 结果进入内部 Machine 与 rollout：`runtime/tool_execution.rs:266–275`；恢复读取 rollout：`agent_machine/recovery.rs:240` 起。

已有 CAS，不能误报为“没有内容寻址状态”。但文本 Tape 的 CAS root 不等于完整 Machine checkpoint。应明确：

| 对象 | 建议明确的职责 |
| --- | --- |
| Machine state | 当前控制状态、pending operation/effect、wait、预算等 |
| durable rollout/checkpoint | 可恢复的执行证据及 checkpoint 关联 |
| prompt projection | 某次 generation/evaluation 所需的最小输入 |
| AgentFS Tape/视图 | 对外可观察与可编辑的明确版本化投影；若继续称完整 truth，就必须补齐信息 |

在这一步定案前，不宜承诺完整 fork/replay、自我学习、无损恢复。不要在现有 rollout 旁新建第二份独立“认知历史”。

### F4 · P0：Shell 身份存在，但默认执行仍发生在客户端

`crates/alan/src/main.rs:637–647` 使用 `Shell::new(attachment.root)` 和本地 `StdioDriver.run`，而非把输入送给服务端 Shell Process 的输入消费者。

`service-manager/src/local_entry.rs:107–125` 分配 Shell Process，但给其 `/proc` 配置 `SystemProcessRunner::new(None, None)`；`process_runner.rs:47–56` 的 `/bin/alan-agent` 分支需要 agent_runtime，否则返回 127。该 runner 也没有实际 `/bin/alan-shell` 执行分支。

因此当前“Shell 是普通 Process”主要实现了 identity/namespace 外壳，尚未形成完整的服务端 Shell evaluator。此结论来自静态追踪，未做运行复现。

建议复用同一个 evaluator，补齐真实 Shell Process IO 消费及 runner wiring。Herdr 中的 renderer 只负责终端交互，不能成为新的隐藏执行 owner。

同时，`rust-inline-tui/spec.md:136–150` 要求裸 `alan` 进入 file-backed Rust TUI，当前 main 实际走 StdioDriver；README、AGENTS、CONTEXT 的入口描述也需统一。先定入口合同，再改 renderer，不能按旧文档假定当前已走 TUI。

### F5 · P0：活动交互/历史方案引入了另一条 launch authority

ADR-0038/0048 要求通过命令平面启动普通 Agent，renderer attach IO；但 `expose-agent-rollout-history/specs/local-entry-service/spec.md:7–22` 和 `define-alan-interaction-model/design.md` D7 给 renderer 专属 `/mnt/agent-runtime/clone`，不让普通 Shell/子 Process 继承。

这是**活动方案与已接受原则的冲突**，不是已验证运行中的越权漏洞。建议保留普通命令平面启动，让 renderer 输入显式启动请求；将 history quota、观察权限、durability 要求与 launch authority 分开。若要改变原则，必须显式 supersede ADR，不能通过一个历史查看功能隐式改变执行架构。

macOS 客户端退役后，更没有理由为其保留专属 launch bypass。

### F6 · P1：实时 Process IO 与 Local Entry 回收尚不完整

`kernel/src/procfs/file_server.rs:588–591` 等 runner 完成后一次性追加输出，不满足 fx 式实时 shell。`define-alan-programmable-client-surface/design.md:141–174` 已规划 incremental sink，应复用该方向而不是在 TUI 开第二条 stdout 真相通道。

`local_entry.rs:30,99–142,208–218` 使用持续增加的 BTreeMap；drain 标记退出但未回收/限制条目。与 `local-entry-service/spec.md:16–18` 的 bounded state 承诺不符。需在 owning lifecycle slice 补 retention/tombstone 上限。

### F7 · P1：模型评价不能代替授权，恢复不能重做未知副作用

现有 `runtime/tool_effect_lifecycle.rs:162` 在执行前持久化 Unknown 并 flush；`:97–123` 区分 Unknown 与 Applied。这是应复用的基础，不应推倒。

所有 **Agent 发起的** 动作，无论源于代码、Jev 选择还是生成模型提议，都进入现有 action governance / effect lifecycle。ADR-0022 已明确该治理是 Agent Runtime scoped，不能把普通人类 Shell 命令和所有 App 操作强制纳入同一个 AI 风险评分器。

Unknown effect 恢复需要确认/消歧，不承诺 exactly-once 外部执行。判别 replay 与效果 replay 分开；评价的高分不是权限证明，也不能改变 grants、credentials 或 namespace。

Kernel 已有 namespace subset / reachability 检查（`procfs/file_server.rs:484–516`），所以 ADR-0027 把所有能力约束都描述成“尚待落地”已过时。但这些 aP 逻辑检查不等于隔离恶意 native syscall；Host subprocess 仍需要 OS sandbox。退役 GUI 时尤其不能误删仍承担安全边界的 Host adapter。

### F8 · P1：Memory change 仍携带旧 Workspace/Host backing

`add-proactive-memory-v2/design.md:86–87,146` 使用 `.alan/runtime/<channel>/memory`；canonical `runtime-memory-contract/spec.md:151–159` 已要求 System Store ownership，不从 cwd 推断权威。活动设计中的 `alan memory ...` 也需重新核对当前 Host / OS command plane，而不是继续扩张 Host CLI。

canonical memory spec 称 Runtime 唯一 writer，活动 change 则转向 Store 提交：应明确 Runtime 提议/调度，Store 验证并原子提交的边界。

Jev 可评价记忆候选，不自动解决记忆内容生成，更不能把一次成功执行自动升级为长期行为策略。事实、偏好、程序性知识、routing policy、权限是不同对象；promotion 需要来源、版本、评价和回滚，权限变更仍需独立授权。

### F9 · P1：q 合同混淆了 service handle 与挂载树

`package-management-contract/spec.md:50–57` 写 `/srv/package/ctl`；实际 `quartermaster.rs:23` 操作 `/mnt/package/ctl`、`result`，`runtime/supervisor.rs:574` 从 `/srv/package` 取得 handle 后挂载服务树。

应修正规范：`/srv/package` 是 rendezvous handle，`/mnt/package/{catalog,status,ctl,result}` 才是文件操作面。这个偏差与 Jev 无关，可独立修复。

### F10 · P1/P2：q 的当前能力、system-package 目标和 Skill 资产被混为一谈

当前 q 的真实链路是 namespace source snapshot → Package Service materialize/catalog → immutable revision lease → `/lib/pkg/<id>` → Skill descriptors。

- `package.rs:52` 的 `PackageExport` 只有 skill_id/root/dependencies。
- `package/materializer.rs:172,197` 解析 Skill roots。
- `runtime.rs:767–803` 取得 revision、挂只读 projection、构造 Skill reference 并保留 lease。
- ADR-0030:87 和 package spec:150,184 明确 v0 Skills only，包内 `bin/` 不自动注册成 Tool。
- `runtime.rs:674–719` 的 BootManifest / ToolRegistry 路径独立于 q catalog。
- `boot_unit.rs:198,218` 仍通过 `include_str!` 嵌入系统 boot 内容，与 ADR-0041 的 system-package transaction 目标有差距。

因此 q 不是毫无基础，也尚不是通用 executable 分发器。文档需区分 distribution package、Skill directory、Tool manifest。`docs/skill_authoring.md:31–51` 推荐的 bin/scripts 应明确是资产、Host authoring hook，还是实际 Alan Tool，不能让读者认为安装即执行。

Jev rubric、候选描述可先是 Agent Definition / Skill 的普通文件；只有出现独立分发需求才走 q。模型 profile 归 Connection Service，secret 归 Host Store；运行评分与预算归 Machine，判别证据归 rollout，不进 package catalog。

给 Jev 的 Skill 候选必须来自当前 Process 已解析的可用 capability view，遵守 `enabled`、依赖、explicit reference 与 `allow_implicit_invocation`，不能直接拿全局 catalog 作为可执行候选。

## 5. 退役 Alan for macOS，融入 Herdr

### 5.1 这是删除一个产品责任，不是更换 Swift UI 库

用户已明确不再需要 Alan for macOS。建议新增一个退役与终端宿主边界 change，正式撤销自有桌面客户端的产品义务；不要继续修复其组件系统、自动更新或 UI parity，随后才计划删除实现。

| 对象 | 建议处置 | 原因/边界 |
| --- | --- | --- |
| `clients/apple` 窗口、sidebar、pane、Ghostty renderer、App Intents、桌面 UI | 退役 | Herdr 已承担用户需要的终端组织体验 |
| Sparkle、App bundle/cask 分发、Apple UI 测试与截图流水线 | 随客户端退役清理 | 不继续为已放弃的产品支付发布和验证成本 |
| `alan-shell-core` / `shell-core-ffi` | 倾向退役，删除前核实消费者 | 当前 Rust 外部使用主要是 FFI；其 workspace/tab/pane domain 是自建终端宿主模型，不是 `alan-shell` evaluator |
| `crates/shell`、`crates/tui`、`crates/alan` | 保留并重新明确分工 | file-native shell、终端 presentation、Host CLI 是不同层 |
| `os-host`、aP 本地 attachment、Service Manager | 保留 | Herdr pane 关闭不应被等同于整个 Alan OS 退出 |
| Connection secrets、Host Mount、OS sandbox adapter | 保留所需实现 | GUI 不是凭据与执行隔离的语义 owner |
| privileged helper、managed terminal accounts | 逐项拆分/评估，不打包删除 | GUI 终端账户功能可退役；仍被安全执行依赖的能力必须有等价边界 |
| Memory Stores、installed packages、用户 authored content | 保留，显式迁移/备份 | 不能因 UI 退役清空 System Store / Host Store |

“退役 macOS App”不意味着删除所有含 `macos` / `apple` 的文件。Rust macOS CI、平台凭据、沙箱、Host service 仍可能需要。也不意味着自动卸载用户机器上的 App、删除账户、撤掉网站/feed 或撤销发布密钥；这些外部动作需要单独的执行范围。

### 5.2 Herdr 集成分两层，不先造插件系统

本次确认 `HERDR_ENV=1`，只读查看安装的 Herdr CLI。版本为 **0.9.1**；`herdr agent` 的支持 kind 与 `herdr integration` 列表中均没有 Alan。未创建 pane、安装集成、发送通知或控制其他 Agent。

第一层：普通终端程序兼容。Alan 在 Herdr pane 中通过 stdin/stdout/PTY 运行，不要求 Herdr 理解 aP。优先验证：resize、窄屏、Unicode、bracketed paste、Ctrl-C、EOF、滚动历史、退出码与重连。fx 的安静 inline/normal-buffer 体验值得借鉴；不照搬其权限 reviewer，也不重建 tab/workspace。

第二层：可选原生 agent 识别。需要确认/实现 Herdr 支持的识别或 integration hook，让 Alan 的 working、waiting approval/input、completed、failed 被正确展示。当前 CLI 没有 Alan kind，不能宣称 `herdr agent start --kind alan` 已可用，也不应该假装成另一个 agent kind。

不要靠抓终端文本反向决定 Alan 的权限或运行状态。Alan 的事实来自 Process/AgentFS；Herdr 的状态标记只是投影。pane ID、tab ID 也不能成为 Alan PID 或 durable work ID。只把必要状态映射给 Herdr，避免把 prompt、secret 或完整 namespace 泄漏到通知/窗口标题。

普通终端支持应独立可用；没有 Herdr 时也能运行 Alan。首版不要求 aP 插件、Herdr 专用任务数据库、跨产品 Session manager 或新的远程协议。

### 5.3 生命周期要补一个明确合同

保留“关闭视图只 detach”的原则，但不要笼统说所有终端子进程永不终止：

- Alan renderer/CLI 接到 EOF、SIGHUP 或 pane 关闭，释放 attachment；不得因此关闭 system-level Host。
- 显式 detached 的 Agent 工作继续，状态由 `/proc` 与 durable evidence 持有。
- foreground Shell 工作的信号转发、entry drain 和子进程取消策略需要明确；不能靠 Herdr 视觉状态推断。
- Ctrl-C 是用户取消当前工作还是退出 renderer，需要区分前台状态，不能默认为“关闭整个系统”。

上述场景尚未验证，应成为新终端合同的验收项。

## 6. ADR 更新地图

ADR 应记录历史决策及显式 supersession，不静默重写当年的事实；新的规范行为仍通过 OpenSpec delta 落地。immutable OpenSpec archive 不改。

| ADR | 建议 |
| --- | --- |
| 0001 | 自有主窗口 summon 决策随客户端退役 supersede，不转移成 Herdr 的布局要求 |
| 0002–0006 | 保留 Root 普通 Process、受治理权限、request-owned、descriptor context、memory 两轴；补事件工作与 request 生命周期解释 |
| 0007 | 保留可选 Agent Workspace 概念，但不再默认由自有 macOS App 承载；没有近期消费者则不新增实现 |
| 0008–0011 | 保留 Runtime 服务、Executable/Tool/Skill 区别及治理；0009 的 `/lib/skill` 示例与当前 `/lib/pkg` projection 对齐 |
| 0014–0023 | 保留小 Kernel、流、证据/产物上移、标准 namespace、Agent-scoped governance；不因 event-driven 添加全局对象 |
| 0024 | 保留 Plan 9 决策，增补 typed evaluation、Machine/Tape/durable record 分工；限定 namespace capability 与 OS sandbox 的关系 |
| 0025 | 更新 crate/依赖图和退役 surface，展示实际 Service Manager / OS Host；不把 Jev 引入 Kernel |
| 0026 | 保留组合思想；删除/标注已经失效的“尚未创建 change”状态；fork 不意味着免费复制外部效果 |
| 0027 | 重做当前路线图快照；Ring 2/3/4 状态落后代码与规范；移除 Apple 客户端中心；重新确认专家可编程与默认渐进披露并存 |
| 0028 | 保留 remote namespace/lease 原则；Herdr 已有终端宿主不证明 Alan remote service 已实现，也不强迫重建远程产品 |
| 0029 | 保留历史清理决策，增加后续 ADR/退役决策指针，不再引用它证明“当前 attachment 尚未决定” |
| 0030 | 保留 q v0 Skills-only，明确未来 executable export 不属于现有安装能力 |
| 0032–0040 | 保留 Host lifetime、command plane、Process launch context、durable owner、重启新 PID；补 Shell 实执行缺口及终端断连语义 |
| 0041 | 标记 embedded bootstrap 现状与 system-package 目标之间的未实现部分 |
| 0042–0043 | 保留 readiness/restart budget，不让 Herdr agent status 替代服务健康 |
| 0044–0049 | 保留独立 Host / aP / reference+offset / detach / Shell / Local Entry；移除 Apple 默认消费者，解决 renderer launch 特权方案与 bounded entry 差距 |
| 0050–0051 | 保留 Host grants 与 profile/secret 分离；退役 GUI 后定义终端批准呈现，不转移安全 owner |
| 0052 | 保留无隐式 Host directory source，更新已过时的 package change blocked 句子 |
| 0053 | 保留 authored content 与 generated state 区分；退役迁移不得静默丢弃用户内容 |

编号存在历史空缺，例如 0031；不要凭编号补造缺失 ADR。建议只新增少量真正的新决策：客户端退役/终端边界、混合 Agent Machine 及其状态权威。其余通过关联说明与 OpenSpec 细化，避免每个模型适配器都增加 ADR。

## 7. OpenSpec 更新地图

| Capability 组 | 更新重点 |
| --- | --- |
| `agent-namespace-runtime`、`agent-file-layout-contract`、`agent-root-layout-contract` | 混合 transition、wait/complete/exit、write lease 不再仅限 generation；Machine/Tape/recovery 所有权 |
| `llm-file-server`、`provider-connection-contract`、`provider-request-controls`、`connection-service` | generation/evaluation 能力、typed request/result、连接配置与凭据复用；避免所有 request controls 被假定通用 |
| `evidence-retention-and-projection`、`content-addressed-knowledge`、`branching-execution-file-server` | 判别证据、checkpoint 完整性、版本与输入 digest；分支选中不等于外部效果提交 |
| `runtime-memory-contract`、`runtime-memory-surfaces` | System Store、Store 提交、promotion 与 feedback 边界 |
| `auto-approve-policy`、`sandbox-autonomy-invariants`、`autonomous-review-mode` | 保留 deterministic gates；评价不可用/不确定不自动放行；治理范围不扩大成人类所有操作 |
| `alan-shell`、`rust-inline-tui`、`local-entry-service`、`process-launch-context` | 裸 alan 默认入口、真实 Shell evaluator、runner wiring、实时 IO、bounded entry |
| `alan-renderer-host-contract`、`agent-runtime-ui-file-surfaces`、`tool-result-presentation` | 终端 renderer、结构化结果呈现、输入/取消、断连与结果重读；不强迫默认入口直接打开 root Agent |
| `alan-os-host-lifecycle`、`local-alan-os-attachment`、`ap-wire-transport` | 保留非 GUI Host 和 aP；明确 Herdr 只在字节终端层，不要求其直接实现协议 |
| `package-management-contract`、`skill-system-contract` | `/srv` vs `/mnt` 修正；distribution/Skill 区别；安装/授权/选择/执行分开 |
| `service-manager`、`alan-os-system-store` | 明确 boot 内容现状；包、证据、记忆各自 durable owner |
| 19 个 `macos-*` capability | 随客户端退役逐条评估并通过正式 REMOVED delta 撤销 App 义务；安全/平台能力先迁至仍存活的 owner，不按前缀批删 |
| `shell-core-authority-contract`、`shell-workspace-core-contract` | 若无存活消费者，随自建 workspace/tab/pane domain 一起退役，而不是转移到 Herdr 内重新实现 |
| `alan-app-distribution`、`product-brand-identity` | 从 App bundle/cask/Sparkle 改为仍支持的 CLI/Host 分发与产品定位，保留必要兼容/迁移说明 |
| `repository-quality-gate`、`governance-tooling-contract`、`documentation-governance` | 清理 Apple-only 门禁与过时 guard，保留 Rust macOS 测试和安全检查；退役规范与文档一致 |
| `message-routing`、`plan9-kernel-substrate` 等未受影响底座 | 原则上保留；routefs 负责确定性组合，不变成概率策略引擎 |

## 8. 全部 13 个活动 change 的处置建议

任务勾选数是文档状态，不是完成证明。退役不是“全部打勾再 archive”。取消的 change 应明确原因和被取代关系，不把未实现 delta 同步成 canonical。

| Change | 当前任务 | 建议 |
| --- | --- | --- |
| `add-cognitive-model-routing` | 0/20 | 全面重写，承载混合 Machine / evaluation，不再执行原 S1/S2 Process 路由方案 |
| `expose-agent-rollout-history` | 0/26 | 保留 durable history 价值；拆掉 renderer launch 特权，转向终端可发现/可恢复证据 |
| `define-alan-interaction-model` | 0/18 | 改为 Herdr 内 Alan 交互；保留渐进披露与结果审阅，删除 native workspace/UI 义务及隐式 launch 例外 |
| `verify-macos-managed-user-pty` | 0/9 | 客户端 PTY 验证目标取消；若承担安全执行前提，迁入 OS sandbox 验证，不继续为旧 UI 建账户 |
| `define-updf-product-umbrella` | 0/20 | 暂缓；撤销 Alan for macOS preview 假设，包格式/domain 与具体 viewer 分离，不作为核心认知前置 |
| `define-groove-master-alan-app` | 0/19 | 暂缓并重定消费者；独立 domain/file-server 思想可保留，但不能再计划 Alan for macOS native client |
| `define-alan-programmable-client-surface` | 0/33 | 保留 shared grammar、增量 IO 与 package/binfs 边界；拆出可先交付的 IO/evaluator slice，不让整个 Acme 可编程 UI 阻塞 Herdr 首版 |
| `add-macos-shell-component-system` | 1/23 | 取消，不继续为退役客户端建组件系统 |
| `add-alan-voice-mvp` | 0/18 | 当前 macOS Hold-to-Talk UI 方案停止；仅在真实终端/外部输入需求出现后重提 Voice Service adapter，Jev 不替代 ASR |
| `spike-macos-matter-controller` | 0/17 | 现有 App-hosted spike 暂停；有具体硬件需求再作为平台 adapter 独立验证，不与此次核心重构绑定 |
| `add-proactive-memory-v2` | 0/18 | 先修旧存储/命令面与 schema 失败；再依赖新的 evidence/Machine 合约，不先做自动策略学习 |
| `add-alan-anywhere-mvp` | 0/23 | 重评必要性，取消 macOS enrollment/UI 依赖；先用现有终端接入满足需求，不默认继续建设账号/设备目录/远程产品 |
| `add-macos-app-auto-update` | 27/32 | 停止剩余 App 发布验证，按退役收尾处理；已有 Sparkle/发布 wiring 是删除审计范围，不宣称发布链已完整验收 |

其中 component-system 的基线也已陈旧：proposal 声称 token guard 尚未进 CI，但 `.github/workflows/ci.yml:69–76` 已运行该检查。此类漂移无需再为旧 UI 单独修复，随退役收口。

## 9. 其他文档与开发工作流

| 文件/目录 | 应更新内容 |
| --- | --- |
| `AGENTS.md`、`CLAUDE.md` | 新组件定位、混合 Machine、终端首选宿主；移除未来实现仍必须验证 Alan Dev.app 的常规指令，保留历史客户端维护才适用的限定 |
| `README.md`、`CONTEXT.md`、`docs/architecture.md` | 产品入口、真实启动路径、Host 与终端宿主区别；去掉“macOS attachment 尚未决定”等旧句；q v0、运行实现 vs 目标分开 |
| `docs/README.md`、`docs/agents/domain.md` | 当前导航与词汇；不把 retired App 文档继续列为开发主路径 |
| `docs/skills_and_tools.md`、`docs/skill_authoring.md` | 明确 Skill/package/Tool、包内 bin 资产与执行权限；候选选择可用 evaluator 但 exposure 不变 |
| `docs/testing_strategy.md`、`docs/live_runtime_smoke.md`、`docs/live_provider_harness.md` | 终端/Host 验收、typed evaluation fixtures、effect recovery；不把主流程限定为文本生成 |
| `docs/harness/` | 在已有 harness 增补评价失败/升级/无生成成功/断连恢复；不要只衡量 TTFT，需同时量测正确动作率、误执行、升级率、端到端延迟与成本 |
| `clients/apple/README`、`ARCHITECTURE`、macOS runbooks、`docs/design/design-language.md` | 随退役删除或留必要迁移指针；不要在现行设计指南保留新的 native UI 义务 |
| `justfile`、CI/release workflows、scripts、Cargo workspace、开发 skills | 删除 Apple-only build/release/FFI 使用路径前做消费者核查；保留非 GUI Rust macOS 质量门禁；不要只删源目录造成构建悬空 |

文档债不全由停滞造成：同时存在“目标写成现状”“实现完成但路线图未更新”“活动 change 仍基于旧架构”三类问题。更新时应标记 implemented / specified-but-missing / proposed / retired，而不是继续写一个笼统的 current。

## 10. 实际运行的检查

| 检查 | 结果 | 解释 |
| --- | --- | --- |
| `openspec validate --all --strict` | 86 通过、2 失败，共 88 项 | 当前规范集合并非全绿；不是本文新增行为引起 |
| `bash scripts/check-openspec-current-surfaces.sh` | 通过 | 模式检查通过不代表语义一致 |
| `bash scripts/check-rust-inline-tui-contract.sh` | 失败 | `ShellDesignTokens.swift` 的 Paper & Ink / Ink domain 注释被宽泛 `Ink` 规则误判成旧 JS 库 |
| Herdr 环境与帮助 | `HERDR_ENV=1`，0.9.1 | 只读；当前支持 kind 未列 Alan，无端到端集成证明 |

两个严格校验失败的具体位置：

1. `add-proactive-memory-v2` 对 `Runtime validates model-mediated memory write plans` 的 MODIFIED delta 遗漏 canonical 场景 `User asks alan to remember a stable preference`。
2. `define-alan-programmable-client-surface` 对 `Executable text uses explicit control operations` 与 `Buffer activity is observable as events` 的 MODIFIED delta 分别遗漏 `Partial control writes do not execute`、`Execution event is observed`。

MODIFIED 是完整替换 requirement，不能只写新场景而无意丢掉旧约束。重写时应明确保留或按合法 delta 机制删除，不以修校验为理由随意取消行为。

另静态发现 inline TUI guard 仍要求 main 中出现 `alan_tui::run`，而当前入口为 StdioDriver；本次执行先在 Ink 误报处失败，未运行到该后续检查。应重建与新终端入口一致的行为检查，不仅修改匹配字符串。`just verify` 当前确实存在，不应误报为无效命令。

## 11. 建议交付顺序与完成标准

### 阶段 A：先撤掉不再需要的产品义务

建立客户端退役 change：明确 Herdr/终端边界，撤销 Apple UI 与 App 发布义务，逐项登记需保留的平台安全能力和用户数据。同步 ADR supersession、canonical deltas、活动 backlog 与开发入口文档。

完成标准：没有活动计划继续要求建设 Alan for macOS；不会因删除 UI 丢失 Host/secret/sandbox owner；移除后的 workspace、CI、安装路径仍自洽。代码清理另按批准范围实施。

### 阶段 B：定案 Machine 与证据，而非立即接 API

重写 cognition change；确定 generation/evaluation DTO、状态/恢复权威、操作完成语义、write lease、fallback 与预算。复用现有 Machine/rollout/Connection，不新增平行执行框架。

完成标准：确定性成功、evaluation 成功、无匹配升级、评价失败、等待恢复、结构化完成、取消、Unknown effect 恢复都能在同一状态模型解释；同一次选择不能扩张权限。

### 阶段 C：修通真实 Shell 与终端数据路径

补 evaluator、runner、Process incremental output、Local Entry bounded lifecycle；统一裸 `alan` 入口。对旧 programmable-client change 做最小纵向切片，不先完成整套 editfs UI/脚本语言/通用包管理器。

完成标准：从终端输入经真实 Shell Process 启动受控工作；输出实时来自 Process 文件；关闭 pane 不错误关闭 Host；显式取消与 detach 可区分；重新 attach 不复制执行。

### 阶段 D：做一个受限 Jev 闭环和 Herdr 体验闭环

选择一个已有、低风险且有正确性判据的任务：明确候选 → typed evaluation → 既有 capability/action → 结果验证 → durable evidence。先 shadow 测量再启用自动选择；fallback 到 generation 必须有界。

同时在 Herdr 验证 normal-buffer/inline 的读取与交互；普通终端兼容完成后，再决定是否需要原生 Alan kind / hooks。不要将 Herdr 插件开发变成 Alan 可运行的前置条件。

完成标准：比较规则基线、仅生成基线与混合模式的正确动作率、错误自动执行率、升级率、p50/p95 端到端延迟与成本；只凭模型公布的推理延迟不足以通过。

### 阶段 E：按实际需求扩展，不恢复原来的全线并行

在前述链路稳定后，再决定 memory promotion、standing routines、q executable export/binfs、remote、UPDF/Groove/Matter。它们各有独立价值，但不是证明混合 Machine 或终端产品成立的必要前提。

最终收敛目标不是“Alan 再实现一个 Herdr”，也不是“把所有动作交给 Jev”，而是：**Herdr 提供成熟终端宿主；Alan 提供文件原生、可组合、可恢复且受授权约束的智能计算能力。**
