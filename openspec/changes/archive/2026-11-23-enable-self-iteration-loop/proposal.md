# Proposal

## Why

Turning daily Alan usage friction into verified fixes currently requires manual
context capture, manual issue filing, manual worktree and pull request
iteration, and manual release preparation.

`alan9` already provides the Plan 9 building blocks needed for file-native
self-iteration: byte-framed `aP` wire transport (`crates/ap`), inspectable
Plumber-style message routing (`crates/routefs`), the Quartermaster `q`
extension hub (`crates/service-manager/src/package`), and bounded non-interactive
workflow execution (`crates/alan/src/cli/workflow`). This change connects those
primitives into a progressive self-iteration loop without weakening the OS
confinement boundary (ADR-0060, ADR-0061), Host credential ownership
(ADR-0051), explicit namespace install sources (ADR-0052), or foreground
invocation lifecycles (ADR-0056).

## What Changes

- **Stage 1 — Sanitized TUI issue intake (`rust-inline-tui`)**: Add `/issue`
  (alias `/bug`) in the file-backed TUI to assemble a strictly redacted
  diagnostic draft (Alan version, OS/arch, active provider/model label,
  permission posture, latest error/notice summary; no raw transcript, workspace
  code, or secrets), display an editable confirmation preview, and submit via
  mounted `/mnt/github` when available, authenticated `gh` CLI otherwise, or a
  prefilled GitHub web URL fallback.
- **Stage 2 — Self-iteration `skill` and bounded `alan workflow`**: Exercise the
  full Issue -> isolated worktree -> failing reproduction test -> fix ->
  `just quality` & workspace tests -> pull request -> CI/review iteration cycle
  using existing Quartermaster `skill` packaging and `alan workflow`
  (`alan -p --ephemeral --json --grant <id>`), strictly gated by a control-plane
  actor and label allowlist (`alan:auto`) with zero merge or release authority.
- **Stage 3 — `fileserver` extension kind and reference `githubfs` (`package-management-contract`, `file-server-plugin-contract`)**:
  Extend Quartermaster `q` with a fifth governed package kind, `fileserver`, for
  external aP-speaking File-Server Services published under `/srv/<id>` and
  explicitly mounted at `/mnt/<id>`. Specify the reference `githubfs` tree
  (`ctl`, `status`, read-only `events` aP `Stream`, `repos/<owner>/<repo>/issues/...`,
  `pulls/...`) and compose `/mnt/github/events` with `routefs` (`/mnt/route`) to
  dispatch bounded worker subagents with deduplication and loop guards.
- **Stage 4 — Human-gated release preparation (`file-server-plugin-contract`)**:
  Add a bounded release-preparation workflow that computes changes since the
  previous tag, prepares version bumps and changelog entries, and opens a
  verified release pull request, while keeping PR merge, git tag creation, and
  artifact publication strictly human-gated (followed by `alan update`).

## Capabilities

### New Capabilities

- `file-server-plugin-contract`: File-native contract for Quartermaster-installed
  external aP File-Server Services (`fileserver`), the reference `githubfs` tree
  and event stream, untrusted external input isolation, control-plane trigger
  allowlists, `routefs` subagent dispatch with loop guards, and human-gated
  release boundaries.

### Modified Capabilities

- `rust-inline-tui`: Add `/issue` (`/bug`) redacted diagnostic issue drafting,
  user confirmation gating, and tiered submission.
- `package-management-contract`: Extend Quartermaster from four package kinds to
  five (`skill`, `mcp`, `tool`, `provider`, `fileserver`) with zero authority
  granted by installation and confined external aP supervision.

## Impact

- `crates/tui`: `/issue` (`/bug`) slash command, redacted diagnostic snapshot
  collector, editable preview/confirmation prompt, and tiered transport handler.
- `crates/service-manager`: `fileserver` package manifest validation, catalog
  persistence, confined external aP service supervision, and `/srv/<id>`
  publication.
- `crates/ap`: External stdio / Unix socket framing via `WireTransportClient`
  for `fileserver` packages.
- `crates/routefs`: Rule-routed dispatch from `/mnt/github/events` into bounded
  worker ports and human review ports.
- `crates/alan/src/cli/workflow`: Declarative workflow manifest templates and
  opt-in polling execution without introducing silent always-on processes behind
  bare `alan`.
- Host adapters: GitHub token resolution via `gh` / Host Store without placing
  credentials inside packages or System Store files.
