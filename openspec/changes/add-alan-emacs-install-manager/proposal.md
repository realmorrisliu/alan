## Why

`tools/alan-emacs` now contains a small vanilla-first Emacs distribution, but
the current local setup still behaves like a developer symlink. That is not the
right user model for Alan for macOS. Users should not need to know where the
source checkout lives, pass `--init-directory`, or wire shell wrappers by hand.

Alan should distribute the Emacs environment as an Alan-owned resource and
provide a restrained `alan emacs` command surface that installs, checks, and
removes only the Alan-owned Emacs distribution state.

## What Changes

- Add an `alan emacs` CLI group with a narrow command surface:
  `status`, `install`, `doctor`, and `uninstall`.
- Treat `tools/alan-emacs` as the development source for a bundled
  `alan-emacs` distribution resource.
- Install the distribution into an Alan-managed user data location, not by
  linking the user's Emacs config directly to the source checkout.
- Programmatically select the active Emacs config entry from the user's actual
  Emacs behavior and existing filesystem state instead of hard-coding
  `~/.emacs.d` or `~/.config/emacs`.
- Ensure bare `emacs` loads Alan Emacs after install.
- Have Alan for macOS terminal panes set simple editor environment values:
  `EDITOR=emacs` and `VISUAL=emacs`.
- Keep daemon and service lifecycle management outside `alan emacs`; the command
  may report service-related observations but does not wrap `brew services`,
  `launchctl`, `systemctl`, or Emacs daemon control.

## Capabilities

### New Capabilities

- `alan-emacs-distribution`: Defines the Alan-owned Emacs distribution,
  installer, config-entry selection, status/doctor/uninstall behavior, and
  service-management boundary.

### Modified Capabilities

- `alan-app-distribution`: Alan release artifacts need to carry the
  `alan-emacs` resource alongside the CLI/app resources.
- `macos-shell-terminal-lifecycle`: macOS terminal boot environments should
  expose `EDITOR=emacs` and `VISUAL=emacs` after the distribution is installed.
- `macos-shell-build-test-contract`: Verification must cover installation
  selection, conflict handling, bare Emacs loading, and terminal editor env
  injection.

## Impact

- `crates/alan/src/main.rs` and `crates/alan/src/cli/` gain a new restrained
  `emacs` CLI command group.
- Release/dev packaging needs a stable resource location for `alan-emacs`.
- `tools/alan-emacs` remains the source copy in the monorepo, while the
  installed user copy lives under Alan-managed data.
- macOS terminal boot-profile generation needs a small editor-env addition.
- Focused Rust and Apple script tests need to cover the install/status/doctor
  paths and terminal env projection.

No first implementation should add Emacs packages, Alan bridge behavior,
Homebrew service commands, launchctl/systemctl wrappers, PATH shadowing, or shell
startup-file mutation.
