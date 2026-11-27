# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.11] - 2026-11-27

### Added

- Add Project Ledger, named workstreams, in-instance async child workstreams, `routefs` cross-stream handoff, TUI `/streams` / `/focus` / `/root` controls, workflow inbox digests, and stateful cron streams (`#1167`, `#1176`, `#1177`).
- Add personal capability extensions: community skill intake, native CoW workspaces (`clonefile`/`FICLONE`) + `/mnt/branch` + Intent Provenance, executable `alan-arena` behind fail-closed OS sandbox confinement, governed tools, and pure-Rust `gdrivefs` aP file server (`/mnt/gdrive`) (`#1169`, `#1177`).
- Add zero-root macOS loopback NFSv3 bridge (`macos_nfs_reification`) for read-only aP namespace reification under Seatbelt (`#1180`).
- Add first-class Decisions API (`DecisionRequest`/`DecisionResponse` with `Predicate`, `Choice`, and `Score` primitives) across OpenRouter, Google Gemini (`/v1beta/decisions`), OpenAI (`/v1/decisions`), TypeSafe, Magpie, and `FallbackDecisionEvaluator`, plus `llmfs` `connections/<conn>/decide` and independent `max_decisions` metering (`#1183`).
- Add System 1 smart approval (`ConfinedSmart` / `--smart` / `rw·smart` / `Shift+Tab` 3-way cycle) with a 5-question parallel Rubric, bounded System 2 fallback, bounded task budget auto-extension, and live `Ctrl+T` telemetry (`#1184`).
- Upgrade the TUI with a high-density 2-row status bar below the composer, live task budget projection (`machine/ui/budget`), `Ctrl+T` / `Ctrl+O (s)` Status Panel, modal-only SGR mouse capture preserving native scrollback/copy, and OSC 8 hyperlinks (`#1182`).
- Add capability-probed Kitty Graphics Protocol and iTerm2 `OSC 1337` inline thumbnail rendering for pasted and tool image attachments (`#1185`).
- Add `--no-resume-update` CLI flag and `ALAN_NO_RESUME` environment parity (`#1174`).
- Migrate the ChatGPT provider to Sign in with ChatGPT (`SIWC`) (`#1181`) and redesign `alanworks.app` as a Plan 9 archive site on Solid 2 + Vite 8 SSG (`#1186`).

### Fixed

- Fix one-shot `--grant`/`--cwd` namespace resolution, grant-root trailing slash handling, `alan doctor` version reporting, `read_file` paging, Gemini `MessageRole::Context` system instruction injection, and `partialArgs` object markers (`#1175`, `#1179`).
- Fix high-context `>1 MiB` `llmfs` generation request failures (raising `GenData` limit to `16 MiB` with `stream_error:payload_too_large` `/compact` hints and expanded Vertex finish-reason mapping), confine `CARGO_TARGET_DIR` inside Seatbelt/Landlock workspaces, submit `/quit` on a single `Enter`, raise `BashTool` timeouts (`180s` default / `600s` max with float parsing), format 0-match `Grep` summaries as `0 rows`, and add seamless Tier-2 `Uncalibrated` `--smart` fallback (`#1182`, `#1188`).

## [0.1.10] - 2026-11-24

### Fixed

- Fix `githubfs` latency and redundant GitHub CLI requests by skipping `computed_bytes()` on directory `stat()`, reading `IssuesDir`/`PullsDir` cache-first for synced repos while preserving on-demand queries for unsynced repos, caching `gh auth token` with single retry invalidation on unauthorized responses, scoping sync/directory listings to `state=open`, and fetching issues and pulls concurrently (#1162).
- Fix compaction pressure evaluation, prompt framing, and failure handling by driving automatic compaction strictly from token utilization when `context_window_tokens > 0`, framing compaction and memory-flush inputs as tool-free text with protected trailing directives and raised output token budgets (`8192` / `4096` / `2048`), retrying empty compaction summaries inside the generation loop, keeping manual and soft pre-turn compaction failures non-destructive, and preserving the latest user prompt in degraded fallback summaries (#1163).

## [0.1.9] - 2026-11-24

### Fixed

- Fix TUI startup screen clearing on interactive non-`dumb` terminals without purging scrollback, and preserve committed transcript lines across partial upward inline viewport expansions (#1159).
- Fix `githubfs` on-demand read-only repository queries without writing `/mnt/github/ctl`, surface active fileserver permissions and descriptions across `alan-agent-engine` and `alan-service-manager`, and clarify `Activate`/`Upgrade` scope tags in `request_service` confirmations (#1160).

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
