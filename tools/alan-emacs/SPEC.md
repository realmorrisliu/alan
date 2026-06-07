# alan-emacs Spec

## Purpose

`alan-emacs` is a small personal Emacs distribution inside the Alan monorepo. It
starts from vanilla Emacs and grows through explicit, reviewable configuration
modules.

The first milestone is not an Alan for macOS integration. It is a repeatable
local workflow:

1. Edit `tools/alan-emacs`.
2. Run one command.
3. Use the updated configuration from the system Emacs config location.

## Requirements

### Source-Owned Configuration

The `tools/alan-emacs` directory owns `early-init.el`, `init.el`, and the
`lisp/alan-*.el` modules. The active system Emacs configuration must point at
this directory
instead of being copied by hand.

### Symlink Installation

The installer must prefer:

```text
~/.config/emacs -> /path/to/alan/tools/alan-emacs
```

When `XDG_CONFIG_HOME` is set, the target must be:

```text
$XDG_CONFIG_HOME/emacs
```

Existing user configuration must not be overwritten silently. Installation
backs it up before replacing the target with a symlink.

### One-Key Update

`just update` must verify that the system config target points at this
repository and then load the configuration in batch mode. Because installation
uses a symlink, normal source edits are already visible to Emacs; update is the
health gate.

### Rollback

The tool must keep backups of pre-existing config targets and provide a rollback
command that restores the latest backup by default.

### Vanilla First

The initial configuration must use built-in Emacs functionality only. Third-party
packages, package locks, LSP servers, tree-sitter grammars, and Alan bridge
features are follow-up decisions.

## Non-Goals

- Do not change Alan for macOS.
- Do not add an Alan bridge.
- Do not fork, patch, or bundle GNU Emacs.
- Do not replicate Spacemacs, Doom, or a layer framework.
- Do not automatically modify shell startup files or PATH.
- Do not send buffer contents to another process automatically.

## Open Questions

- Which editing packages are worth adding after the vanilla baseline is stable?
- Should future third-party package state live under XDG cache, XDG data, or a
  lockfile-managed directory?
- What personal workflows justify an Alan bridge later?
