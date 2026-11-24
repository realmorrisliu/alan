# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.8] - 2026-11-24

### Added

- Preinstall `github-fileserver` as catalog-only (zero ambient authority) in `PackageService`, surface failed command `hint:` stderr lines under TUI action summaries, and support `allow_persistent` confirmation options and receipts in the TUI (#1156).
- Add approval-gated Root-only `request_service` virtual tool (`service_activation` checkpoint) with host-side `ProcessServiceActivation`, live fileserver hot-mounting into Root `LiveNamespace`, `system_namespace`, and `LocalEntryService`, multi-tool-batch pause safety, and optional persistent `RootBindingMode::Active` activation on `allow_persistent` (#1157).

## [0.1.7] - 2026-11-24

### Fixed

- Fix TUI image paste, macOS `Ctrl+V` clipboard attach, slash-path disambiguation, and structural JSON command summaries, adding validated local/pasted/clipboard image attachment parsing, live preview strips, `Backspace` removal, and image-only turn submission (#1152).
- Fix `gh` CLI and `github-fileserver` auth under OS sandboxing by classifying `gh` subcommands as `Network`, routing explicit `GH_TOKEN`/`GITHUB_TOKEN` runs past `~/.config/gh` read denials, emitting non-leaking `~/<rel>` sandbox credential-denial hints, and resolving host `gh auth token` for authorized network-enabled `github-fileserver` launches (#1153).

## [0.1.6] - 2026-11-23

### Added

- Add dual `stable` and `beta` release channels with build-time version/channel metadata injection (`crates/alan/build.rs`), rolling `main` beta pre-release builds (`release-beta.yml`), `alan update --channel`, `alanworks.app/beta.json`, and `install.sh --channel` (#1147).
- Add persisted release channel selection (`services/updater/channel.json`) via `alan update channel [stable|beta]` and interactive TUI `/settings -> Release Channel` picker alongside `/beta.json` Cloudflare Worker asset routing (#1150).
- Automate AI-native redacted `/issue` and `/bug` drafting with interruptible progress, allowlisted tool failure context, `Ctrl+L` `alan:auto` label toggling, and deterministic fallback (#1146).
- Upgrade TUI approval and queue UX with pre-execution `bash`/`edit_file`/`write_file` previews, a dedicated bottom confirmation selector (keyboard navigation, `Reject with note…`, `d` detail expand/collapse, viewport-aware overflow scrolling, key swallowing, and settled single-line receipts), compact queued input previews, and context-sensitive footer hints (#1148).

### Changed

- Harden Vertex/Gemini `GenerateContent` projection and HTTP 4xx diagnostics, add Seatbelt `ProtectedOnly` inline interpreter code allowlist support, improve multiline `Bash` title summaries, and resolve TUI confirmation keypress leakage and queue/status consistency (#1145).
- Update Cargo and GitHub Actions dependencies in batch Dependabot maintenance (#1149).

## [0.1.5] - 2026-11-23

### Added

- Add Alan self-iteration loop foundation (#1133): redacted TUI `/issue` and `/bug` intake with tiered submission (`/mnt/github` -> `gh` CLI -> draft fallback), reference `githubfs` aP file server (`alan __fileserver-github`) and `routefs` event dispatcher (`GithubEventDispatcher`) with control-plane `TriggerAllowlist`, Quartermaster `fileserver` package kind with confined OS-sandboxed aP supervision, and bounded `alan workflow init-self-iterate` / `init-release-prep` / `evaluate-trigger` / `check-release-gate` commands plus `alan-self-iterate` and `alan-release-prep` skill packages.
- Complete Quartermaster plugin lifecycle and onboarding UX (#1134): atomic Companion Skills on non-`skill` packages, durable System Store Root Package Bindings (`SkillsOnly` knowledge-only default on `alan q install` vs explicit `alan q enable`/`disable` runtime activation, preserving existing activation on reinstall, plus `--package` and `--fileserver <id>[:ro|rw][:net]` CLI flags), foreground `ServiceManager` supervision and dual mounting of active fileservers into Root Agent and `LocalEntryService` with atomic rollback on projection collision, explicit foreground `alan workflow watch <id>` with process-group cancellation and reaping on `SIGINT`/`SIGTERM`, and single-package onboarding skills bundled in `github-fileserver`.

### Changed

- Replace raw OS sandbox backend labels in the TUI with clearer permission mode indicators (#1132).

## [0.1.4] - 2026-11-22

### Added

- Add a kernel least-privilege Tool namespace view (`MountFs::with_read_only_prefixes`) and unify builtin file tools (`read_file`, `write_file`, `edit_file`, `list_dir`, plus namespace-capable `grep`/`glob` with `Stream`/`Clone` safety, `HostDirPolicy`, and abort-safe buffered writes) over aP, enabling seamless access to `/memory`, `/proc`, `/man`, `/mnt`, and EmbeddedAlan `/mnt/<app>` mounts (#1130).
- Display prompt cache hit rate alongside accurate single-generation `ctx` occupancy in the TUI status bar and `/status` (#1129).

### Fixed

- Fix `Thinking` chronology ordering so thinking blocks render before subsequent tool actions and assistant replies, improve Tool-to-Assistant transcript spacing, and refine `Ctrl+O` Action Detail drawer formatting and collapse hints (#1129).

## [0.1.3] - 2026-11-21

### Added

- Add non-blocking fast startup with concurrent background catalog discovery and non-secret disk cache (`catalog-cache.json`), plus persistent Root `/model` selection across restarts (#1127).
- Add offline, read-only, secret-safe `alan doctor` (`--json`) and TUI `/doctor` diagnostics (#1126).

### Fixed

- Remove internal `/mnt/project-request-*` path leakage from user-facing replies (#1126).

### Changed

- Sync and archive post-merge OpenSpec specs and changes for M1–M5 (#1125).

## [0.1.2] - 2026-11-21

### Added

- Add streaming download progress bar with stage indicators, terminal-width fitting (`80`-column safe), and segmented connect/idle/total timeouts to `alan update` (#1121).
- Unify file-native model catalog resolution in `ConnectionService` and `/mnt/llm/connections/<conn>/models`, and add live Vertex AI `v1beta1/publishers/google/models` discovery with family capability inference, Express/error fallback, and custom `gemini-*` model support (#1122).

### Changed

- Polish TUI transcript UX with improved turn spacing and semantic user prompt anchor, unordered and ordered list hanging indents, inline code styling without underline, `<1s` collapsed thinking, and removal of `effort unknown` from the status bar (#1123).

## [0.1.1] - 2026-11-20

### Fixed

- Forward `model_catalog()` and `account_identity()` through `LiveSecretProvider` and `ConnectionLlmProvider`, parse Magpie `reasoning_levels` / `default_reasoning_level`, and propagate live `context_window_tokens`.
- Allow in-TUI `/login` and `/model` on primary Root Agent Processes to switch across ready generation profiles (`<profile>::<model>`), track `reload_epoch`, and clear stale confirmed bindings on `ResetSession`.
- Expose multi-profile models in `/model`, support fuzzy filtering by profile/provider, and parse `max` and `ultra` reasoning effort levels.
- Fix Google Cloud Vertex AI authentication in CLI and TUI by verifying token validity before skipping ADC login, streaming `gcloud auth application-default login --quiet` URLs in the TUI, prioritizing `gcloud config get-value project`, offering Vertex AI Express API key fallback when `gcloud` is unavailable, wiring Host `auth.json` into Vertex runtime clients, and consolidating CLI/TUI connection setup atomically.

### Added

- Automatic First-Run Setup Wizard on unconfigured interactive launches (`Esc` to skip, `ALAN_NO_SETUP=1` to disable).
- In-TUI `/settings` menu with two-step reset confirmation (defaulting to Cancel) and immediate First-Run Setup handoff, plus `alan reset [--all] [--yes]`.

## [0.1.0] - 2026-02-28

### Added

- Initial public project scaffolding and runtime architecture baseline.
