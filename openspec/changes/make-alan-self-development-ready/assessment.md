# Alan Shell 可用性、自举与终端设计评估

日期：2026-09-29。此文是当前 change 的非规范性证据与判断；验收合同见
`specs/`，推进顺序见 `tasks.md`。用户补充的两行 shell 图片已纳入设计。

## 最新状态 — 2026-10-01

受监督自开发已有 G1/G2 本地证据，仍未通过 G3，当前 WIP 尚未获得对应
提交的 CI/合并证据。不能把多次 Alan-authored 修改或绿色单元测试等同于
可重复、无人监督的自举；模型绑定和暂停目录切换仍在由两个 live Root 实现。
作者会在部分完成后结束一次输入，监督者需检查剩余工作并继续提交，不能
仅凭 UI 的 ready 状态推断开发任务完成。

相较 9 月 29 日基线，已复验的 Dev 预览支持真实模型与项目状态、`: ` / `! `
输入、Markdown/diff 层级、物理行有界的 Tool 摘要及 Ctrl+O 保留详情。
真实只读 fixture 的 CD/Bash 输出和详情可读，打开/返回详情保留未提交草稿。
这些证据来自预览候选，而非尚在变化的模型/目录控制源码。

slash 过滤时的闪烁已在共享整帧绘制入口修复。40/60/73/80/120 列原生逐键
测试确认同步更新边界；用户在 73×22 Herdr 预览中确认不再闪烁。用户随后
指出候选项出现在输入上方导致输入上下跳动；验收已改为候选项在 composer
下方，输入/光标锚点不随候选数量变化。这项布局修改尚未交付。

当前与 fx 的剩余体验差距主要是模型/状态操作的完整闭环、可靠排队/恢复
和暂停时授权，以及稳定的补全布局。此前 fx 0.0.11 的 Markdown 对比保留
为历史基线；未完成当前候选与 fx 在相同宽度/主题下的完整复验，不能宣称
视觉或交互已经全面追平。详细可复查的冻结范围、日志与验收边界见 tasks.md。

## 阶段验收 — 2026-09-30

同一干净构建已通过普通 PTY 与 Herdr 的完整 G1：显式授权、Agent 实际读写、
检查、真实 diff、取消并终止子进程、恢复与纠正、自然退出。Herdr 取消闭环另有
连续五次成功记录。具体 SHA、脚本、ANSI 与验证状态见 `tasks.md`。

G2 也已在隔离 checkout 中由 Alan 自己完成：写修复、运行检查、解决独立审查
问题、通过完整质量检查，新构建在 Herdr 取消、恢复、纠正并自然退出。
受监督自开发已得到本地证明；G3、CI 与合并尚未完成，仍不能宣称可重复自举。
下面“结论”和基线矩阵保留为 2026-09-29 历史评估。

G2 的隔离起点为 `bb305940e9a44ca09b28a077976b5ffb741ff325`。Alan 自己
修复了取消完成被误呈现为 Error 的共享路径，初版 TUI 154 项测试通过。
独立验证已取得旧实现实际断言失败的证据；后续修订的最终绿测已通过，
编译失败不计作有效红测。
独立审查要求恢复不匹配 submission 的真实观察调用、按测试放置合同提取
增长的 inline suite，并将 helper-only 的 reconnect 检查改为真实重连检查。
Alan 已自行解决这些 findings，两项复审均无遗留问题；详见 tasks.md 的 G2 证据。

监督过程中 Alan 曾依据截断读取结果重写无关测试，随后经正常取消与纠正
自行恢复；验证失败后也曾提前结束并报告下一步。因此当前证据支持受监督
自开发尝试，尚不支持无人监督的稳定自举。

9 月 30 日重新对比 fx 0.0.11 与 Alan，使用同一约 64 列 pane、同一固定
Markdown 输出要求，不调用 Tool。fx 呈现标题、列表及代码/diff 区域，Alan
仍输出原始 Markdown 标记。ANSI 证据为 `/tmp/alan-fx-markdown-20260930.ansi`
和 `/tmp/alan-markdown-20260930.ansi`。fx 的 Herdr atomic paste+Enter 本轮
复现 paste end-marker 拒绝，分开发文本与 Enter 后成功；这是该操作方式的
实测兼容性边界，不外推为普通键入失败，也不比较模型速度。

## 结论（2026-09-29 基线）

**Alan 已是能对话、能在显式授权后读取项目的受控 Agent 原型；尚未达到
可作为日常开发工具、稳定开发自己的状态，不能宣称已完成自举。**

区别不是“有没有 write_file/bash”，而是从用户启动到授权、理解项目、改代码、
运行验证、检查差异、纠正错误和继续工作的整条产品路径是否成立。现有底层
构件足以继续完成这条路径，没有证据要求重建 Agent Execution Engine。

本次没有让 Alan 在主工作树写代码，也没有把外部代理写的代码计作 Alan 自举。
真实项目写入、Rust 工具链在 sandbox 内构建、Alan-authored patch 的独立评审
和新构建复验尚未串成闭环。G2 的状态是“未证明/未通过”，不是证明技术上不可能。

## 证据基线与边界

- Alan source：`660253fa1ff0557dc1f2ef40fb0acefdcb5d1ae2`；
  `cargo build --locked -p alan --bin alan` 成功。
- binary SHA-256：`dbe82ffb0e0d2757c856e336f35d9ec9ff941be7a91c0398fa93392bc19bd47f`。
- `ALAN_INSTALL_CHANNEL=dev`，默认 Connection `chatgpt-main`；未核实其有效模型，
  因而不比较模型速度、质量或成本。
- 首轮 fx 为 `0.0.10`；本轮重新启动时实际显示 `0.0.11`，`fx --version` 确认。
  未主动执行升级。0.0.10 的粘贴兼容问题不外推为 0.0.11 已复现。
- Herdr 同一 cwd、相邻 pane、保留用户焦点；首轮 pane `w58:p6`，补测
  `w58:p7`。约 70 列工作区域；Alan resize 到约 85 列再恢复。
- 可视证据来自 Herdr visible/recent/ANSI 输出与 renderer 调用链，并非像素级
  截图验收。用户图片是目标交互参考，不是当前 Alan 的截图。
- 本机原始记录位于 `/tmp/alan-fx-20260929/`，是临时佐证，不能视为长期 CI
  或发布证据。下面保留可独立阅读的步骤、结果及关键代码位置。

## 实测矩阵

| 场景 | 结果 | 判断 |
| --- | --- | --- |
| 新 Alan 只回复 `ALAN_OK`，然后 Ctrl+D | 成功，回到 fish | 对话和正常退出已可用 |
| 普通启动后读当前目录 Cargo.toml | bash 返回 `Tool Process has no explicit Host execution adapter` | 缺项目引导，不代表获授权执行路径失效 |
| 同一实例 `!pwd` | 同一错误；`alan!` 正确显示 | 显式路由已在，默认项目路径未接通 |
| 显式要求 Alan 调用 `request_mount`，只读 `/mnt/alan` | 实际产生 pending request；TUI 只有 waiting 状态 | Agent 能请求授权，用户呈现不完整 |
| 操作者从外部指定 `ALAN_INSTANCE_RUNTIME_DIR` 后批准 request-1 | grant 成功；Alan 得知授权完成 | 需要内部知识与第二条 CLI，不能算普通入口闭环 |
| 授权后用 read_file 读 `/mnt/alan/Cargo.toml` | 返回 resolver 2、edition 2024；未写文件 | 已验证受控读项目能力 |
| 同一次读取的 UI | 127 行文件作为含转义换行的长 JSON 铺满输出 | 成功路径也有严重信息噪声 |
| `/help`、`/continue` | 第一次 Enter 接受补全，第二次才执行 | 不符合通常命令提交预期 |
| `@Cargo`、跨启动 Up 历史 | 无文件候选；新启动未召回上次输入 | 裸入口没接现有来源 |
| idle draft Ctrl+C | 保留草稿，显示 interrupt requested | 编辑操作与执行控制混淆 |
| 长回答请求 → Ctrl+C → 提交短 follow-up → `/continue` | 首次取消成立；排队缺回显；后续未见短回答，画面停在 35s，Ctrl+C/Ctrl+D 未退出 | 一次待复现可靠性异常，不能断言死锁或根因 |
| 结束异常实例，重新启动并问答 | SIGTERM 后退出；新实例 ALAN_OK、Ctrl+D 成功 | 异常不等于所有实例无法对话 |
| fx 0.0.10 读取同一文件 | 一条 Read 摘要、三行正确回答；Ctrl+O 有完整详情 | 渐进披露明显更成熟 |
| fx `/help`、`/model` | 可导航帮助、模型列表和当前模型 | 可发现性与有效状态可见性更好 |
| fx 0.0.11 固定 Markdown 回答 | 标题、加粗、列表、rust 代码区域有清楚层级 | 对比 Alan 纯文本折行管线有设计差距 |

完整权限拒绝、写入/测试、恢复选择、实际 Herdr view detach、相同模型下性能
以及 fx 取消闭环，本次未对等验收，不标为通过或失败。

## UI/UX critique

### 总体判断

这里主要不是常见网页式 AI 视觉模板问题，也不是需要更多装饰。Alan 仍像把
执行日志接到输入框，fx 则把输出组织成了用户可扫描的工作过程。其最大机会
是让语义、层级和操作反馈回到真正的生产路径。

已有优势：inline 输出与宿主滚屏方向正确；`alan:` / `alan!` 保留明确意图；
键盘与终端原生选择无需重造。继续复用这些基础。

### 优先问题与设计处理

| 优先级 | 具体问题与影响 | 处理 |
| --- | --- | --- |
| P0 | 等待授权只显示 waiting，用户不知道在等什么、怎么解除 | 同一前台入口展示目录、范围、原因及批准/更换/取消 |
| P1 | 没有稳定的模型/项目/状态位置，无法确认输入会作用于谁 | 用户指定的两行 Agent prompt：上行上下文和真实状态，下行输入 |
| P1 | 成功读文件也喷出 JSON；内容挤掉回答，长单行逃过折叠 | 复用 typed presentation，经 Action 文件保留语义；按物理行和字节限摘要 |
| P1 | 排队清空草稿却不显示回执，取消与等待像停住 | 区分已接收、queued、running、paused、unknown；明确下一步 |
| P2 | 整行染色和统一字符串折行无法表达 Markdown、diff、代码层级 | 现有 HistoryCell 输出语义 spans；统一留白、缩进、字重和颜色角色 |
| P2 | 补全/历史实际未接通；命令双 Enter | 接现有来源，调整键盘提交语义，帮助中明确说明 |
| P2 | 有 more-lines 文案但没有同等级完整 Tool 详情入口 | 基于保留证据做 Ctrl+O 临时详情，保留草稿和滚屏位置 |

不用网页字体/卡片/动画规范套终端；继承宿主等宽字体与主题。颜色只承担
语义辅助，不能代替成功/失败文字、diff 的 +/- 或模式提示。

### 两行 Agent prompt

用户参考图的价值是“上下文一行、输入一行”的稳定节奏，而不是 Git 分支装饰。
建议默认：

```text
alan /crates/tui · <有效模型> · ready
: ▌
```

执行中和等待时只更新上行必要字段：

```text
alan /crates/tui · <有效模型> · working 8s · 1 queued
: ▌
```

命令输入第二行变为 `! `。这是用户在两行参考之后确认的新显示形式；
当前源码仍为 `alan: ` / `alan! `，路由语义不变。没有项目时明确 `no project · /project`；模型
未知时显示 unknown。token、费用、profile、Git 状态放 `/status`，不默认
塞进 prompt。窄屏先缩短目录，保留模型与当前状态；关键批准操作可临时扩展。
该布局紧跟 transcript，不占据整个屏幕，也不固定在屏幕底部。

## 调用链证据与最小修复位置

- `crates/alan/src/main.rs:745` 使用 `FileBackedRunConfig::new`，没有接补全与
  历史来源；`crates/tui/src/file_backed.rs:83` 默认三个来源为空。
- `crates/os-host/src/boot.rs:216` 产品 Root 以 `/` 启动；Host cwd 未作为
  授权项目接入。`crates/service-manager/src/host_mount.rs:794` 按有效 grant
  构造执行 adapter，因此缺 grant 时拒绝执行是正确的安全边界。
- `crates/tui/src/file_backed/file_surface.rs:854` 的
  `action_snapshot_to_history_cell` 把输出转换为 PlainText；
  `crates/agent-engine/src/runtime/tool_presentation.rs` 已有共同语义映射。
- `crates/tui/src/history.rs:229` 起只按逻辑行计数截断，随后才折行；
  单行 JSON 的转义换行不会触发预期的屏幕行限制。
- `crates/tui/src/history.rs` 的 Assistant 分支走 `wrap_plain_text`；
  `crates/tui/src/transcript_ui.rs:23` 再按字符串前缀给整行样式，Markdown 与
  Diff 语义不会自然保留下来。改语义传递链，避免按工具名补一个个 renderer。
- `crates/tui/src/file_backed/app.rs:297` 的 completion Enter 只接受候选；
  `handle_key` 的 Ctrl+C 总走 interrupt，解释两个输入摩擦。

## 已有证据与新门槛的关系

`unify-agent-command-input/tasks.md` 的 2026-09-28 TB1/TB2 已记录经过显式挂载
后的 Agent 写→Shell 读、Shell 写→Agent 读、命令退出状态和一次无重放恢复。
这些证据有效，不因本次默认入口失败而抹去；但不是 Alan 对自身代码的
修改→失败/通过验证→独立审查→新构建验收。计划按 G1/G2/G3 补齐这一距离。

建议先把一次真实开发循环跑通，再让 Alan 承担后续界面小改动。授权、Review
和发布仍由正常工程流程管理，自举不等于自批、自合并或自发替换稳定版本。

## 2026-09-29 后续验证

当前本地实现已从评估进入实现阶段，但尚未发布。源码仍在
`660253fa1ff0557dc1f2ef40fb0acefdcb5d1ae2`，工作树有未提交的 Alan Shell、Host
和 OpenSpec 变更；早先 `just quality` 及终端 UI、Host lifecycle、Agent Engine
cwd-binding、Alan CLI 的聚焦测试曾通过。PTY 诊断代码已移除；诊断改动后的完整
门禁和候选构建需要重跑，不能用先前结果代替当前 SHA 的验证。

此前一次带临时诊断代码的普通终端 PTY 运行已进入显式项目选择和工具批准。
在那次诊断运行中，界面停留在 “sending response”，没有启动获批的限时命令。
向 PTY 写入 Ctrl+C 后，独立 crossterm reader 和 Alan 的动作分派都识别了按键；
Alan 试图写入
`/agent/8/machine/ctl`，对应提交 ID 为
`2814a5ac-d87a-4529-a119-33e1a06166d7`，但控制写没有完成。测试实例已结束并
清理其临时运行目录。此证据只适用于那次诊断运行，不能当作后续干净候选的
按键路径结论。后一干净普通 PTY 在 sleep 审批处没有启动子进程；当前 Herdr
复测也没有提交审批输入。根因仍未确定。G1 因此仍未通过，G2/G3 暂不启动；
也没有独立 Review、合并或 current-head CI 证据。

## 2026-09-29 Herdr clean-candidate retest and status-line change

Using the clean `target/quality-gate/.../alan` candidate in the existing Herdr
pane, I explicitly selected the disposable fixture as read-write. Alan showed
the project-relative location and `/mnt/project-request-1/` cwd. It read
`original: apple`, replaced the exact line with `updated: pear`, and the fixed
exact-line grep and real `git diff -- note.txt` succeeded. This proves the
authorized read/edit/check/diff slice in Herdr; it does not prove G1.

The same invocation displayed `model unknown`, while its rollout
`turn_context` recorded `gpt-6-luna`. The status line had hardcoded the unknown
label. The implementation now carries the model selected by Service Manager
after Agent Definition and Connection profile metadata resolution through
AlanOsHost into the TUI; when a Host does not provide it, the UI still says
`unknown`. Focused Service Manager and TUI tests pass; a fresh candidate build
and live Herdr check are still required.

For cancellation, `! sleep 60` reached the unknown-capability approval prompt.
No approval response was submitted and no `sleep` child started. Pasting text
worked, but Herdr `pane send-keys` for a printable character, Ctrl+U, Esc and
Ctrl+C produced no visible composer change even after temporarily focusing the
Alan pane. The reader thread was polling the same raw TTY as the pane's Alan
PID; the rollout contains no user response after the escalation. I stopped
only that exact test PID with TERM after it remained unresponsive. This leaves
Herdr key delivery versus Alan's event handling unresolved; it is not evidence
that Alan cancellation passed or failed. The fixture's verified diff remains
in the temporary test directory. Ordinary-PTY cancellation and a correction
after cancellation are still open G1 evidence.

## 2026-09-29 ordinary PTY retest after moving Root Agent PID polling

Moved Root Agent PID polling out of the TUI event loop into a background task,
with a regression proving a delayed PID file read does not prevent terminal
events from being received. `cargo test --locked -p alan-terminal-ui` passed
153 tests, and the full `just quality` gate passed after this code change.
Candidate SHA-256:
`6de169e8d4c0b9fcac505745418e6c02b60dbded8b4432b6ec70e6158d5f2f0c`.

The fresh ordinary PTY showed `alan no project · model gpt-6-luna · ready`
with the bare `: ` and `! ` prompts. `! sleep 60` reached its capability
approval. After `approve`, the UI stayed at “sending response…” and no sleep
process started. Ctrl+C did not produce a visible state change; a printable
`x` was accepted into the composer, proving input delivery but not cancellation.
The rollout `2c77a179-a043-4a29-9d74-f7bdbb908c90` contains the escalation but
no completed response or subsequent engine activity.

A live sample showed the event loop waiting for its next event while the
background Root Agent PID read was inside `Shell.cat`; an aP `remote_call` was
waiting in a Unix-socket request write, another call was queued on that
connection's writer, and the response router was waiting for a frame. This
locates a transport wait but does not identify which request owns the stalled
write or prove it caused the approval response to remain pending. The exact
candidate process was stopped after the sample; no child process remained.
Root PID polling no longer blocks the event loop, but the response and control
write path is still unresolved. G1 remains unpassed; G2 and G3 have not started.

The same candidate was launched in Herdr. The current instance showed
`model gpt-6-luna`; `! sleep 60` reached the approval prompt, and pressing `1`
changed the notice to “sending response…”. Its rollout
`ccc4171d-70fa-47a2-b6d4-d02fd0c32c0e` still contains only
`escalation_required`, with no completed confirmation or engine activity, and
no sleep process existed. Ctrl+C produced no visible acknowledgement. A short
sample showed `AlanOsHost::serve_until` writing a tagged aP response frame over
the local socket while the Root Agent remained alive. That does not identify
the response or prove this socket write is the approval response. I stopped
only this exact test process with TERM and verified Herdr returned to fish.
This repeat confirms the blocker in Herdr but is not a G1 pass.
