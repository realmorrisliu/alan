## Context

Alan 当前的 macOS 本地安装链路是单通道的：

- `just install` 调用 `scripts/install.sh`，安装
  `$HOME/Applications/Alan.app`，并把 `alan` / `alan-tui` 链到嵌入式
  `Contents/Resources/bin`。
- Xcode project 默认生成 `Alan.app`，display name 为 `Alan`，bundle id 为
  `app.alanworks.macos`。
- runtime path resolver 默认把用户数据放在 `~/.alan`，并把全局 public skills
  放在 `~/.agents/skills`。
- daemon host config、connection profiles、credentials、managed auth、registry、
  sessions 和 models 都是这个单一身份的一部分。
- macOS shell-control socket 当前使用临时目录下的 `alan-shell-control`，没有
  channel 维度。

这些默认值适合正式 Alan，但不适合本机开发。开发者需要一个可并行运行的
`Alan Dev`，它可以测试未发布代码、破坏自己的配置和登录状态，但不能影响正式
Alan 的日常工作环境。

## Goals / Non-Goals

**Goals:**

- 定义 `stable` 和 `dev` 两个 install channel，并让第一版 dev channel 可在本机与
  stable channel 并行运行。
- 保持 stable channel 现有用户可见契约不变。
- 让 dev channel 拥有独立 app bundle、bundle id、CLI/TUI command names、alan
  home、daemon endpoint、host config、credentials、managed auth、skills source、
  singleton 和 shell-control namespace。
- 让同一个 source workspace 可以同时被 stable 和 dev 使用，但 generated runtime
  state 必须按 channel 隔离。
- 提供本地安装和卸载工作流，避免手动 env 切换成为主要使用方式。
- 把第一版限制为本地开发通道，便于后续实现和验证。

**Non-Goals:**

- 不发布公开 `Alan Dev` 下载包、Homebrew cask、Sparkle appcast 或 auto-update
  channel。
- 不自动迁移、复制或同步 stable channel 的 connections、credentials、managed auth、
  agents、models、skills、sessions 或 memory。
- 不改变 stable channel 的 `Alan.app`、`alan`、`alan-tui`、`~/.alan`、
  Homebrew cask 或 direct install contract。
- 不把 workspace-authored `.alan/agents/` 或 `.agents/skills/` 改成 dev-only；这些
  仍然是 workspace 内容，由 source control/工作区语义决定是否共享。
- 不把 dev channel 作为 production beta/nightly 发行体系。

## Decisions

1. **使用显式 install channel，而不是 env-only dev mode。**

   Channel 是安装和运行身份，不只是 `ALAN_HOME=...`。`alan-dev`、`Alan Dev.app`
   和 `alan-dev-tui` 必须在进入 config/path/daemon 解析前选择 `dev` channel。
   环境变量可以作为调试覆盖，但不能是第一版的主入口。这样可以避免用户在正式
   `alan` 命令里忘记设置 env 后误写 `~/.alan`。

2. **用一个 channel descriptor 驱动所有 machine identity。**

   实现应集中定义至少这些字段：

   | Field | stable | dev |
   | --- | --- | --- |
   | channel id | `stable` | `dev` |
   | app bundle | `Alan.app` | `Alan Dev.app` |
   | display name | `Alan` | `Alan Dev` |
   | bundle id | `app.alanworks.macos` | `app.alanworks.macos.dev` |
   | CLI | `alan` | `alan-dev` |
   | TUI | `alan-tui` | `alan-dev-tui` |
   | alan home | `~/.alan` | `~/.alan-dev` |
   | global public skills | `~/.agents/skills` | `~/.agents-dev/skills` |
   | daemon bind default | `0.0.0.0:8090` | `127.0.0.1:8091` |
   | daemon URL default | `http://127.0.0.1:8090` | `http://127.0.0.1:8091` |
   | shell control namespace | `alan-shell-control` | `alan-dev-shell-control` |

   Scripts, app metadata generation, CLI daemon defaults, TUI config resolution,
   shell-control paths, logging/capture helpers and tests should consume this descriptor or a generated
   projection of it. Scattering literals would recreate the same single-channel problem.

3. **Keep stable behavior untouched and make dev additive.**

   `just install` and `just uninstall` remain stable-only. Dev gets separate recipes such as
   `just install-dev` and `just uninstall-dev`. Dev uninstall removes `Alan Dev.app` and
   `alan-dev` / `alan-dev-tui` links when owned by that install, but it does not delete
   `~/.alan-dev` unless a future explicit data-removal command is added.

4. **Dev app is local-only but still release-shaped.**

   Dev install should exercise the same broad packaging shape as stable: app bundle with embedded
   CLI/TUI, signing, manifest checks, ownership-safe symlink behavior and no force-kill/relaunch.
   It may skip notarization because it is not a public artifact. Reusing the release-shaped path
   prevents dev-only launch assumptions from hiding bugs that would appear in stable packaging.

5. **Runtime data isolation is channel-scoped by default.**

   The active channel controls alan home resolution before any runtime, daemon, CLI, TUI or app
   config is read. Dev channel reads and writes `~/.alan-dev/host.toml`,
   `~/.alan-dev/connections.toml`, `~/.alan-dev/credentials/`,
   `~/.alan-dev/agents/`, `~/.alan-dev/models.toml`, session/history stores,
   registry, managed auth, memory and caches. It MUST NOT silently fall back to
   stable channel state when dev state is missing.

6. **Workspace authored content can be shared; generated runtime state cannot.**

   Source-controlled workspace definitions such as `<workspace>/.alan/agents/` and
   `<workspace>/.agents/skills/` remain workspace content. Generated workspace state must carry a
   channel dimension, for example under `<workspace>/.alan/runtime/<channel>/...`, or another
   clearly channel-scoped generated path. Dev channel must not write into legacy stable generated
   paths such as `<workspace>/.alan/sessions/` or `<workspace>/.alan/memory/`.

7. **Connection and auth import is explicit.**

   First run of `Alan Dev` should behave like a fresh install. If users want to reuse a stable
   profile or login for testing, that must be a deliberate import/copy command or manual action.
   Dev startup must not read stable managed auth as fallback, because auth-state bugs are exactly
   the kind of breakage this channel is meant to contain.

## Risks / Trade-offs

- **[Risk] Literal drift across scripts, Xcode, Swift, Rust and TUI.** -> Centralize channel
  metadata and add focused guardrails that scan for unallowlisted stable-only literals in dev-aware
  code paths.
- **[Risk] Two daemons accidentally bind the same endpoint.** -> Give dev a distinct default bind
  address and client URL; validation should launch or inspect both channel configs together.
- **[Risk] Dev reads stable auth because a path resolver falls back on missing files.** -> Treat
  missing dev config as configuration-required or onboarding-required, not as permission to read
  stable state.
- **[Risk] Workspace `.alan` state becomes confusing.** -> Keep authored workspace definitions in
  their existing locations, but put generated runtime state under an explicit channel namespace and
  update ignore/docs accordingly.
- **[Risk] Brand guardrails reject `Alan Dev`.** -> Add an allowlisted local-channel exception while
  keeping public product branding as `Alan`.
- **[Risk] Dev channel packaging becomes a parallel release system.** -> Keep dev non-public in V1:
  no cask, no Sparkle feed, no public zip, no notarized release requirement.

## Migration Plan

1. Introduce a channel descriptor and teach local install scripts to assemble/install stable and dev
   without changing stable behavior.
2. Add dev app metadata generation or build-setting overrides for `Alan Dev.app` and
   `app.alanworks.macos.dev`.
3. Add `alan-dev` and `alan-dev-tui` entrypoints that select dev channel before reading config.
4. Thread channel-aware path resolution through runtime, daemon, CLI/TUI, managed auth and skill
   discovery.
5. Add channel-specific shell-control, singleton, support path, logging and capture-helper defaults.
6. Add focused checks that verify stable and dev install outputs, data paths and daemon/control
   endpoints are distinct.
7. Manually verify stable Alan keeps running while `Alan Dev` starts, opens a workspace, creates a
   session and logs into or configures its own provider profile.

Rollback is straightforward because dev is additive: remove `Alan Dev.app`, `alan-dev` and
`alan-dev-tui`, and leave `~/.alan-dev` intact for inspection. Stable `Alan.app`, `alan`,
`alan-tui` and `~/.alan` are not modified by dev uninstall.

## Open Questions

- Should a later change add an explicit `alan-dev import stable-profile <id>` helper, or keep
  profile reuse entirely manual?
- Should dev channel ever offer an opt-in read-only view of `~/.agents/skills` for convenience?
  This design chooses isolation first for V1.
