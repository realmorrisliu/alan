# alan-emacs Spec

## Purpose

`alan-emacs` is a small personal Emacs distribution inside the Alan monorepo. It
starts from vanilla Emacs and grows through explicit, reviewable configuration
modules.

The first milestone is not an Alan-to-Emacs bridge. It is a repeatable local
workflow:

1. Edit `tools/alan-emacs`.
2. Run `alan emacs install`.
3. Use the installed Alan-managed copy from ordinary `emacs`.

## Requirements

### Source-Owned Configuration

The `tools/alan-emacs` directory owns `early-init.el`, `init.el`, and the
`lisp/alan-*.el` modules.

### Alan-Managed Installation

Installation is owned by the Alan CLI:

```bash
alan emacs install
```

The installer materializes this source distribution into Alan-managed user data:

```text
~/.local/share/alan/emacs/current
```

The user's active Emacs config entry must point at that installed copy, not at
the source checkout.

### Detector-Driven Config Entry

The installer must choose exactly one config entry from:

```text
~/.emacs.d
$XDG_CONFIG_HOME/emacs, or ~/.config/emacs when XDG_CONFIG_HOME is unset
```

It must reuse Alan-owned state, accept a single empty candidate, or probe the
installed `emacs` default user config directory. It must refuse non-empty
non-Alan-owned config entries.

### Health And Removal

`alan emacs status` reports source, install, and config ownership state.
`alan emacs doctor` checks Emacs availability, install integrity, and observed
daemon state without controlling services. `alan emacs uninstall` removes only
Alan-owned config links and managed install data.

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
- Do not link user configuration directly to `tools/alan-emacs` in normal use.
- Do not wrap Homebrew, launchctl, systemctl, or Emacs daemon lifecycle commands.
- Do not send buffer contents to another process automatically.

## Open Questions

- Which editing packages are worth adding after the vanilla baseline is stable?
- Should future third-party package state live under XDG cache, XDG data, or a
  lockfile-managed directory?
- What personal workflows justify an Alan bridge later?
