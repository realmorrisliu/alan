## Why

alan's interactive terminal experience is currently split across the Rust `alan`
binary and a separate Bun/TypeScript/Ink TUI executable. That split now creates
both product and distribution problems:

- the primary terminal experience lags far behind Codex's Rust TUI interaction
  model;
- macOS packaging, signing, and local install paths must carry a second
  `alan-tui` executable with its own runtime constraints;
- `alan chat`, `alan ask`, bare `alan`, and `ALAN_TUI_PATH` create multiple
  overlapping entrypoints for what should be one product surface;
- the existing TypeScript TUI UI and interaction model are not a foundation we
  want to evolve.

alan should make the Rust terminal UI the primary default experience inside the
only shipped `alan` binary.

## What Changes

- Add a Rust TUI as a workspace crate consumed by the existing `alan` binary.
  The crate may be independently organized and tested, but it MUST NOT ship a
  separate `alan-tui` binary.
- Change bare `alan` with no subcommand to launch the Rust TUI.
- **BREAKING**: remove `alan chat` and `alan ask` as public commands.
- **BREAKING**: delete the Bun/TypeScript/Ink TUI and remove all production
  fallback paths to it, including `ALAN_TUI_PATH`.
- Base the first Rust TUI version on Codex's terminal interaction model:
  crossterm/ratatui rendering, inline viewport, terminal scrollback transcript,
  bottom composer, typed history cells, pending approval/input surfaces,
  resize reflow, and terminal-focused snapshot/vt100 verification.
- Keep the TUI daemon-backed for V1: the UI owns terminal interaction and
  presentation, while sessions, tools, runtime execution, and persistence stay
  behind the existing alan daemon/session APIs.
- Preserve bare-`alan` startup ergonomics: local daemon health check,
  auto-start/readiness wait when needed, existing daemon reuse, and explicit
  remote daemon override behavior before any session API call.
- Update release packaging, app install, Homebrew cask expectations, and Apple
  shell launch paths so the only embedded and linked command-line executable is
  `alan`.
- Fold any active or future app-distribution OpenSpec artifacts and release
  scripts into the single-binary contract during implementation: any requirement
  that embeds, signs, validates, links, or installs `alan-tui` is superseded by
  this change.

## Capabilities

### New Capabilities

- `rust-inline-tui`: defines the Rust terminal UI as the default bare-`alan`
  experience, embedded in the only shipped `alan` binary, with a Codex-like
  terminal interaction baseline and daemon-backed session contract.

### Modified Capabilities

- `daemon-api-contract`: replace TypeScript-TUI-specific generated helper and
  drift-check requirements with language-neutral/Rust-client requirements that
  support the new Rust TUI without making TypeScript the authoritative TUI
  protocol surface.
- `macos-shell-build-test-contract`: require build, packaging, install, and
  Apple shell checks to enforce the single `alan` binary contract and prevent
  reintroducing the TypeScript TUI, `alan-tui`, or legacy command entrypoints.

## Impact

- New Rust TUI crate and tests:
  - `crates/tui/**`
  - `Cargo.toml`
  - Rust terminal snapshot/vt100-style test fixtures
- CLI entrypoint changes:
  - `crates/alan/src/main.rs`
  - `crates/alan/src/cli/mod.rs`
  - removal of `crates/alan/src/cli/chat.rs`
  - removal of `crates/alan/src/cli/ask.rs`
- TypeScript TUI deletion:
  - `clients/tui/**`
  - Bun/Ink-specific scripts, generated client files, package metadata, and
    tests used only by the deleted TUI
- Packaging and install changes:
  - `justfile`
  - `scripts/install.sh`
  - `scripts/assemble-release-app.sh`
  - `scripts/validate-release-app.sh`
  - release signing/notarization helpers
  - Homebrew cask metadata or future cask contract text
- Apple shell integration:
  - shell launch commands and contract checks that currently expect `alan chat`
    or `alan-tui`
- Documentation:
  - `README.md`
  - `AGENTS.md`
  - `clients/apple/README.md`
  - related OpenSpec changes or release docs that still describe `alan-tui`
