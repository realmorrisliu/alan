## Why

Alan 已经有 app-first 的签名、公证、GitHub Release zip 和 Homebrew cask
包装基础，但直接下载安装的 `Alan.app` 还不能自己发现和安装新版本。现在要把
自动更新纳入正式 macOS 分发契约，让非 Homebrew 用户可以通过稳定的更新 feed
跟进公开发布，同时继续保持 GitHub Releases 作为发布产物来源。

## What Changes

- 为直接安装的 `Alan.app` 增加 Sparkle 2 自动更新能力。
- 将 Sparkle feed 固定到 `https://alanworks.app/appcast.xml`，由
  Cloudflare Pages 托管网站和 appcast 小文件。
- 继续让 GitHub Releases 托管签名、公证后的
  `alan-<version>-macos.zip` 和 checksum，appcast 只引用这些 release
  asset。
- 明确 Homebrew cask 安装路径不由 Sparkle 接管；Homebrew 用户继续通过
  `brew upgrade --cask alan` 更新，避免 cask 状态和 app bundle 内容漂移。
- 为 release/appcast 生成、版本递增、Sparkle update 签名元数据、公证、
  feed 缓存和旧版到新版的真实更新流程增加验证要求。

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `alan-app-distribution`: 增加 macOS App 自动更新、Cloudflare 托管
  appcast、GitHub Release zip 资产、Homebrew 分流和版本递增契约。
- `macos-shell-build-test-contract`: 增加 Sparkle 集成、appcast 生成、签名
  公证、缓存头和真实旧版更新到新版的发布验证要求。

## Impact

- `clients/apple/alan-macos.xcodeproj` 需要引入 Sparkle 2 依赖并配置
  `SUFeedURL` / `SUPublicEDKey` 等 Info.plist 元数据。
- macOS app 菜单需要提供显式的 `Check for Updates...` 入口，并在
  Homebrew 管理的安装中避免误导用户使用 Sparkle 更新。
- `scripts/assemble-release-app.sh` / release validation 需要覆盖 Sparkle
  framework/helper 的签名、公证和 appcast 签名产物。
- release 流程需要发布 GitHub Release zip 后生成或刷新
  `appcast.xml`，并把该文件部署到 `alanworks.app`。
- Cloudflare Pages 只托管网站和 `appcast.xml`；release zip 仍然属于
  GitHub Releases。
