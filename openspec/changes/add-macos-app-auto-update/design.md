## Context

Alan 当前已经有 app-first 的 macOS 发布链路：`just release` 构建 Release
`Alan.app`，嵌入 release CLI，使用 Developer ID 签名，可选公证并输出
`alan-<version>-macos.zip`。Homebrew cask 模板已经假定 zip 来自 GitHub
Releases，并从 app bundle 内链接 `alan`。

缺口在直接下载安装路径：非 Homebrew 用户拿到 `Alan.app` 后，后续版本只能手动
检查 GitHub 或重新下载安装。用户已经确定 `alanworks.app` 会作为产品网站域名，
并希望 Cloudflare 只托管网站和 Sparkle appcast，小文件由 Cloudflare Pages
负责，zip 继续放 GitHub Releases。

## Goals / Non-Goals

**Goals:**

- 为直接安装的 `Alan.app` 增加 Sparkle 2 更新检查和安装能力。
- 使用 `https://alanworks.app/appcast.xml` 作为稳定 `SUFeedURL`。
- 让 Cloudflare Pages 托管产品网站和 `appcast.xml`，不托管 release zip。
- 继续使用 GitHub Releases 托管签名、公证后的
  `alan-<version>-macos.zip` 和 checksum。
- 明确 Homebrew cask 安装由 Homebrew 更新，不由 Sparkle 改写 bundle。
- 建立 release/version/appcast 验证，避免 Cargo、Xcode、appcast 和 zip 版本漂移。

**Non-Goals:**

- 设计或实现 `alanworks.app` 的产品网站视觉和内容。
- 把 release zip 从 GitHub Releases 迁移到 Cloudflare R2。
- 实现自研 GitHub Release downloader 或自研 app bundle 替换逻辑。
- 第一版强制静默后台安装；默认保留用户可见的 Sparkle 更新确认流程。
- 第一版强制 Sparkle signed feed。必须签名的是 update archive；signed feed
  可以作为后续安全加固。
- 让 Sparkle 接管 Homebrew cask 安装的更新。

## Decisions

1. **使用 Sparkle 2，而不是自研 GitHub updater。**

   Sparkle 负责 macOS app 更新检查、下载、签名校验、权限处理和原子替换。Alan
   只负责生成可信 release artifact 和 appcast。自研 updater 会重新实现高风险的
   bundle 替换、安全校验和权限逻辑，不适合作为第一版。

2. **Cloudflare Pages 只托管网站和 appcast。**

   `https://alanworks.app/` 是产品网站入口，
   `https://alanworks.app/appcast.xml` 是 Sparkle feed。`appcast.xml` 需要
   明确 `Content-Type: application/xml; charset=utf-8`，并使用短缓存或
   `Cache-Control: no-cache, max-age=0, must-revalidate`，避免客户端在发布后
   长时间读到旧 feed。

3. **GitHub Releases 继续拥有 zip 资产。**

   appcast 的 enclosure URL 指向
   `https://github.com/realmorrisliu/alan/releases/download/v<version>/alan-<version>-macos.zip`。
   这保持现有 Homebrew cask 模板、release checksum 和公开发布页面一致。Cloudflare
   不需要代理或缓存 zip。

4. **直接安装和 Homebrew 安装分流。**

   直接下载或拖拽安装的 `Alan.app` 可以显示 `Check for Updates...` 并使用
   Sparkle。Homebrew cask 管理的安装必须把 Homebrew 视为权威更新器，避免 Sparkle
   在 Homebrew 管理的 app bundle 上执行替换。第一版可以通过安装路径、Homebrew
   管理的 binary links、或其他明确检测信号禁用/改写菜单提示。

5. **版本以发布版本和递增 build number 共同判定。**

   Cargo workspace version、Xcode `MARKETING_VERSION`、zip 文件名、GitHub release
   tag、appcast `sparkle:shortVersionString` 必须一致。Xcode
   `CURRENT_PROJECT_VERSION` / appcast `sparkle:version` 必须单调递增，因为 Sparkle
   用 bundle version 判定更新顺序。release validation 应在发布前阻断漂移。

6. **release 流程先手动闭环，再自动化。**

   第一阶段把本地 `just release`、GitHub release asset 上传、appcast 生成和
   Cloudflare Pages 部署跑通。真实验收必须从旧版已安装 `Alan.app` 更新到新版。
   稳定后再把 GitHub Actions、Cloudflare Pages deploy 和 appcast 生成纳入自动化。

## Risks / Trade-offs

- **[Risk] Sparkle 嵌入 framework/helper 后签名顺序不完整。** -> release
  validation 必须检查 Sparkle 嵌套代码、Alan app、CLI/TUI 都是 Developer ID
  签名，且最终 app bundle 通过严格 codesign、公证和 stapler 验证。
- **[Risk] appcast 被 Cloudflare 缓存导致新版不可见。** -> `appcast.xml`
  使用低缓存头，并在验证中通过 HTTP header 检查确认。
- **[Risk] Homebrew 安装被 Sparkle 改写，破坏 cask 状态。** -> App 端检测
  Homebrew 管理路径或链接，禁用 Sparkle 安装路径或给出 Homebrew 更新提示。
- **[Risk] 版本号漂移导致 Sparkle 不提示更新或错误降级。** -> 发布脚本检查
  Cargo、Xcode、zip、GitHub tag 和 appcast 版本一致，并检查 build number 单调递增。
- **[Risk] Sparkle EdDSA private key 泄漏。** -> 私钥不进入 repo；本地或 CI
  通过安全文件、Keychain 或 secrets 注入，release 日志不得打印私钥内容。
- **[Risk] 首次发布没有旧版样本，无法证明更新。** -> 在真正公开前保留一个旧版
  signed/notarized fixture 或通过临时较低 build number 的安装包验证旧版到新版流程。

## Migration Plan

1. 创建 Sparkle EdDSA key，把 public key 写入 app Info.plist 配置，private key
   存在本地安全位置或未来 CI secret。
2. 给 Xcode project 增加 Sparkle 2 依赖和 `Check for Updates...` 菜单入口。
3. 更新 release assembly/signing validation，覆盖 Sparkle 嵌套代码和版本一致性。
4. 增加 appcast 生成脚本：读取 release zip、GitHub release URL、版本号、长度、
   EdDSA signature，生成 `appcast.xml`。
5. 在 `alanworks.app` 的 Cloudflare Pages 项目中发布网站静态文件和 `appcast.xml`，
   并配置 appcast 的低缓存 header。
6. 发布一个包含 zip asset 的 GitHub Release，部署 appcast，再用旧版
   `Alan.app` 验证 Sparkle 能检测、下载、校验并安装新版。
7. 更新 Homebrew 文档和 cask 维护说明，明确 cask 用户通过 Homebrew 更新。

Rollback：如果 Sparkle 集成出现问题，可以发布一个不启用自动检查的修复版本，或从
Cloudflare Pages 暂时回滚/移除 `appcast.xml` 中的新 item。已经发布的 GitHub
Release zip 不应原地替换；需要用更高版本发布修复。

## Open Questions

- Homebrew 安装检测第一版使用哪一个主信号：app 路径、Homebrew binary links，还是
  release manifest 中的安装来源标记？
- 第一版是否需要 beta/prerelease channel，还是只支持 stable appcast？
- GitHub Release asset 上传和 Cloudflare Pages deploy 是否在第一版保留为手动步骤，
  还是直接接入 GitHub Actions？
