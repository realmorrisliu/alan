## MODIFIED Requirements

### Requirement: Terminal host boundary is testable
The terminal host SHALL expose a testable boundary for runtime attachment,
teardown, and text delivery without requiring the real Ghostty library in every
test.

#### Scenario: Mock runtime accepts text
- **WHEN** a test runtime is registered for terminal content and `terminal.send_text` is issued
- **THEN** the test verifies the text reaches the runtime and the control response reports accepted bytes with the terminal `content_id`

#### Scenario: Mock runtime unavailable
- **WHEN** no runtime is registered for terminal content and `terminal.send_text` is issued
- **THEN** the test verifies the response reports failure or durable queueing according to the delivery contract

### Requirement: Runtime service ownership has focused tests
The Apple client SHALL include focused tests for process bootstrap, window
runtime service ownership, terminal ContentInstance handle creation, reattachment,
text delivery, and teardown using fake Ghostty adapters where possible.

#### Scenario: Fake runtime reattaches view
- **WHEN** a test creates a terminal ContentInstance handle, detaches the host view, and attaches a replacement host view
- **THEN** the test verifies that the `content_id` handle identity and runtime metadata remain unchanged

#### Scenario: Fake runtime tears down once
- **WHEN** a test closes a terminal ContentInstance, PaneSlot, tab, or window through shell actions
- **THEN** the fake runtime observes exactly one teardown call per affected terminal ContentInstance

### Requirement: Control-plane runtime tests use the service boundary
Control-plane tests SHALL exercise runtime-dependent mutations through the same
terminal runtime service boundary used by production code.

#### Scenario: Service accepts text
- **WHEN** a control-plane test sends text to fake live terminal content with `terminal.send_text`
- **THEN** the command response reports accepted bytes from the fake service and shell diagnostics remain clean

#### Scenario: Service reports runtime missing
- **WHEN** a control-plane test sends text to terminal content whose service handle is absent
- **THEN** the command response reports a stable runtime-missing error

## ADDED Requirements

### Requirement: Content container model has focused tests
Apple client SHALL 为 v0.2 content-container model、旧状态迁移、mixed split、content-aware
command validation 和 content-keyed terminal runtime continuity 提供 focused 自动化测试或明确的人工验证记录。

#### Scenario: Old terminal state migrates
- **WHEN** 测试加载 `persist-macos-shell-workspaces` 产生的 terminal-only workspace manifest
- **THEN** shell model 迁移出 PaneSlot 和 terminal ContentInstance
- **AND** focused space、focused tab、focused PaneSlot、pin/live snapshot 和 terminal metadata projection 保持一致

#### Scenario: Legacy shell state is not restored as workspace authority
- **WHEN** 测试环境同时存在旧 `shell-state-window_main.json` 和 workspace manifest
- **THEN** app restore 使用 workspace manifest materializer
- **AND** 旧 shell-state 只作为 runtime/control-plane projection 或诊断输入存在

#### Scenario: Mixed split mutates safely
- **WHEN** 测试创建 terminal + markdown + settings 的 mixed split tab
- **THEN** split、focus、move、close 和 equalize 操作保持 split tree 有效
- **AND** terminal runtime identity 不因非 terminal PaneSlot 操作重建

#### Scenario: Non-terminal command rejection tested
- **WHEN** control-plane 测试向 markdown 或 settings PaneSlot 发送 `terminal.send_text`
- **THEN** response 使用 stable unsupported-content error
- **AND** fake terminal runtime service 没有收到 delivery

#### Scenario: Terminal content identity survives movement
- **WHEN** 测试将 terminal ContentInstance 所在 PaneSlot 移动到另一个 tab 或重新 attach 视图
- **THEN** terminal runtime handle、scrollback、metadata 和 pending delivery 仍绑定到同一个 `content_id`

#### Scenario: Retired unpinned tab finalizes terminal content
- **WHEN** workspace lifecycle pruning retires an inactive unpinned Tab that contains terminal ContentInstances
- **THEN** focused tests verify those terminal runtimes are finalized through the runtime service
- **AND** retired PaneSlots and terminal ContentInstances cannot receive later `terminal.send_text` delivery

#### Scenario: Content rendering registry verified
- **WHEN** renderer registry 收到 terminal、markdown 和 settings content descriptor
- **THEN** 测试或 review checklist 确认每个 kind 路由到对应 renderer 或 bounded unavailable surface

#### Scenario: Visual evidence covers mixed content
- **WHEN** content-container UI implementation 标记完成
- **THEN** maintainers 可以检查 running-app screenshot 或记录，确认 light-mode shell 中存在混合 content tab/split，且 sidebar、toolbar、pane title bar 和 terminal-first chrome 保持一致
