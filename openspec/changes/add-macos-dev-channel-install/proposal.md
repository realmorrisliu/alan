## Why

Alan 现在只有一个本地安装通道：开发中的 `Alan.app`、`alan` CLI、daemon 配置和
`~/.alan` 数据都服务同一个日常工作环境。开发者在本机测试 macOS app、runtime、
connection 或 shell-control 改动时，一旦改坏安装产物或数据路径，就可能影响正在使用的
正式 Alan。

## What Changes

- 增加本地-only 的 `dev` install channel，用于安装和运行 `Alan Dev.app`。
- 保持正式通道现状：`Alan.app`、`app.alanworks.macos`、`alan`、`alan-tui`、
  `~/.alan`。
- 为 dev 通道定义独立身份和入口：`Alan Dev.app`、
  `app.alanworks.macos.dev`、`alan-dev`、`alan-dev-tui`、`~/.alan-dev`。
- 允许正式 Alan 和 Alan Dev 同时运行，并隔离 bundle singleton、shell-control
  socket、daemon endpoint、host config、connection profiles、credentials、managed
  auth、session/memory/cache 和 generated workspace runtime state。
- 增加 `just install-dev` / `just uninstall-dev` 之类的本地工作流，且不得覆盖正式
  app、CLI/TUI links 或 `~/.alan` 数据。
- 明确第一版不做公开 dev release、Homebrew dev cask、Sparkle dev feed、自动迁移或
  自动复制正式配置；配置导入必须是显式用户动作。

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `alan-app-distribution`: 增加 stable/dev install channel、dev app/CLI/TUI
  命名、本地安装/卸载和非公开分发边界。
- `macos-app-instance-lifecycle`: 增加 channel-scoped bundle identity、
  singleton、logging/support identity 和 shell-control namespace 隔离。
- `agent-root-layout`: 将全局 agent root 从固定 `~/.alan` 扩展为当前 install
  channel 的 alan home；stable 仍使用 `~/.alan`，dev 使用 `~/.alan-dev`。
- `workspace-runtime-state-hygiene`: 增加 channel-scoped generated workspace
  runtime state，防止 dev 版写入正式版会读取的 generated state。
- `provider-connection-contract`: 增加 channel-scoped connection metadata、
  credential store 和 managed auth state，防止 dev 版复用或覆盖正式登录状态。
- `skill-system-contract`: 增加 channel-scoped global public skill source 约束，
  避免 dev 通道安装/修改的全局 skill 影响正式通道。
- `remote-control-contract`: 增加 channel-scoped local daemon defaults，确保
  dev daemon/client 不默认连接或绑定到正式通道。
- `product-brand-identity`: 允许 `Alan Dev` 作为本地开发通道的限定显示名和
  machine identifier，同时保持公开产品品牌为 `Alan`。
- `macos-shell-build-test-contract`: 增加 dev channel 安装、并行运行、路径隔离和
  guardrail 验证要求。

## Impact

- macOS Xcode build settings / packaging scripts need channel-aware product name,
  bundle identifier, display name, signing, package manifest, and app install
  target handling.
- CLI/TUI embedding and link installers need channel-aware tool names and
  ownership checks.
- Runtime path resolution needs a host/install-channel input so stable uses
  `~/.alan` while dev uses `~/.alan-dev`, including host config, registry,
  connections, credentials, models, agents, sessions, and managed auth.
- Daemon/client startup needs per-channel defaults so `alan` talks to the stable
  daemon and `alan-dev` talks to the dev daemon without relying on manual env
  switching.
- macOS shell control-plane paths, singleton locks, log subsystems, support
  paths, capture helpers, tests, and brand guardrails need channel-aware
  allowlists.
