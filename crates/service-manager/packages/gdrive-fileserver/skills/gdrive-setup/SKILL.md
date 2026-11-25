---
name: gdrive-setup
description: |
  Guide operator setup, Host-owned Google Drive OAuth scoping, and verification of the
  `gdrive-fileserver` Quartermaster package (`/mnt/gdrive`).

  Use this when:
  - Installing or enabling `gdrive-fileserver` (`read-only` default vs `read-write` with `drive.file`)
  - Troubleshooting Drive OAuth scopes, quota limits, or `/mnt/gdrive/status` diagnostics
metadata:
  short-description: Guided setup for gdrive-fileserver and Host-owned Drive OAuth
  tags: [gdrive, google-drive, quartermaster, setup, fileserver, oauth]
capabilities:
  required_tools: [read_file]
---

# Google Drive File-Server Setup Guide (`gdrive-setup`)

Companion Skill bundled with `gdrive-fileserver`. Reading this skill grants **no**
runtime, network, credential, or mount authority: the operator must explicitly run
activation commands in their terminal.

## 1. Inspect Package & Host OAuth Prerequisites

Ask the operator to verify installation state:

```bash
alan q inspect gdrive-fileserver
```

Expected defaults right after `alan q install`:
- `export_kind: fileserver`
- `root_binding: skills_only`
- `network_domains: www.googleapis.com, oauth2.googleapis.com`
- `companion_skills: gdrive-setup, gdrive-research`

Drive OAuth credentials live exclusively in Host Store (`AuthStorage`), defaulting to
least-privilege `https://www.googleapis.com/auth/drive.readonly`. Uploading via
`uploads/new` additionally requires explicit `https://www.googleapis.com/auth/drive.file`
scope and a `ReadWrite` mount. Raw OAuth tokens never enter System Store or `/mnt/gdrive`.

## 2. Activate `/mnt/gdrive` Explicitly

Choose the smallest capability level needed:

- **Read-only inspection (`/mnt/gdrive` read, search, and Workspace export)**:
  ```bash
  alan q enable gdrive-fileserver --allow-network
  ```
- **Read-write (`ctl` sync/pin + `uploads/new` when `drive.file` scope is granted)**:
  ```bash
  alan q enable gdrive-fileserver --access read-write --allow-network
  ```
- **Single-invocation override without persisting global Root activation**:
  ```bash
  alan --fileserver gdrive-fileserver:ro:net
  ```

To disable runtime activation later while keeping Companion Skills:

```bash
alan q disable gdrive-fileserver --keep-skills
```

## 3. Verify Runtime Health Safely

Once mounted inside Alan, read `/mnt/gdrive/status` (read-only JSON) to verify:
- `auth.authenticated` and `auth.scopes` (`has_file_write_scope` is `false` under `drive.readonly`)
- `cursor`, `quota`, and `rate_limit` counters
- `last_error` (always token-redacted)
