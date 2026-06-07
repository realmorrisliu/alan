# alan-emacs

A small personal Emacs distribution, built from vanilla Emacs upward.

This directory is the source of truth for the Emacs configuration. Installing
it creates a symlink from the system Emacs config directory to this directory,
so editing files here immediately changes the active configuration.

## Commands

```bash
just doctor
just install
just update
just check
just rollback
```

`just install` links:

```text
~/.config/emacs -> /path/to/alan/tools/alan-emacs
```

If `XDG_CONFIG_HOME` is set, it uses `$XDG_CONFIG_HOME/emacs`.

Existing config is moved into:

```text
~/.local/state/alan-emacs/backups/
```

`just update` verifies the symlink and loads the config with `emacs --batch`.
Since the install is symlink-based, this is a health check rather than a copy
step.

## Layout

```text
early-init.el
init.el
lisp/
  alan-core.el
  alan-ui.el
  alan-editing.el
  alan-project.el
  alan-git.el
bin/
  alan-emacs
```

Machine-local overrides can go in `alan-local.el`. That file is ignored by git.

## First Principles

- Start from default Emacs.
- Prefer built-in packages until a real daily workflow proves a dependency is
  worth adding.
- Keep modules small and named by workflow.
- Keep generated state outside the repo where practical.
- Make install and rollback boring.
