# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Read [AGENTS.md](AGENTS.md) for the full agent guide (architecture, config, skills, and product
language). This file is the short operational summary plus the rules that are easy to get wrong.

## Critical Workflow Rules

- **OpenSpec owns all specs and design docs.** Change proposals, design docs, task lists, and spec deltas go in `openspec/changes/<change-id>/`; merged long-lived contracts live in `openspec/specs/`. Do NOT create spec files under `docs/superpowers/specs/`, `docs/spec/`, or `plans/` — this overrides any default workflow that writes design docs elsewhere. Completed changes are archived to `openspec/changes/archive/YYYY-MM-DD-<change-id>/`.
- **macOS app testing targets the dev channel only**: `Alan Dev.app`, bundle `app.alanworks.macos.dev`, CLI `alan-dev`, state `~/.alan-dev` (`just install-dev`). Never launch, quit, or install over the user's stable `Alan.app` / `~/.alan` unless explicitly asked — it is their live work environment.
- **After Rust changes**: run `just verify` (fmt + lint + test + mock smoke). `just verify-full` adds an E2E pass that needs real LLM config in `~/.alan`.
- New/edited Rust tests follow `openspec/specs/rust-test-placement-contract/spec.md`: choose inline unit tests, extracted white-box test files, or crate-level integration tests deliberately.
- Branch from `main`; conventional-style commit messages recommended (for example,
  `fix(agent-engine): preserve machine state when a turn fails`).

## Commands

```bash
just check        # fmt + clippy + test (run before committing)
just verify       # fmt + lint + test + mock smoke (after Rust changes)
just test         # cargo test --workspace
just lint         # clippy with -D warnings
just smoke        # CI-safe mock smoke test, no LLM needed

cargo test -p alan-agent-engine                # single crate
cargo test -p alan-agent-engine test_name     # single test
cargo test -p alan-llm --features mock   # with MockLlmProvider

# macOS shell (Swift) — script-driven, no plain xcodebuild test target
just apple-shell-focused-tests       # focused shell tests without Ghostty artifacts
just apple-shell-ui-smoke            # UI smoke against installed Alan Dev app
just apple-shell-ghostty-integration # needs local Ghostty artifacts prepared
just install-dev                     # build + install Alan Dev.app locally
```

## Architecture

Alan models every Agent Process as an **AI Turing Machine**: LLM generation is the transition
function, the `Tape` is machine state, and Tools are side effects. Process owns lifecycle and
identity; Agent Machine owns tape and transition-local state; AgentFS owns IO and control;
rollout/checkpoint files own execution evidence; Memory Stores and handoff files own continuity.
Agent definitions on disk configure an Agent Executable but do not create a second lifecycle owner.

### Rust workspace (`crates/`)

Dependency order, bottom-up:

- `ap` (`alan-ap`) — the file-service protocol (the 9P analog): `FileServer` trait, fids, byte/offset streams, wire `Request`/`Response`, in-process transport. Layer 0; no other Alan crate depends below it.
- `kernel` (`alan-kernel`) — the Plan 9 substrate: namespace engine, process table, `/proc`, `/srv`. Depends only on `alan-ap`.
- `agent-protocol` (`alan-agent-protocol`) — the retained Event/Op execution alphabet used by the
  Agent Execution Engine and AgentFS projections; it is not a client/server transport.
- `llm` — `LlmProvider` trait + adapters (Anthropic, OpenAI Responses/Chat, ChatGPT managed, Gemini, OpenRouter). `MockLlmProvider` behind the `mock` feature.
- `agent-engine` (`alan-agent-engine`, formerly `runtime`/`alan-runtime`) — the machine: agent loop, turn execution, tool orchestration, policy engine, compaction, memory, skill system, prompt assembly. Provider-agnostic and hosting-agnostic; domain concerns stay in outer crates.
- `tools` — built-in tool implementations (read/write/edit/bash/grep/glob/list_dir) in layered profiles.
- `tui` — file-backed Ratatui renderer and input loop over AgentFS and `/proc`.
- `alan` — the direct CLI host and linked TUI entry point.

Tool governance is two-stage: `PolicyEngine` (`allow | escalate | deny`, policy from the AgentRoot chain or builtin profiles) then the `workspace_path_guard` execution guard (workspace containment, protects `.git`/`.alan`/`.agents`; not a strict OS sandbox). Escalations surface as recoverable `Yield` events — there is no session-wide approval cache.

Skills are Markdown packages with YAML frontmatter, resolved from built-ins, `~/.agents/skills/`, agent-root `skills/` dirs, and workspace equivalents; contract in `openspec/specs/skill-system-contract/spec.md`.

### macOS client (`clients/apple/alan-macos`)

SwiftUI + AppKit terminal workspace with Ghostty-backed panes. Its current owners are shell state,
terminal runtime, local shell control, updater, and privileged-helper integration. It has no Agent
Console, Alan API client, or decided Alan OS attachment boundary. Tests are script-based under
`clients/apple/scripts/`.

UI work is governed by `openspec/specs/macos-shell-ui-ux-conformance/spec.md` (terminal-first, Arc-like space/tab sidebar, light-mode-first native materials, no dashboard/card composition, no implementation jargon in default chrome) plus the design context section of AGENTS.md. Visual changes are reviewed against screenshots.

## Configuration Pointers

- Agent config lives in an AgentRoot such as `~/.alan/agents/default/agent.toml` (or the path named
  by `ALAN_CONFIG_PATH`). Stable and dev roots are host-private backing storage, not Alan OS path or
  format contracts.
- Provider/model setup is connection-profile driven (`~/.alan/connections.toml` and direct
  `alan connection …` commands); secrets live in the owning host secret store, never in
  `agent.toml`. Do not add inline `*_api_key` or `*_base_url` fields to user-facing examples.
- Rust style: Edition 2024, rustfmt 100-char width, clippy thresholds in `clippy.toml`; `tracing` instead of `println!`; `anyhow` in apps, `thiserror` in libs.
