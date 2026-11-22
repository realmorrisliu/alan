# Disposition — 2026-11-22

## Status: Implemented on `feat/enable-self-iteration-loop` (Pending PR Review & CI)

This OpenSpec change defines Alan's staged self-iteration loop across four
progressive milestones, all implemented and locally verified on
`feat/enable-self-iteration-loop`:

- **Stage 1 (`feat/si-stage1-issue`)**: Sanitized `/issue` (`/bug`) intake inside
  the file-backed TUI (`crates/tui/src/file_backed/issue_flow.rs`,
  `diagnostics.rs`, `crates/alan/src/foreground/issue_submit.rs`, and real-PTY
  `pty_e2e_issue.rs`).
- **Stage 2 (`feat/si-stage2-4-workflows`)**: Self-iteration Quartermaster `skill`
  (`crates/service-manager/packages/alan-self-iterate`) and bounded
  `alan workflow init-self-iterate` / `evaluate-trigger`
  (`crates/alan/src/cli/workflow/{manifest.rs,self_iterate.rs}`) with
  control-plane actor, repository, and label (`alan:auto`) allowlists.
- **Stage 3 (`feat/si-stage3-fileserver`, `feat/si-stage3-githubfs`, integrated `alan __fileserver-github`)**:
  Quartermaster fifth package kind (`fileserver`) with confined external `aP`
  supervision (`crates/service-manager/src/{package/,fileserver_service.rs}`),
  reference `githubfs` file server (`/srv/github` mounted at `/mnt/github`,
  `crates/service-manager/packages/github-fileserver/`,
  `crates/alan/src/cli/fileserver_github.rs`), and `routefs` rule-routed
  subagent dispatch (`GithubEventDispatcher`) with strict control-plane
  allowlists and loop guards.
- **Stage 4 (`feat/si-stage2-4-workflows`)**: Human-gated release preparation
  (`crates/service-manager/packages/alan-release-prep`,
  `alan workflow init-release-prep` / `check-release-gate`), preserving
  mandatory human confirmation before merge, tag publication, and binary release
  publishing, followed by local `alan update`.

## Delivery policy

- Cutting `v0.1.5` is intentionally held until PR review, 16/16 CI checks, merge
  to `main`, and post-merge spec sync/archive complete, followed by an explicit
  Stage 4 human-gated release PR.
- No stage or requirement in this change is claimed as shipped on `main` until
  PR review (including Codex review), 16/16 CI checks, and post-merge OpenSpec
  sync/archive pass.
