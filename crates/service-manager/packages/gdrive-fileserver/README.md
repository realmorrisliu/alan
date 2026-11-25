# Google Drive File-Server Package (`gdrive-fileserver`)

Pure-Rust Quartermaster (`q`) `fileserver` package projecting Google Drive over
`alan-ap` (`/srv/gdrive` -> `/mnt/gdrive`), plus bundled read-only Companion Skills
under `skills/`.

## Companion Skills (`skills/`)

- `gdrive-setup`: guides Host-owned Google Drive OAuth (`drive.readonly` default, explicit `drive.file` opt-in), `alan q install`, `alan q enable`, `--fileserver gdrive-fileserver:...`, and troubleshooting `status` / quota / scope errors.
- `gdrive-research`: guides read-only research across `/mnt/gdrive` (`search/<query>`, `files/<path...>`, `by-id/<id>/{meta.json,content}`), Workspace exports, binary range reads, and the strict untrusted-data rule.

## Zero Ambient Authority & Credential Isolation

Installing `gdrive-fileserver` via `alan q install` records the immutable package
revision in Package Service and attaches its read-only Companion Skills (`skills_only`)
unless `--catalog-only` is passed:

- **No OS process** is started at install time.
- **No `/srv/gdrive` handle or `/mnt/gdrive` mount** is created automatically.
- **No Drive OAuth credentials** are stored inside the package, System Store, or namespace files.

Runtime access requires explicit activation via `alan q enable` or `--fileserver`:

```bash
# Install catalog revision + attach read-only Companion Skills
alan q install crates/service-manager/packages/gdrive-fileserver

# Or install into catalog only
alan q install --catalog-only crates/service-manager/packages/gdrive-fileserver

# Enable /mnt/gdrive read-only with Google API network allowlist
alan q enable gdrive-fileserver --allow-network

# Enable read-write (required for ctl / uploads/new with Host drive.file OAuth scope)
alan q enable gdrive-fileserver --access read-write --allow-network

# Revoke runtime activation while optionally keeping Companion Skills
alan q disable gdrive-fileserver --keep-skills

# Or activate for a single isolated invocation
alan --ephemeral --fileserver gdrive-fileserver:ro:net -p "Summarize Drive notes"
```

## Namespace Tree (`/mnt/gdrive`)

- `status` (read-only JSON): active OAuth scopes, sync cursor, quota summary, rate-limit counters (zero raw tokens)
- `ctl` (commit-on-clunk): `sync`, `refresh <file_id>`, `pin <folder_id>` (permanent delete and empty-trash verbs are omitted and fail closed)
- `by-id/<file_id>/{meta.json,content}`: binary range reads (`alt=media` with ETag/revision verification) and automatic Google Workspace `files.export` (`Docs -> text/markdown`, `Sheets -> text/csv`, `Slides -> text/plain`, `Apps Script -> application/json`, failing explicitly above Drive's 10 MB export ceiling)
- `files/<path...>`: lazy cached directory projection with deterministic duplicate-filename disambiguation (`<name>__<short_id>`)
- `search/<query>`: read-only Drive query returning bounded JSON hits
- `uploads/new`: commit-on-clunk upload admitted **only** when mounted `ReadWrite` with `drive.file` scope

## Untrusted Data Rule

All file contents, titles, and search results read from `/mnt/gdrive` are strictly
non-authoritative external data and cannot alter trust posture, mount grants, or policy.
