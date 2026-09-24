# Jev 与 fx 调研：Alan 的 System 1 与 Shell 交互方案

调研日期：2026-09-19。

后续范围更新：用户已决定退役 Alan for macOS，改以 Herdr 为首选终端宿主。本文中继续建设自有 macOS 客户端的建议不再适用；最新产品边界、全库审查和更新顺序见 [整体架构审查](architecture-review.md)。Jev/fx 的技术调研结论与该产品决定分开看待。

状态：研究与建议，不是已接受的规范变更，也不是实现完成声明。根据用户对重构范围的澄清，本文已由兼容式 Jev 接入方案修订为 Agent Machine 认知架构重构方向。本文放在既有 cognitive routing change 下；正式实施前应整体替换该 change 的旧模型路由设计，并协调相关 Shell 与 interaction deltas。

证据基线：Alan 本地 `576fb4752e098e93f076c10f9daba1d200cc3b7a`；fx 上游 `6a0a1ff186c7b74a28d98418b793f95a3864fd3b`。初始 Alan 工作树干净。本次核查官方文档、公开源码和本地调用链，未调用收费模型、读取凭据、安装 fx 或运行其界面；延迟、中文质量、视觉体验均未实测。

## 1. 建议决策

**将 Alan 的 Agent Machine 从生成模型驱动的 Tool loop，重构为由事件驱动、可以组合确定性代码、判别模型和生成模型的状态机。Jev 是第一种判别模型实现；Shell 是观察、发起和干预工作的表面。**

用户原先的方向有价值，两个新发现使落点更精确：

1. Jev 不是“小而快的聊天模型”。它接收状态和预定义问题，返回类型化选择、评分、概率，不生成回答、patch 或任意 Tool 参数。因此，不能直接替换 Alan 原设计中的生成型 System 1 Agent Process。[TypeSafe introduction](https://docs.typesafe.ai/introduction)
2. fx 已经集成 Jev，用途是可选的 Tool action 自动审查。其交互是 normal-buffer 中的 Agent CLI，普通输入进入 Agent，并非自动辨认命令与自然语言的完整 Shell。[fx Jev adapter](https://github.com/vercel-labs/fx/blob/6a0a1ff186c7b74a28d98418b793f95a3864fd3b/src/builtins/gateway/typesafe_permission_reviewer.zig)、[输入提交](https://github.com/vercel-labs/fx/blob/6a0a1ff186c7b74a28d98418b793f95a3864fd3b/src/core/app/input_submit_runtime.zig#L706)

关键调整不是增加一个入口分类器，而是取消“认知步骤必然生成文本”“System 1/2 各等于一个 Agent”“每个任务必须先聊天”的前提。Process、namespace 和文件所有权仍有独立价值；生成优先的 Machine、Tape 投影、模型操作和完成条件则应重新设计。实现按垂直场景分步迁移，不保留两套长期运行的认知权威。

## 2. Jev 到底提供什么

### 2.1 已核实的 API 和运营参数

| 项目 | 当前事实 | 对 Alan 的意义 |
| --- | --- | --- |
| 提供方 | TypeSafe AI；2026-09-15 发布 early access | 接入资格和配额须在实际账号上确认 |
| HTTP | `POST https://api.typesafe.ai/v1/systemone`，Bearer key | 独立 evaluation 协议，不是 Chat Completions |
| 请求 | `model`、`state`、`questions` | 传程序状态与问题，不传聊天生成请求 |
| 响应 | `answers`、实际 `model`、token `usage` | 保留类型、概率、模型版本和计量 |
| 版本 | `jev-1.13.0`；当前 latest/preview 均指向它 | 校准后固定版本，升级重新评估 |
| 输入 | 仅文本，可用 string/object/array 表示 | 图片和音频需要上游处理 |
| 上下文 | 整体 64k；state + 最长问题另受 32k 限制 | 不能把完整 Tape 无限制送进去 |
| 价格 | 输入 $0.042/百万 tokens，输出免费 | 适合短上下文高频判断 |
| 限流 | 当前 250,000 tokens/s、1,200 requests/min，官方声明动态调整 | 需要超时、限流回退，不能假设恒定容量 |
| 语言 | 英语最佳，中文等非英语质量不均 | 中文和中英混合必须单独验收 |
| 客户微调 | 不提供客户 fine-tune/LoRA | 通过候选、条件与上下文设计适配 |

来源：[发布文章](https://typesafe.ai/blog/introducing-system-one-models-and-jev)、[HTTP API](https://docs.typesafe.ai/api)、[Models](https://docs.typesafe.ai/models)。参数是调研时快照，非长期保证。

三种原语承担不同任务：

| 原语 | 输出 | Alan 中合适的用途 |
| --- | --- | --- |
| Choice | 一个候选、候选概率分布、confidence | 从可见 Skill、处理路径或已有动作候选中选择 |
| Score | 按语义等级得到的评分、分布、confidence | 对检索结果相关性、响应是否符合目标进行排序 |
| Noul | 某个明确命题成立的概率，0–1 | 判断是否缺信息、某候选是否适用 |

Choice 最多 255 个候选，Score 使用 2–10 个等级。Noul 没有单独 confidence。一次可对同一 state 提多个独立问题，但问题之间不会互相推理，也不自动保证逻辑一致性。[Choice](https://docs.typesafe.ai/primitives/choice)、[Score](https://docs.typesafe.ai/primitives/score)、[Noul](https://docs.typesafe.ai/primitives/noul)、[Confidence](https://docs.typesafe.ai/confidence)

公开 HTTP 合约没有 token streaming/SSE 接口，首版应按完整 JSON 结果处理。官方 function-calling 示例实质是选择预先枚举的函数与参数，再由代码执行；这不等于生成任意命令、文件路径或 JSON 参数。[HTTP API](https://docs.typesafe.ai/api)、[Function calling cookbook](https://docs.typesafe.ai/cookbooks/function_calling)

### 2.2 哪些宣传不能直接作为采用依据

官方报告 70–500ms 延迟，并注明常在服务所在的美国西海岸发起测试；不能推导出上海网络下的 p95。首页的大幅速度/成本提升来自特定 workflow 对比，厂商也承认有选题偏差和收益上界因素。结构匹配保证不能推导为语义判断零错误。[发布文章](https://typesafe.ai/blog/introducing-system-one-models-and-jev)

`confidence` 是从输出概率分布提炼的统计量，不应解释成某个动作具有相同数值的正确率，更不能解释成授权。候选集缺少正确答案时，也可能得到集中的错误选择。应提供明确的 `unknown`/`none`/`escalate` 出口，并在 Alan 任务上校准。[Confidence](https://docs.typesafe.ai/confidence)

官方已知问题包括：计数、数值、日期比较、间接推理、字面理解、无关长上下文，以及对抗性 state。尤其明确承认 state 不默认被视为敌对内容，注入可以改变判断。因此 Jev 适合提供信号，不适合成为权限根。[Jev 1.13 jaggedness](https://docs.typesafe.ai/model-jaggedness/jev-1.13)

### 2.3 采用顺序

| 用途 | 建议 | 原因 |
| --- | --- | --- |
| 对有限路由候选做判断 | 首个用例 | 闭集、短上下文、可与当前路线对比 |
| Skill/Tool 候选排序 | 第二个用例 | 仅在当前 namespace 可见候选内排序 |
| Memory/RAG 候选相关性筛选 | 有真实检索问题后再做 | 不需要提前给全部记忆加标签 |
| 生成型 fast attempt 的结果检查 | 后续实验 | 判断器与生成器可能共同出错，需要端到端验证 |
| 自然语言生成、代码修改、任意命令生成 | 交给生成模型 | Jev 没有相应生成能力 |
| 精确计数、路径解析、日期运算 | 代码完成 | 模型增加延迟且降低确定性 |
| 放行未知副作用或替代用户授权 | 不采用 | 类型化判断不提供执行权 |

官方的 [intent routing](https://docs.typesafe.ai/patterns/intent-routing)、[skill suggestion](https://docs.typesafe.ai/cookbooks/skill_suggestion)、[SDE cascade](https://docs.typesafe.ai/cookbooks/sde_cascade) 都能支持上述组合方向；示例不构成 Alan 效果证明。

## 3. Alan 当前状态：哪些已有，哪些只是设计

以下以固定 Alan SHA 的代码为依据；链接相对本文所在目录。

| 层 | 当前证据 | 结论 |
| --- | --- | --- |
| Cognitive routing | [design](design.md)、[tasks](tasks.md)，任务全部未勾选；engine/AgentFS 未找到 `route next` 实现 | 设计完整，不能声称路由已运行 |
| Provider | [LlmProvider](../../../crates/llm/src/provider.rs) 提供 generate/chat/generate_stream | 生成型接口不适合直接承载 Jev |
| llmfs | [request_wire.rs](../../../crates/llmfs/src/request_wire.rs) 的 v2 请求要求非空 messages | 当前文件协议也不是 evaluation 协议 |
| Namespace generation | [namespace_generation.rs](../../../crates/agent-engine/src/runtime/transition/turn_execution/namespace_generation.rs) | 已有文件调用边界；不可在 coordinator 中绕过它直连 HTTP |
| Connection/凭据 | [connection_profile.rs](../../../crates/service-manager/src/connection_profile.rs)、[connection CLI](../../../crates/alan/src/cli/connection.rs) | 可复用 profile/Host Store，但需要新增 evaluation 能力声明 |
| 裸 `alan` | [main.rs](../../../crates/alan/src/main.rs) 的无子命令分支走 `StdioDriver` | 实际入口目前是通用文件 Shell |
| Shell Process | [local_entry.rs](../../../crates/service-manager/src/local_entry.rs) | attach 创建普通 Shell Process，保留这个生命周期 |
| Shell 语法 | [shell/lib.rs](../../../crates/shell/src/lib.rs) | 支持文件操作和 `/bin` executable；不是完整 POSIX shell |
| 富 TUI | [file_backed.rs](../../../crates/tui/src/file_backed.rs)、[app.rs](../../../crates/tui/src/file_backed/app.rs) | 已有 AgentFS 观察、composer、历史、action、流输出协调；未看到裸入口调用它 |
| 滚屏 | [terminal.rs](../../../crates/tui/src/terminal.rs)、`drain_committed_scrollback` | 已有写回滚屏机制，不能把 Alan 描述成完全缺少 inline 基础 |
| 可编程表面 | [已交付 tracer bullet](../archive/2026-09-24-define-alan-programmable-client-surface/design.md) | 裸 alan 已接入现有 Root Agent/TUI 与 one-shot；后续 rich UX 和重连验收走 interaction change，不恢复旧 parser/executor/editfs 方案 |

AGENTS.md 的概览称当前交互产品路径为 Rust TUI，但当前 `main.rs` 的实际裸入口是 stdio Shell。方案需以调用链为准，避免只修改 TUI 却没有改善用户真正进入的界面。

另外，`alan-routefs` 当前是确定性类型消息分发，不是模型选路器。不要因名字相似就把 Jev 判断塞进去。[routefs](../../../crates/routefs/src/lib.rs)

## 4. 重构目标：事件驱动的混合认知 Agent Machine

### 4.1 旧设计限制在哪里

当前 `turn_execution.rs` 的主路径组装 GenerationRequest、调用生成 Connection、消费 assistant/Tool calls，再决定下一轮。这使很多工作必须先变成 prompt，再等模型生成控制意图。

原 cognitive-routing design 在这个循环外再加快/慢 Agent，本质上没有改变生成模型对控制流程的支配。Jev 带来的机会是：对已有状态的很多判断，本来就不需要生成一句话，也不需要先有一份聊天历史。

| 原假设 | 新目标 |
| --- | --- |
| System 1 是便宜/快速生成模型，System 2 是深度生成模型 | System 1 是受约束的快速判断与响应方式；System 2 是开放式推理、构造和修正方式 |
| 一次认知步骤约等于一次生成调用 | 一次状态转移可能只用代码、一次 evaluation、一次 generation，或消费一个外部结果 |
| 快/慢两次尝试分别必为子 Agent Process | 同一 Agent Machine 可以切换认知方式；隔离、委派和独立任务才决定是否 spawn |
| 每轮先组装聊天上下文 | 先读当前任务所需状态；消息只是输入和证据的一种 |
| System 1 天然只读，System 2 才能执行变化 | 认知方式与执行权正交；任何副作用都走同一授权机制 |
| 无 Tool call + final text 表示工作完成 | 完成取决于本次任务约定的结果与证据；可以是文本、文件、类型化结果或动作完成 |
| 系统先决定用哪个模型 | 系统先决定需要判断、生成、执行还是等待，再选择可用 Connection |

表中是建议替换的假设，不是对当前规范已经生效的声明。尤其 AGENTS.md 的“Transition function = LLM generation”和“无 Tool calls 即 halt”需要与正式新合约一起修订。

### 4.2 一个 Machine，几种可组合的转移

建议让一个 Agent Process 继续拥有一个 Agent Machine。下面是概念模型，不要求逐项创建新类、服务或 crate：

```text
事件 → 当前 Machine 状态 + 必要的 namespace 视图
                     │
              确定性转移逻辑
              ┌──────┼────────┬────────┬────────┐
              ▼      ▼        ▼        ▼        ▼
            判断    生成     执行     等待     完成
           Jev等   生成模型  Tool/文件 事件/审批  发布结果
              └──────┴────────┴────────┘
                     │
              类型化结果 / 证据
                     └────────→ 下一次状态转移
```

“判断”回答一个已定义的问题；“生成”构造当前候选集之外的新内容或方案；“执行”应用当前已授权的动作；“等待”释放执行资源但保留明确待续条件；“完成”结束当前工作，不等于结束长存 Process。

确定性转移逻辑属于 Agent Execution Engine。模型返回建议或内容，不直接转移 Kernel 状态。这里也不应出现另一个全局 Cognitive Manager。

不要求所有事件先过 Jev。例如一个 Tool 已返回明确成功码，且任务完成条件已经满足，代码可以直接结束。也不要求每个生成结果再付费调用 Jev 检查；只有确实需要语义判断的地方才调用。

### 4.3 System 1 与 System 2 的关系改为局部协作

System 1 不再只是入口的“分诊台”，它可以出现在整个工作过程中：

- 开始时，从明确候选中选择适用能力或响应路径。
- 执行中，对短小 Tool 结果做语义判断，决定继续、补信息或进入深度推理。
- 结果到达时，判断是否满足一个不能用代码精确验证的条件。
- 用户打断时，判断新的内容是否需要修改当前目标；取消等显式控制仍直接由代码处理。

System 2 负责构造 System 1 无法在当前候选空间中表达的内容：开放式诊断、代码、计划、解释、任意参数。它产生的结果经验证后可以回到快速执行路径，形成“判断 → 深思 → 执行 → 判断”的局部循环，不是单向升级后永远留在深层模型。

候选不足、状态缺失与复杂推理要分开。缺少已授权文件时先读取；候选无法表达任务时才需要生成；输入真实歧义且影响执行时才需要用户澄清。不要把所有不确定性都变成一次大模型调用，也不要把所有小任务都先额外分类。

这是行为分工，不是为每个 Agent 强制配置两个人格。Jev 加当前生成 Connection 即可支持第一版；是否再加 Flash 属于生成能力的成本优化，与 System 1 的定义无关。

### 4.4 能力、授权与 Process 边界

Jev 的 Choice 可以选择当前可见、适用的候选；候选来自显式任务定义、已安装 Tool/Skill 及已验证的生成结果。namespace 中所有可见命令并不自动成为此次任务获准执行的候选。

无论由 Jev、生成模型还是确定性代码提出动作，都进入同一 action resolution、authorization 和 Tool Process 路径。当前存在的 `resolve_tool_call`、`authorize_tool_call`、`execute_allowed_tool_call` 等职责应被复用或演进，不再以“是否来自 LLM tool_call”作为执行入口前提。[transition.rs](../../../crates/agent-engine/src/runtime/transition.rs)

由此可以支持**获得明确授权的 System 1 写操作**，例如用户预先授权的有限规则处理；也允许 System 2 只读研究。是否允许写入取决于作用域和授权，不能取决于模型大小或判断置信度。首个实现用只读场景验收，是交付次序，不是 System 1 的永久架构限制。

同一 Machine 内进行 evaluate/generate 切换，并不改变它的 namespace 或恢复以前没有的权限。需要不可信方案的执行隔离、不同凭据或真正的委派时，使用普通子 Process；旧设计的只读投机 attempt 仍是可用隔离模式，但不再定义所有 System 1。

任务取消、输入纠正和结果消费使用 Machine 当前输入/状态版本关联。迟到结果不可作用于更新后的任务；执行前重新检查候选绑定、资源版本与当前授权，不能复用陈旧的“当时允许”。

### 4.5 模型层：先有操作，再有模型

当前 `LlmProvider` 的 generate/chat/generate_stream 以及 llmfs Generation v2 不是中性的“认知能力接口”。此次重构应明确支持两种语义，而不是把评价 JSON 包装成 assistant text：

| 操作 | 请求 | 结果 |
| --- | --- | --- |
| evaluate | 有限 state、类型化问题/候选、预算 | 类型化判断、分布、版本、usage |
| generate | 指令、消息/内容投影、允许的输出与工具能力、预算 | 文本、结构化建议、Tool action proposals、usage |

操作通过现有可挂载 Connection 的文件生命周期暴露；复用 `clone/data/events/status/ctl` 的可观察和取消机制。是否调整 Generation 的命名和 wire 版本，应由 `llm-file-server` delta 明确决定，而非为迁就旧名字模糊语义。不为同一个厂商另建一个全局常驻服务。

TypeSafe HTTP 投影放在 adapter，Connection 管凭据与可用能力，Machine 管为何调用以及如何消费结果。Jev 不被要求实现虚假的 chat/streaming/Tool generation 方法；生成 provider 也不默认声称具有经过验证的 evaluation 能力。

首个端到端场景只需要 Choice 就先实现 Choice；Score/Noul 是已知扩展点，不需要提前建立通用算子图、插件框架或 workflow DSL。概念上允许模型多样性，不等于实现时造一个包罗万象的接口。

### 4.6 状态、Tape 与证据重新分工

当前 AgentMachine 中 Tape 以消息、摘要、上下文为中心，而转移局部状态已有独立 owner。这给重构提供了支点。[agent_machine.rs](../../../crates/agent-engine/src/agent_machine.rs)

建议采用下列分工：

- **Machine 状态**：当前工作目标、待解决问题、合法候选、等待条件、预算、当前操作引用。放在原有 Machine owner 内，不增加全局 Task/Conversation 对象。
- **Tape**：演进成可表示已接受输入、类型化判别结果、生成内容、动作引用与结果的有序记录；是否用新增记录变体实现，在正式 spec 中定稿。不得把判别结果伪造为 assistant 对话。
- **模型上下文**：从当前状态、所需文件和相关 Tape 记录构造的有界投影。Jev 只看当前问题所需 state，生成模型也不必每次重发全部执行证据。
- **rollout/checkpoint**：持久化转移证据与恢复所需状态；不是再建立一套并列 event store。
- **Memory Stores**：跨 Process 的长期知识。一次概率判断不会自动变成长期事实或已学习技能。

恢复时使用已记录的模型结果恢复原有转移，而不是重新调用随机模型期待同一个判断。对副作用沿用既有幂等/结果核查：结果不明时记录不确定性，不能通过“重想一次”自动重放。

初期反馈只用来评估和调整版本化问题/策略。若以后把 System 2 完成的过程整理为复用能力，应走现有 Skill/Tool 包的显式验证与安装路径。一次成功不等于可以自动生成永久规则，更不在这里承诺在线训练、蒸馏或无限自修改。

### 4.7 不只面对聊天输入

已有 interaction proposal 已提出 conversation、background servant、event-driven 三种模式，但事件触发 runtime 尚无完整 owner。[interaction design](../define-alan-interaction-model/design.md)

因此统一的 Machine 输入可以包括用户输入、Tool/Process 完成、审批结果和显式订阅的服务事件。它们是类型化输入，不需要服务先写成一句“用户说……”来启动生成模型。

有界要求仍需明确：事件来源可见且获授权、去重与恢复游标、并发预算、事件风暴合并以及取消订阅。第一版使用已有 Tool 完成和输入事件证明共同转移机制；不要顺带实现全系统 scheduler 或观察所有文件的常驻 AI。

### 4.8 一个完整例子：从认知架构看交互

用户明确授权一个后台任务：“有新报告到指定目录后，判断是否是供应商报价；是的话提取关键条款供我审核，不要回复供应商。”

1. 显式 watcher/触发能力存在并获授权时，将新增文件事件交给负责此工作的 Agent Process；没有这个能力时不能假称当前 Alan 已支持。
2. 代码验证文件、重复事件和输入范围；文档转为受限文本视图。
3. Jev 判断是否属于报价，返回已定义标签与概率；类型不明则记录待处理或进入生成分析。
4. 开放式条款提取需要生成模型；它取得必要文件和任务约束，返回结果。
5. 代码校验可精确验证的字段，必要时再做少量语义评估。
6. 结果文件和完成证据发布后，Shell 显示“报价已整理，待查看”，无需额外调用模型编写完成套话。
7. 发邮件不是此次授权的一部分，任一模型都不能据此自行执行。

这个场景包含快判断、深处理、文件输出、异步等待和 Shell 结果呈现，但不需要五个彼此对话的 Agent。它是目标架构示例，不是首期交付所有依赖的承诺。首期可用“用户显式提交一个已挂载文本文件”替代 watcher，验证完全相同的核心认知路径。

### 4.9 要替换的设计与保留的基础

**整体替换**现有 `add-cognitive-model-routing` 中“两个模型角色 + 默认子 Agent 尝试 + 单向升级”的中心设计。正式 proposal 应围绕 Agent Machine 的混合转移契约命名与组织；旧 change 必须显式 supersede/fold，不能两个方案都保持待实施权威。

配套修订：

1. Agent Machine/namespace runtime：类型化输入与操作、等待/完成语义、共同 action pipeline。
2. llmfs/provider connection：evaluate/generate、能力发现、类型化结果和操作终态。
3. Tape/rollout/checkpoint：非消息判别记录、恢复与效果去重。
4. AgentFS：Machine 当前操作及证据的只读投影；控制仍归 `machine/ctl`，不为每类认知增加 ctl。
5. interaction/Shell：工作流的发起、输出、暂停与干预，不强制聊天呈现。
6. AGENTS.md 的 AI Turing Machine 对照表：转移函数不再等同单一 LLM generation；文本 final 不再是唯一停机结果。

**继续保留**Process 身份与生命周期、Kernel 最小依赖、namespace 能力边界、AgentFS IO、Memory Stores，以及 provider/Host adapters。这些并不是“只有生成模型”导致的限制。

是否把 `machine/routing` 扩展为已有 machine 状态中的 decision/operation 投影，要在正式合约中选一个 owner。不要一边保留旧 routing 状态，一边增加第二套全局 cognition 状态。

## 5. fx 值得学什么

### 5.1 用户体验与输入语义

fx 默认在普通 terminal buffer 中呈现顺序输出，保留终端自己的滚屏。composer 与简短状态在活动区域，完整 transcript、文件审批和目录选择使用临时深入界面。readline 风格编辑、粘贴、历史和运行中追加输入使它接近日常命令行。[terminal ownership](https://github.com/vercel-labs/fx/blob/6a0a1ff186c7b74a28d98418b793f95a3864fd3b/src/ui/shell_runtime.zig#L57)、[shortcuts](https://github.com/vercel-labs/fx/blob/6a0a1ff186c7b74a28d98418b793f95a3864fd3b/src/ui/input/shortcuts.zig)

但它的普通输入仍是 Agent prompt，slash command 是 UI 控制。`Shift+Tab` 切权限模式，`Ctrl+Z` 挂起回宿主 Shell；不能误读为自然语言与命令之间的自动双模切换。运行时 Enter 可 steer，Esc/空草稿 Ctrl+C 取消，Ctrl+O 查看详情。[官方 quick start](https://fx.sh/docs)、[input runtime](https://github.com/vercel-labs/fx/blob/6a0a1ff186c7b74a28d98418b793f95a3864fd3b/src/core/app/app_input_runtime.zig)

其 headless `fx ask` 对重定向输出区分结果与诊断，这种 Unix 可组合性同样值得借鉴。不能只复刻一个长得像终端的聊天输入框。[command contracts](https://github.com/vercel-labs/fx/blob/6a0a1ff186c7b74a28d98418b793f95a3864fd3b/src/builtins/commands.zig)

### 5.2 技术与状态模型

fx 是 Zig 0.16+ 原生项目；另提供 ACP、`libfx`、Agent 和 Terminal 两种嵌入面，浏览器通过 WASM/JSPI 和终端组件运行。公开 README 将 WASM SDK 标为 experimental。[README](https://github.com/vercel-labs/fx/blob/6a0a1ff186c7b74a28d98418b793f95a3864fd3b/README.md)、[SDK](https://github.com/vercel-labs/fx/blob/6a0a1ff186c7b74a28d98418b793f95a3864fd3b/sdk/README.md)、[Terminal embedding](https://fx.sh/docs/lib/terminal)

fx 自有 session persistence、history、tool results 与子 session。它的 terminal contract 将进程 lifecycle、attention 和写入者分开，值得参考；这些类型不等于已经证明完整的人工接管体验。[session store](https://github.com/vercel-labs/fx/blob/6a0a1ff186c7b74a28d98418b793f95a3864fd3b/src/core/session/session_store.zig)、[terminal contracts](https://github.com/vercel-labs/fx/blob/6a0a1ff186c7b74a28d98418b793f95a3864fd3b/src/core/terminal/contracts.zig)

Alan 已有 Rust engine、AgentFS、Process 和 native terminal host。嵌入完整 libfx 会引入第二套状态与执行生命周期，也不能天然复用 Alan namespace。建议借交互和测试思路，不换 runtime、不加 Zig/JS 依赖。

fx 使用 Apache-2.0，复制代码时需遵守相应许可证和第三方声明；仅借鉴交互原则不需要引入其代码。[LICENSE](https://github.com/vercel-labs/fx/blob/6a0a1ff186c7b74a28d98418b793f95a3864fd3b/LICENSE)

### 5.3 fx 的 Jev 实现：可借鉴的边界和不能照抄的策略

调用路径是 reviewer 选择 → TypeSafe adapter → 既有 permission-decision consumer。直连发送 `state + questions.decision`，候选为 `clear/caution`；没有 TypeSafe key 时可走 Gateway。配置别名 `typesafeai/jev` 与 Gateway ID `typesafe-ai/jev` 不应混同为 Alan 已支持的 model id。[Jev adapter](https://github.com/vercel-labs/fx/blob/6a0a1ff186c7b74a28d98418b793f95a3864fd3b/src/builtins/gateway/typesafe_permission_reviewer.zig)

值得学的是 typed adapter 接既有消费者，失败保持 pending action 未执行；但该实现的概率与 confidence **只记录，不作为放行阈值**。其 prompt 主要识别具体恶意/注入行为，不把所有危险、外部、破坏性操作等价视为禁止。这是 fx 自己的产品 policy，不是 Alan 可直接继承的授权语义。同一源码的通用 reviewer 单次默认超时为 30 秒，并可对传输失败给一次新 deadline 的重试；这个预算也不适合照搬成 Shell 意图识别延迟。[auto classifier](https://github.com/vercel-labs/fx/blob/6a0a1ff186c7b74a28d98418b793f95a3864fd3b/src/core/permissions/auto_classifier.zig#L354)

此外 fx 的旧架构说明仍有 sandbox 描述，但 changelog 已移除内建 sandbox 配置，测试也把旧 sandbox key 当作无效旧数据。获准命令是普通 Host subprocess；permission reviewer 不能等同于 OS 隔离。[changelog](https://github.com/vercel-labs/fx/blob/6a0a1ff186c7b74a28d98418b793f95a3864fd3b/CHANGELOG.md#L325)、[配置测试](https://github.com/vercel-labs/fx/blob/6a0a1ff186c7b74a28d98418b793f95a3864fd3b/src/core/config/config_runtime.zig#L3332)

## 6. Alan 的 Shell 交互方案

### 6.1 产品行为

默认启动仍进入 Shell，Agent 是用户选择的执行与交互对象。视觉上统一成一个顺序工作流，输入区明确显示当前目标：Shell 或某个 Agent。目标选择属于 renderer 的文件投影，不给 `alan-shell` 核心加入 Agent 特殊模式。

以下为建议体验，不是已存在的命令语法：

```text
Alan   /mnt/project

Shell › ls /mnt/project
        Cargo.toml  crates/  openspec/

[用户选择一个 Agent Executable；产生普通 Agent Process]

Agent › 解释一下刚才失败的检查
        正在读取检查结果…
        ▸ 读取结果  完成
        失败原因是……

Agent › █
```

“Shell/Agent”目标始终可见但克制；PID、Connection、概率、请求 JSON 默认藏在详情。返回 Shell 只解除当前交互附着，不结束 Agent Process；取消本轮、终止 Process、清空画面分别表达，不能共用一个含混动作。[ADR-0039](../../../docs/adr/0039-alan-enters-shell-before-agent-views.md)、[ADR-0048](../../../docs/adr/0048-alan-shell-runs-as-an-ordinary-process.md)

首版不做“任意输入由 Jev 猜测是 Shell 还是聊天”。Shell 输入按确定性 grammar 处理，Agent 输入才可进入模型。未知 Shell 命令应给出错误/建议，不能自动转发 Host shell；生成命令若要执行，仍走明确的 Tool 与授权路径。

### 6.2 实现落点

1. **真实入口已交付。** 裸 alan 现在把 TTY 任务送到现有 Root Agent renderer，重定向 stdin 走 one-shot；`!` 只请求既有受治理的 bash Tool。不要实施旧建议中的 parser/executor 抽取；后续富呈现与可见重连验收按 [interaction change](../define-alan-interaction-model/tasks.md) 重规划。
2. **复用当前 TUI。** 保留 composer、history、completion、AgentFS watcher 与 `StreamReconciler`。把“已完成内容提交滚屏”和“可变活动区域重绘”分开；不为 fx 风格重写全部 renderer。
3. **保持唯一事实来源。** Shell command 结果读 Process streams；Agent 回复读 AgentFS，继续按权威记录消除预览重复；Tool 详情引用原有 action/result，显示折叠不删除证据。
4. **默认紧凑，细节按需。** 主流显示任务、短状态和结果；长输出预览可展开/按范围读取。计划、批准请求与错误仅在相关时出现。提供显式快捷键进入完整详情，再返回同一输入草稿。
5. **输入不中断执行。** 粘贴文本不自动提交；Ctrl+C 按当前目标产生明确取消语义；运行中新增输入的 steer/queue 行为由已有 runtime 合约决定，不只在 UI 改按钮。
6. **复用外部终端宿主。** Herdr 负责窗口、pane 与 PTY；Alan 通过普通终端输入输出工作。原生 macOS 客户端已退役，不再建设 sidebar、窗口或 Ghostty 集成。遵守 ADR-0054 的边界。

调研基线中的 `shell-core` 承担原生 shell workspace/pane 表面模型，现已随桌面源码删除，不是当前实现落点。历史证据固定到[调研提交的 shell-core](https://github.com/realmorrisliu/alan/blob/576fb4752e098e93f076c10f9daba1d200cc3b7a/crates/shell-core/src/lib.rs)；当前 Shell 实现以 `crates/shell/` 和活动 tracer-bullet 路线为准。

### 6.3 第一版验收

| 场景 | 应看到的行为 |
| --- | --- |
| 连续长输出 | 已完成内容保留在终端滚屏；输入草稿和光标稳定 |
| 中文、emoji、多行粘贴、resize | 不重影、不截断 grapheme，不误提交 |
| 输出流和 tape 交错到达 | 用户输入和回复各出现一次，顺序正确 |
| Tool 长输出 | 主流简短，详情可达，失败码和完整证据可查 |
| 取消当前生成 | UI 很快响应，晚到模型结果不覆写新任务 |
| 返回 Shell / 清屏 | 不隐式杀死后台 Process，不擦除执行证据 |
| 非交互输出 | 不混入光标控制序列；结果与诊断可分开消费 |
| 外部终端宿主 | 在普通终端和 Herdr pane 启动当前 Alan 构建，检查真实输出和输入行为 |

验证先用合成 Process streams，不依赖付费模型。终端自动化覆盖 resize/粘贴/恢复，真实终端任务使用当前构建回归；截图只能证明某个画面，不能替代滚屏和取消行为检查。

## 7. 分阶段实施与验收门槛

架构目标一次说清，实现用完整垂直切片证明。分阶段不是为了长期兼容旧认知架构，而是让每一步都可用、可验证、可删除被替代代码。

| 阶段 | 交付 | 验收后再推进 |
| --- | --- | --- |
| A：新认知合约 | 整体替换双模型 routing proposal，定义事件、操作、状态、效果与完成 | 相关 specs/AGENTS 一致；无并列认知权威 |
| B：共同转移骨架 | 将已有生成/Tool 行为迁入明确的状态转移，复用授权、取消与恢复 | 现有任务行为不退化；不保留永久 legacy loop |
| C：Jev 垂直场景 | 文件输入 → Choice → 确定性处理或生成 → 类型化结果 | 一项任务可不生成文本而完成；失败有终态与证据 |
| D：局部认知切换 | 同一工作内判断/生成/执行交替，支持用户纠正与等待 | 无强制 S1/S2 子 Agent；授权独立于模型，取消无迟到效果 |
| E：真实 Shell 表面 | 统一顺序工作流、滚屏/composer/详情、Process 控制和类型化结果 | 终端场景与 Alan Dev 新构建验证；不只修改未接入口的 TUI |
| F：有界事件工作 | 一个有明确 owner 的订阅/后台场景，及按需复用能力 | 去重、重启恢复、授权、预算；不引入万能 workflow 平台 |

影子评测用于校准具体判断，不作为旧架构永久存续的理由。B–D 优先交付一个真正跨越 Machine、模型和效果边界的用例；单独做一个 Jev HTTP demo 不算架构重构完成。

### 7.1 如何判断 Jev 是否真的值得

建议从 200–300 个去敏真实用例起步，分离调参集与验收集，保留中文、混合语言、否定、跨轮修正、目录外任务和不可信 Tool 输出。此规模能发现问题，不能证明极低事故率。

比较三个基线：现有路线、纯确定性处理、Jev 辅助路线。分别记录整体任务成功率、被接受子集的错误率、覆盖率、fallback/timeout 比例、从提交到可用结果的 p50/p95/p99。不要只测单次 API RTT 或只报平均准确率。

简单延迟估算：设判别耗时为 `J`，命中快速路径概率 `q`，快速执行耗时 `F`，原生成耗时 `G`，则 `T ≈ J + qF + (1-q)G`。相对原路线有收益需 `q(G-F) > J`。如果所有请求最后仍调用同一大模型，前置 Jev 通常只增加时延，除非它改善了上下文、工具选择或成功率。

按公开直连价格计算，2,000 输入 tokens/次约 $0.000084，10,000 次约 $0.84。这只是 evaluation 输入费用；总成本还包括新增问题、重试、生成调用和基础设施，按实际 `usage` 统计。同 state 多问题可合批，但额外问题不是免费 token。[Models](https://docs.typesafe.ai/models)、[fan-out](https://docs.typesafe.ai/patterns/fan-out)

### 7.2 上线前的实际条件

需要真实账号验证可用性、网络尾延迟、模型版本和配额。本次没有这些结果，不能声称已经“接上且可用”。Alan 的现有 [live provider harness](../../../docs/live_provider_harness.md) 面向生成/流式/continuation，evaluation 应有自己的 opt-in case，复用其隔离惯例，不能把 chat smoke test 当 Jev 验证。

TypeSafe 文档承诺不训练客户请求/响应，但 ZDR 为 enterprise 选项，并非所有请求默认零保留。对 Memory/仓库内容应采用最小 state 和明确数据范围。[Legal](https://docs.typesafe.ai/legal)

当前 MCA §2.3(f) 含公开 benchmark/performance information 的限制；内部产品验收与对外发布实测报告需要区分，发布前核对适用合同授权。本报告没有私有 API 实测数据。[MCA](https://typesafe.ai/legal/mca)

## 8. 最终取舍

此次重构的核心是：**Agent 是拥有状态、能力与生命周期的 Process；生成模型只是它推进工作的一种手段。** Jev 让判别成为可直接调用的认知操作，确定性代码仍掌握状态转移与执行约束，生成模型用于构造和复杂推理。

fx 为这个架构提供合适的交互参照：Shell 中连续可见的工作和结果，必要时进入对话或详情。完整认知循环可以没有聊天文本，复杂任务也可以随时转入深度生成。

本次已据此修订研究方案；现有 proposal/design/specs/tasks 和运行时代码尚未重写，因此不能把上述目标当作已接受或已实现行为。正式重构需要一份替代旧 cognitive-routing change 的完整 OpenSpec 设计，不能在原双模型路由任务表上直接继续实现。
