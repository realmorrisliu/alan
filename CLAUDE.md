# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Read [AGENTS.md](AGENTS.md) for the full agent guide (architecture detail, HTTP API, config, skills). This file is the short operational summary plus the rules that are easy to get wrong.

## Critical Workflow Rules

- **OpenSpec owns all specs and design docs.** Change proposals, design docs, task lists, and spec deltas go in `openspec/changes/<change-id>/`; merged long-lived contracts live in `openspec/specs/`. Do NOT create spec files under `docs/superpowers/specs/`, `docs/spec/`, or `plans/` — this overrides any default workflow that writes design docs elsewhere. Completed changes are archived to `openspec/changes/archive/YYYY-MM-DD-<change-id>/`.
- **macOS app testing targets the dev channel only**: `Alan Dev.app`, bundle `app.alanworks.macos.dev`, CLI `alan-dev`, state `~/.alan-dev` (`just install-dev`). Never launch, quit, or install over the user's stable `Alan.app` / `~/.alan` unless explicitly asked — it is their live work environment.
- **After Rust changes**: run `just verify` (fmt + lint + test + mock smoke). `just verify-full` adds an E2E pass that needs real LLM config in `~/.alan`.
- New/edited Rust tests follow `openspec/specs/rust-test-placement-contract/spec.md`: choose inline unit tests, extracted white-box test files, or crate-level integration tests deliberately.
- Branch from `main`; conventional-style commit messages recommended (e.g. `fix(daemon): rollback staged state on persist failure`).

## Commands

```bash
just check        # fmt + clippy + test (run before committing)
just verify       # fmt + lint + test + mock smoke (after Rust changes)
just test         # cargo test --workspace
just lint         # clippy with -D warnings
just serve        # run the daemon (cargo run -p alan -- daemon start)
just smoke        # CI-safe mock smoke test, no LLM needed

cargo test -p alan-runtime                # single crate
cargo test -p alan-runtime test_name     # single test
cargo test -p alan-llm --features mock   # with MockLlmProvider

# macOS shell (Swift) — script-driven, no plain xcodebuild test target
just apple-shell-focused-tests       # focused shell tests without Ghostty artifacts
just apple-shell-ui-smoke            # UI smoke against installed Alan Dev app
just apple-shell-ghostty-integration # needs local Ghostty artifacts prepared
just install-dev                     # build + install Alan Dev.app locally
```

## Architecture

Alan models agents as an **AI Turing Machine**: LLM generation is the transition function, the `Tape` (messages/context/summary) is the state, tools are the side effects. Hosting wraps that computation model in four layers: `AgentRoot` (on-disk definition: `agent.toml`, `persona/`, `skills/`, `policy.yaml`) → `Workspace` (persistent identity/memory) → `AgentInstance` (running process) → `Session` (bounded execution with tape + JSONL rollout).

### Rust workspace (`crates/`)

Dependency order, bottom-up:

- `protocol` — the "alphabet": `Event`/`EventEnvelope` (output) and `Op`/`Submission` (input). All transports speak this.
- `llm` — `LlmProvider` trait + adapters (Anthropic, OpenAI Responses/Chat, ChatGPT managed, Gemini, OpenRouter). `MockLlmProvider` behind the `mock` feature.
- `runtime` — the machine: agent loop, turn execution, tool orchestration, policy engine, compaction, memory, skill system, prompt assembly. Provider-agnostic and hosting-agnostic; domain concerns stay in outer crates.
- `tools` — built-in tool implementations (read/write/edit/bash/grep/glob/list_dir) in layered profiles.
- `tui` — Ratatui inline terminal UI talking to the daemon.
- `alan` — the binary: clap CLI + Axum daemon (REST + WebSocket + NDJSON event streams). Route ownership contract: `crates/alan/src/daemon/api_contract.rs`.

Tool governance is two-stage: `PolicyEngine` (`allow | escalate | deny`, policy from the AgentRoot chain or builtin profiles) then the `workspace_path_guard` execution guard (workspace containment, protects `.git`/`.alan`/`.agents`; not a strict OS sandbox). Escalations surface as recoverable `Yield` events — there is no session-wide approval cache.

Skills are Markdown packages with YAML frontmatter, resolved from built-ins, `~/.agents/skills/`, agent-root `skills/` dirs, and workspace equivalents; contract in `openspec/specs/skill-system-contract/spec.md`.

### macOS client (`clients/apple/alan-macos`)

SwiftUI + AppKit terminal workspace (Ghostty-backed panes). Structure: `App/` (lifecycle), `Models/Shell/` (snapshot state), `Controllers/`, `Views/Shell/`, `Services/` (terminal runtime, daemon client), `Support/` (design tokens in `ShellDesignTokens.swift`, sidebar presentation/layout helpers). Tests are script-based under `clients/apple/scripts/`.

UI work is governed by `openspec/specs/macos-shell-ui-ux-conformance/spec.md` (terminal-first, Arc-like space/tab sidebar, light-mode-first native materials, no dashboard/card composition, no implementation jargon in default chrome) plus the design context section of AGENTS.md. Visual changes are reviewed against screenshots.

## Configuration Pointers

- Host daemon/client settings: `~/.alan/host.toml`; agent config: `~/.alan/agents/<name>/agent.toml` (or `ALAN_CONFIG_PATH`).
- Provider/model setup is connection-profile driven (`~/.alan/connections.toml`, `alan connection …`, `/api/v1/connections/*`); secrets live in the host secret store, never in `agent.toml`. Don't add inline `*_api_key`/`*_base_url` fields to user-facing examples.
- Rust style: Edition 2024, rustfmt 100-char width, clippy thresholds in `clippy.toml`; `tracing` instead of `println!`; `anyhow` in apps, `thiserror` in libs.
