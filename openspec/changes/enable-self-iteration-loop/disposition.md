# Disposition — 2026-11-22

## Status: Proposed (Active)

This OpenSpec change defines Alan's staged self-iteration loop across four
progressive milestones:

- **Stage 1**: Sanitized `/issue` (`/bug`) intake inside the file-backed TUI.
- **Stage 2**: Self-iteration Quartermaster `skill` and bounded
  `alan workflow` loop (Issue -> isolated worktree -> reproduction test ->
  fix -> quality checks -> pull request -> CI/review iteration).
- **Stage 3**: Quartermaster fifth package kind (`fileserver`) for external
  aP-speaking File-Server Services, reference `githubfs` file server (`/srv/github`
  mounted at `/mnt/github`), and `routefs` rule-routed subagent dispatch with
  strict control-plane allowlists and loop guards.
- **Stage 4**: Human-gated release preparation (`v0.1.5+` changelog/version
  bump PR drafting), preserving mandatory human confirmation before merge,
  tag publication, and binary release publishing, followed by local `alan update`.

## Delivery policy

- Cutting `v0.1.5` is intentionally held until the self-iteration loop is
  operational and verified end-to-end.
- No stage or requirement in this change is claimed as shipped until its owning
  implementation tasks, tests, security boundary checks, and current-head CI
  checks pass.
