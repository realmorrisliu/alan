## 1. OpenSpec And Scope

- [x] 1.1 Validate that this change supersedes the source-checkout symlink
  approach without reintroducing the old Emacs TUI Terminal Profile direction.
- [x] 1.2 Keep the command surface limited to Alan-owned distribution state and
  exclude service-manager wrapper commands from the task scope.

## 2. CLI Command Surface

- [x] 2.1 Add the `alan emacs` command group with `status`, `install`, `doctor`,
  and `uninstall`.
- [x] 2.2 Implement status output for distribution source, installed copy,
  selected config entry, and ownership state.
- [x] 2.3 Implement doctor checks for Emacs availability, bare Emacs loading,
  config-entry ownership, installed copy integrity, and daemon observations.

## 3. Distribution Source And Install Root

- [x] 3.1 Add a resolver for development source `tools/alan-emacs` and release
  bundled resource locations.
- [x] 3.2 Materialize the distribution under an Alan-managed user data root such
  as `~/.local/share/alan/emacs/current`.
- [x] 3.3 Ensure the installed copy does not point directly at the source
  checkout in normal install mode.

## 4. Config Entry Detection

- [x] 4.1 Implement candidate discovery for `~/.emacs.d` and
  `$XDG_CONFIG_HOME/emacs` / `~/.config/emacs`.
- [x] 4.2 Detect Alan-owned, missing, empty, non-empty user-owned, and conflicting
  candidate states.
- [x] 4.3 Probe the installed `emacs` default user config directory when no
  existing candidate determines the choice.
- [x] 4.4 Stop safely on ambiguous or user-owned non-empty config entries.

## 5. Install And Uninstall

- [x] 5.1 Install by linking exactly one selected config entry to the
  Alan-managed distribution copy.
- [x] 5.2 Make repeated installs idempotent when the selected entry is already
  Alan-owned.
- [x] 5.3 Uninstall only Alan-owned config links and managed install data.
- [x] 5.4 Refuse to remove or mutate non-Alan-owned Emacs config entries.

## 6. Terminal Environment

- [x] 6.1 Add `EDITOR=emacs` and `VISUAL=emacs` to Alan for macOS terminal boot
  environments.
- [x] 6.2 Preserve existing terminal profile, shell, cwd, and `ALAN_SHELL_*`
  environment behavior.
- [x] 6.3 Avoid writing shell startup files or adding wrapper paths to editor
  variables.

## 7. Verification

- [x] 7.1 Add focused Rust tests for config-entry selection and conflict
  handling using temporary homes.
- [x] 7.2 Add focused tests for idempotent install, safe uninstall, and bare
  Emacs verification boundaries.
- [x] 7.3 Add or update Apple shell runtime metadata tests for `EDITOR` and
  `VISUAL` projection.
- [x] 7.4 Run `openspec validate add-alan-emacs-install-manager --strict`.
- [x] 7.5 Run relevant focused Rust tests and Apple shell metadata tests.
- [x] 7.6 Run `openspec validate --all --strict`.

## 8. Archive Readiness

- [ ] 8.1 Before archiving after merge, sync accepted delta requirements into
  `openspec/specs/`.
- [ ] 8.2 Archive only after implementation, verification, review, and merge are
  complete.
