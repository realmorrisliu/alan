---
name: github-setup
description: |
  Guide guided operator setup and verification of the `github-fileserver` Quartermaster
  package (`/mnt/github`), scoped project grants, TUI `/issue` Tier 1 integration,
  self-iteration / release-prep workflows, and explicit `alan workflow watch`.

  Use this when:
  - The user asks how to configure, enable, troubleshoot, or automate GitHub workflows after `alan q install`
  - Checking whether `/mnt/github` is enabled read-only or read-write with network permission
  - Registering a new self-iteration or release-prep workflow and verifying it safely with `--dry-run`
metadata:
  short-description: Guided setup for github-fileserver, grants, and workflows
  tags: [github, quartermaster, setup, fileserver, workflow, grant]
capabilities:
  required_tools: [read_file]
---

# GitHub File-Server Setup & Automation Guide (`github-setup`)

Companion Skill bundled with `github-fileserver`. Reading this skill grants **no** runtime, network, credential, or mount authority: the operator must explicitly run every activation or grant CLI command in their terminal.

## 1. Check Current Installation & Auth State

Ask the operator to run (or inspect read-only status when permitted):

```bash
gh auth status
alan q inspect github-fileserver
```

Verify:
- `gh auth status` reports an authenticated GitHub CLI account on the Host (Alan never stores GitHub tokens in System Store or namespaces).
- `alan q inspect github-fileserver` shows `export_kind: fileserver`, `root_binding: skills_only` (default right after `alan q install`), and `companion_skills: github-setup, self-iterate, release-prep`.

## 2. Activate `/mnt/github` for Interactive Sessions

Choose the smallest capability level needed:

- **Read-only inspection (`/mnt/github` browse & `/issue` untouched)**:
  ```bash
  alan q enable github-fileserver
  ```
- **Full interactive read-write + GitHub API sync + TUI `/issue` Tier 1 (`issues/new`)**:
  ```bash
  alan q enable github-fileserver --access read-write --allow-network
  ```
- **Single-invocation override without persisting global Root activation**:
  ```bash
  alan --fileserver github-fileserver:rw:net
  ```

Restart `alan` after `alan q enable` so `ServiceManager` mounts `/mnt/github` into both the Root Agent and `LocalEntryService`. Inside `/mnt/github`, `/issue` automatically upgrades to Tier 1 (`issues/new`) when `/mnt/github` is mounted read-write.

To revoke runtime activation later:

```bash
alan q disable github-fileserver --keep-skills   # keeps Companion Skills, stops /mnt/github
alan q disable github-fileserver                 # removes Root binding completely
```

## 3. Configure Scoped Self-Iteration & Release Workflows

1. Save a scoped Host Mount grant for the target repository with branch-restricted push rules:
   ```bash
   alan host grant save repo-work /path/to/<repo> \
     --access read-write \
     --git-push-remote origin \
     --git-push-branch 'alan/*'
   ```
2. Register the bounded self-iteration workflow (`--fileserver github-fileserver:rw:net` keeps automation isolated during `--ephemeral` runs):
   ```bash
   alan workflow init-self-iterate \
     --repo <owner>/<repo> \
     --allowed-actor <github-login> \
     --grant repo-work \
     --fileserver github-fileserver:rw:net \
     --trust confined_trusted
   ```
   Optionally register human-gated release preparation:
   ```bash
   alan workflow init-release-prep \
     --repo <owner>/<repo> \
     --allowed-actor <github-login> \
     --grant repo-work \
     --fileserver github-fileserver:rw:net \
     --trust governed
   ```
3. Verify the dry-run plan and one safe poll cycle before starting continuous watch:
   ```bash
   alan workflow run self-iterate --dry-run
   alan workflow watch self-iterate --once --dry-run --json
   ```
4. Start the explicit foreground watcher when ready (Ctrl+C stops both the watcher and `github-fileserver` cleanly):
   ```bash
   alan workflow watch self-iterate --poll-secs 60
   ```
