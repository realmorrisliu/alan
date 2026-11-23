# GitHub File-Server Package (`github-fileserver`)

Reference Quartermaster (`q`) `fileserver` package exporting GitHub repositories,
issues, pull requests, control verbs, and an append-only NDJSON event stream over
`alan-ap` (`/srv/github` -> `/mnt/github`), plus bundled read-only Companion Skills
under `skills/`.

## Companion Skills (`skills/`)

- `github-setup`: guides `gh` prerequisite checks, `alan q enable`, scoped `alan host grant save`, workflow registration, and `--dry-run` watcher setup.
- `self-iterate`: generic issue-to-PR self-iteration workflow (`TriggerAllowlist` + isolated worktree + failing test first + PR, no autonomous merge/release).
- `release-prep`: generic human-gated release preparation workflow (`ReleaseGatePolicy`: PR only, no autonomous merge, tag push, or publish).

## Zero Ambient Authority & Credential Isolation

Installing `github-fileserver` via `alan q install` records the immutable package
revision in Package Service and attaches its read-only Companion Skills (`skills_only`)
unless `--catalog-only` is passed:

- **No OS process** is started at install time.
- **No `/srv/github` handle or `/mnt/github` mount** is created automatically.
- **No GitHub credentials** are stored inside the package or System Store.

Runtime capabilities require explicit activation via `alan q enable`, `--fileserver`,
or `alan workflow watch`:

```bash
# Install catalog revision + attach read-only Companion Skills
alan q install crates/service-manager/packages/github-fileserver

# Or install into catalog only
alan q install --catalog-only crates/service-manager/packages/github-fileserver

# Enable /mnt/github read-only (no network) or read-write with network
alan q enable github-fileserver
alan q enable github-fileserver --access read-write --allow-network

# Revoke runtime activation while optionally keeping Companion Skills
alan q disable github-fileserver --keep-skills

# Or activate only for one isolated/ephemeral invocation
alan --ephemeral --fileserver github-fileserver:rw:net -p "Summarize open issues"
```

At runtime under explicit Service Manager supervision, `GhCliGithubBackend`
resolves auth solely through the Host `gh` adapter and never exposes tokens
into any namespace file (`status`, `events`, `meta.json`, `body.md`, `comments`,
`checks.json`, or `reviews.json`).

## Namespace Tree (`/mnt/github`)

- `ctl` (commit-on-clunk: `watch <owner/repo>`, `unwatch <owner/repo>`, `sync`)
- `status` (read-only JSON: watched repos, cursor, ETags, rate-limit budget)
- `events` (read-only append-only NDJSON `alan_ap::Stream`)
- `repos/<owner>/<repo>/issues/new` (commit-on-clunk JSON issue create -> `{number, url}`)
- `repos/<owner>/<repo>/issues/<n>/{meta.json,body.md,comments,ctl}`
- `repos/<owner>/<repo>/pulls/<n>/{meta.json,checks.json,reviews.json,ctl}` (`comment <body>` only; merge/release/tag/publish verbs fail closed)

## Explicit Foreground Event Watcher

```bash
alan workflow watch self-iterate --once --dry-run --json
alan workflow watch self-iterate --poll-secs 60
```
