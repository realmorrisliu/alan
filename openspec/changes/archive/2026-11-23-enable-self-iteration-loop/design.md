# Design

## Context

Alan currently relies on human operators to copy errors from terminal sessions,
file GitHub issues by hand, spin up isolated git worktrees to fix bugs and
iterate against CI/reviewers, and cut releases manually. Meanwhile, `alan9`
already has the core file-native building blocks in `main`:

- `crates/ap` defines a wire-shaped `FileServer` protocol (`FileServer`,
  `Stream`, `WireTransportClient`, `export_file_server`).
- `crates/routefs` implements Plumber-inspired commit-on-clunk message routing
  and plain-text inspectable routing rules under `/mnt/route`.
- `crates/service-manager/src/package` owns Quartermaster (`q`) package
  installation, immutable revisions, and `/srv/package`.
- `crates/alan/src/cli/workflow` runs declarative, bounded one-shot agent tasks
  over `alan -p --ephemeral --json` with explicit `--grant` and `--trust`.

The goal of this design is to compose those existing mechanisms into an
everything-is-a-file self-iteration loop in four low-risk stages.

## Goals / Non-Goals

- **Goals**:
  - Ship an immediately useful Stage 1 (`/issue` / `/bug`) in the TUI without
    introducing new credential stores or external services.
  - Validate the full Issue -> worktree -> fix -> `just quality` -> PR loop via
    Stage 2 (`skill` + `alan workflow`) before freezing Stage 3's external
    `fileserver` ABI.
  - Model external integrations (starting with GitHub) as mounted `alan9`
    file trees (`githubfs`) composed with `routefs`, rather than custom ad-hoc
    RPC daemons or embedded polling threads.
  - Preserve strict OS confinement, untrusted-input isolation, and human gates on
    PR merges and releases.
- **Non-Goals**:
  - Automatically merging pull requests or publishing git tags / release
    binaries without explicit human confirmation.
  - Launching hidden standing background watchers when running bare `alan`.
  - Storing GitHub tokens or credentials inside Quartermaster packages, System
    Store manifests, or Agent-readable prompts.
  - Adding implicit host directory scanning or weakening OS sandbox boundaries.

## Decisions

### D1. Stage 1 — Sanitized `/issue` (`/bug`) intake in the file-backed TUI

When the user runs `/issue [optional title]` (or `/bug [optional title]`):

1. Alan gathers a strictly redacted diagnostic summary from in-session state
   already visible to the operator: Alan CLI version, host OS/architecture,
   active provider/model label, permission posture (`governed`, `auto-approve`,
   or `unsandboxed` + mount mode), and the latest sanitized error/notice
   summary.
2. Alan MUST NOT attach full conversation transcripts, files from mounted
   projects, environment variables, or credentials automatically. Any extra
   context requires deliberate user editing in the draft.
3. The TUI presents the editable draft and waits for explicit user confirmation
   (or cancellation, which sends nothing).
4. Once confirmed, submission follows a three-tier graceful fallback chain:
   - If `/mnt/github/repos/<owner>/<repo>/issues/new` is mounted in the
     session namespace with write access, write the JSON/frontmatter payload to
     `issues/new` and clunk the fid, reading back `{number, url}`.
   - Else if an authenticated `gh` executable is available on Host PATH, invoke
     `gh issue create` via the explicit Host adapter.
   - Otherwise, construct a bounded prefilled GitHub web URL
     (`https://github.com/<owner>/<repo>/issues/new?title=...&body=...`) so the
     operator can open it in a browser without blocking on auth setup.

### D2. Stage 2 — Validate autonomy with existing `skill` + `alan workflow` before freezing ABI

Before extending Quartermaster with a fifth package kind, Stage 2 validates the
issue-fixing loop using existing mechanisms:

- A Quartermaster-managed `skill` defines the repository engineering loop: read
  issue context as untrusted input, create an isolated git worktree off
  `origin/main`, write a failing reproduction test first, implement the minimal
  fix, run `./scripts/check-rust-source-size.sh`, `just quality`, and
  `cargo test --workspace`, push a feature branch, open a PR via `gh`, watch CI
  checks and review comments (including Codex review), resolve valid review
  findings, and stop cleanly before merging.
- A bounded `alan workflow` manifest triggers `alan -p --ephemeral --json` with
  explicit `--grant` bindings to the Alan checkout and an explicit trust
  posture.
- Only issues authored or labeled (`alan:auto`) by an allowlisted actor recorded
  in the control-plane workflow config (not read from issue text or repo files)
  are eligible for automatic dispatch.

### D3. Stage 3 — Fifth Quartermaster kind: `fileserver` over aP wire transport

`alan-package.toml` adds `kind = "fileserver"` alongside `skill`, `mcp`,
`tool`, and `provider`:

- **Zero install-time authority**: Installing via `q install <namespace-path>`
  validates the manifest and materializes immutable package bytes in Package
  Service. It starts no OS process, opens no network connections, and adds no
  mount, credential, or approval.
- **Manifest contract**: Declares service id (`[a-z0-9][a-z0-9_-]*`),
  entrypoint path within the package, wire transport (`stdio` or `unix_socket`),
  declared network domain expectations, and bounded resource/message limits.
- **Runtime supervision & mount gating**: When explicitly enabled and mounted by
  a Process or operator, Service Manager launches the executable inside the OS
  confinement boundary (with only explicitly granted network/Host adapters),
  speaks `WireTransportClient` framing (`crates/ap`), publishes its service handle
  at `/srv/<service-id>`, and enforces `ReadOnly` vs `ReadWrite` mount access
  when bound at `/mnt/<service-id>`.

### D4. Reference `githubfs` file-server tree and credential isolation

The reference `githubfs` package implements an external aP file server mounted
at `/mnt/github`:

```text
/srv/github  ──mount──▶  /mnt/github/
  ├── ctl                          # commit-on-clunk control: "watch <owner/repo>", "sync", "unwatch <owner/repo>"
  ├── status                       # read-only JSON/text: sync state, cursors, rate-limit budget
  ├── events                       # read-only append-only NDJSON aP Stream (cursor, event_id, kind, actor, repo, ref, labels, head_sha)
  └── repos/<owner>/<repo>/
      ├── issues/
      │   ├── new                  # write title/body/labels JSON -> clunk commits creation, returns number + url
      │   └── <number>/
      │       ├── meta.json        # number, title, author, state, labels, updated_at
      │       ├── body.md          # raw issue body (untrusted external text)
      │       ├── comments         # read comments list; append message + clunk to post a comment (when ReadWrite)
      │       └── ctl              # "label add <name>", "label remove <name>", "close"
      └── pulls/
          └── <number>/
              ├── meta.json        # number, title, head_ref, head_sha, base_ref, state, mergeable
              ├── checks.json      # summarized CI check runs and conclusions
              ├── reviews.json     # review summaries and unresolved review threads
              └── ctl              # "comment <body>" (destructive merge/release verbs omitted by default)
```

Credentials stay outside `githubfs` and System Store: the Host adapter resolves
GitHub credentials (for example via `gh auth token` or Host credential store)
and supplies scoped access only at runtime startup under explicit authorization.

### D5. Composing `githubfs` + `routefs` for event-driven subagent dispatch

Rather than embedding agent orchestration inside `githubfs`, continuous
automation composes two independent file servers:

1. An explicit opt-in dispatcher tails `/mnt/github/events` and writes typed
   event envelopes into `/mnt/route/send` (commit-on-clunk in `routefs`).
2. Plain-text, `cat`-inspectable rules under `/mnt/route/rules` match on
   structured envelope metadata (`repo`, `kind`, `actor`, `labels`) and route
   accepted work items to bounded worker ports or a human review inbox port.
3. **Untrusted text barrier**: Rule evaluation and subagent spawn decisions MUST
   inspect only control-plane allowlists and structured API metadata (`author`,
   `actor`, `labels`, `repo`), never instructions parsed out of `body.md` or
   `comments`.
4. **Idempotency & loop guards**:
   - Ignore events triggered by Alan's own bot/automation identity or automated
     commits on `alan/` worker branches unless a review or CI check state changes
     externally.
   - Deduplicate work by `(event_id)` and coalesce active runs by
     `(repo, issue_or_pr, head_sha)`.
   - Enforce bounded concurrent worker slots and a per-issue maximum retry cap
     (escalating to the human review port when exhausted).

### D6. Foreground lifecycle discipline (ADR-0056)

Bare `alan` remains a single-terminal foreground instance and exits cleanly when
the session ends; it MUST NOT spawn lingering standing watchers. Event-driven
GitHub monitoring runs only when explicitly launched as a scheduled
`alan workflow`, OS scheduler unit (such as `launchd`), or explicit foreground
watch command.

### D7. Stage 4 — Human-gated release preparation

When requested by an operator or allowlisted trigger after fixes land on `main`:

- A bounded release-preparation workflow inspects commits and PRs since the latest
  semver tag, bumps crate versions and changelog entries in a fresh isolated
  worktree, validates release packaging and `just quality`, and opens a release PR.
- Merging the release PR, pushing the release git tag, and publishing release
  artifacts remain strictly human-gated. Once the human merges and the release
  workflow publishes the archive, local `alan update` completes self-upgrade.

## Risks / Trade-offs

- **Prompt injection via public issues or PR comments**: Any GitHub user can
  open an issue or comment on a public repository.
  *Mitigation*: Trigger eligibility is enforced by a control-plane actor and
  label allowlist before any subagent launches; spawned fixers operate inside OS
  confinement on an isolated git worktree, treat `body.md`/`comments` strictly
  as untrusted data, and hold no merge/tag/publish credentials or verbs.
- **Runaway feedback loops (PR push -> CI/Codex comment -> agent push -> loop)**:
  *Mitigation*: Deduplication keys (`event_id`, `head_sha`), self-actor
  filtering, bounded retries per issue/PR, and hard worker concurrency limits.
- **Premature external plugin ABI lock-in**: Freezing a complex `fileserver` ABI
  before practicing the real GitHub workflow could bake in wrong abstractions.
  *Mitigation*: Stage 2 exercises the exact end-to-end engineering loop first
  using `skill` + `alan workflow`, informing Stage 3's minimal file-server ABI.
- **GitHub API rate limits during ETag polling**:
  *Mitigation*: Conditional requests (`If-None-Match` / `ETag`) tracked in
  `status` and bounded poll intervals configured via `ctl`.

## Migration Plan

1. **Additive TUI surface (Stage 1)**: `/issue` (`/bug`) works standalone via
   `gh` CLI or browser URL fallback before `githubfs` exists, then upgrades
   automatically to write `/mnt/github/.../issues/new` when mounted.
2. **Additive Quartermaster kind (Stage 3)**: Existing `skill`, `mcp`, `tool`,
   and `provider` packages keep exact identities and manifests; `fileserver`
   extends `PackageExportKind` additively without altering existing packages.
3. **Hold `v0.1.5` release**: Current `main` remains unreleased past `v0.1.4`
   until the team completes Stage 1–4 verification and triggers a Stage 4
   human-gated release.
