---
name: workspace-manager
description: |
  Manage alan workspaces using the `alan` CLI.

  Workspaces are project directories with an `.alan/` configuration directory.
  Any directory can become a workspace via `alan init`.
  All workspaces are tracked in a central registry (~/.alan/registry.json).

  Use this when:
  - User asks to create, list, or manage workspaces
  - User wants to switch between projects
  - User asks about available workspaces or their status

  alan ships this as a built-in capability package.

metadata:
  short-description: Manage workspaces via alan CLI
  tags: [workspace, project, management, init, registry]
capabilities:
  required_tools: [bash]
---

# Workspace Manager Skill

Manage alan workspaces using the `alan` CLI tool. Workspaces follow a distributed, Git-like model: any directory containing a `.alan/` subdirectory is a workspace.

## CLI Reference

### Initialize a new workspace

```bash
alan init                          # Initialize current directory
alan init --path /path/to/project  # Initialize a specific directory
alan init --name my-project        # Set a custom alias
```

This creates the `.alan/` directory structure and registers the workspace in the central registry.

### List all workspaces

```bash
alan workspace list
```

Output format:
```
ID         ALIAS                PATH
------------------------------------------------------------
a1b2c3     my-project           ~/Developer/my-project
d4e5f6     alan                 ~/Developer/Alan
```

### Show workspace details

```bash
alan workspace info <alias-or-id>
```

Shows: alias, ID, path, creation date, status, and session count.

### Register an existing workspace

```bash
alan workspace add /path/to/existing/project
alan workspace add /path/to/project --name custom-alias
```

The directory must already contain a `.alan/` subdirectory.

### Unregister a workspace

```bash
alan workspace remove <alias-or-id>
```

This only removes the workspace from the registry. It does NOT delete any files.

## Workspace Directory Structure

```
project/
├── .alan/
│   ├── state.json           # Workspace state
│   ├── context/
│   │   └── skills/          # Workspace-specific skills
│   ├── sessions/            # Session rollout files (.jsonl)
│   └── memory/
│       ├── MEMORY.md        # Persistent knowledge
│       └── YYYY-MM-DD.md    # Daily work logs
├── src/                     # User's project files
└── ...
```

## Root Workspace

The `~/.alan/` directory is the root workspace. It is initialized automatically during installation and contains the global workspace registry.

## Rules

1. **Always use the `alan` CLI** — do not manually edit `registry.json`
2. **One workspace per directory** — don't nest workspaces
3. **Management only** — this skill manages workspace metadata, not workspace content
4. **Prefer aliases** — use human-friendly aliases over IDs when talking to users
5. **Verify before removing** — confirm with the user before unregistering workspaces
