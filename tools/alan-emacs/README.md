# alan-emacs

A small personal Emacs distribution, built from vanilla Emacs upward.

This directory is the source distribution used by `alan emacs install`.
The installer copies this tree into Alan-managed user data and links the user's
selected Emacs config entry to that installed copy. User config should not point
directly at this source checkout in normal use.

## Commands

```bash
alan emacs status
alan emacs install
alan emacs doctor
alan emacs uninstall
```

`alan emacs install` materializes this distribution under:

```text
~/.local/share/alan/emacs/current
```

Then it selects exactly one Emacs config entry from the user's actual Emacs
behavior and filesystem state. It refuses to overwrite non-Alan-owned config.

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
```

Machine-local overrides can go in `alan-local.el`. That file is ignored by git.

## First Principles

- Start from default Emacs.
- Prefer built-in packages until a real daily workflow proves a dependency is
  worth adding.
- Keep modules small and named by workflow.
- Keep generated state outside the repo where practical.
- Keep install and uninstall state owned by `alan emacs`.
