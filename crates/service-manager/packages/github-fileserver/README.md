# GitHub File-Server Package (`github-fileserver`)

Reference Quartermaster (`q`) `fileserver` package exporting GitHub repositories,
issues, pull requests, control verbs, and an append-only NDJSON event stream over
`alan-ap` (`/srv/github` -> `/mnt/github`).

## Zero Ambient Authority & Credential Isolation

Installing `github-fileserver` via `alan q install` records only an immutable
package revision in Package Service:

- **No OS process** is started at install time.
- **No `/srv/github` handle or `/mnt/github` mount** is created automatically.
- **No GitHub credentials** are stored inside the package or System Store.

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

## Install

```bash
alan q install crates/service-manager/packages/github-fileserver
```
