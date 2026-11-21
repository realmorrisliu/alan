# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
