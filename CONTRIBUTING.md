# Contributing to alan

Thanks for your interest in contributing to alan.

## Before You Start

- Read [AGENTS.md](AGENTS.md) for project architecture and coding conventions.
- Search existing issues and pull requests before opening a new one.
- For security reports, do **not** open a public issue; follow [SECURITY.md](SECURITY.md).

## Development Setup

1. Install Rust stable (Edition 2024 compatible).
2. Clone the repository and enter the project root.
3. Run baseline checks:

```bash
just check
```

If `just` is not installed, use:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Rust TUI changes should also run:

```bash
cargo test -p alan-terminal-ui
```

## Branch and Commit Conventions

- Create a feature/fix branch from `main`.
- Prefer focused commits with clear intent.
- Conventional-style commit messages are recommended, for example:
  - `feat(runtime): add queued input cap for broker drain`
  - `fix(agent-engine): preserve machine state when a turn fails`

## Pull Request Expectations

A pull request should include:

- Clear problem statement and scope.
- Behavior change summary.
- Test coverage for new logic and regressions.
- Validation commands and results.
- Backward-compatibility or migration notes when relevant.

Use the PR template and keep the change set reviewable.

## Review Process

- Maintainers may request changes for correctness, safety, or maintainability.
- Keep discussion on the PR thread and resolve conversations explicitly.
- Do not self-merge until required checks and review expectations are satisfied.

## Good First Contributions

Look for issues labeled `good first issue` or `help wanted`.
