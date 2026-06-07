## Context

Alan now has a source-owned Emacs distribution under `tools/alan-emacs`. The
near-term product goal is not an Alan-to-Emacs bridge and not a new editor UI in
Alan for macOS. The goal is simpler and more foundational: after a user installs
Alan Emacs, running ordinary `emacs` should open Alan's Emacs environment, and
Alan terminal panes should make common editor-driven command-line workflows use
that same environment.

The current local symlink from `~/.config/emacs` to the source checkout is a
development convenience, not a release model. Alan needs an installer that
copies or materializes the bundled distribution into an Alan-owned user data
location and then connects the user's actual Emacs config entry to that
installed copy.

## Goals / Non-Goals

**Goals:**

- Provide a restrained `alan emacs` CLI command group for Alan-owned Emacs
  distribution state.
- Install Alan Emacs from a bundled resource or development source into an
  Alan-managed user data location.
- Programmatically choose the user's Emacs config entry from actual Emacs
  behavior and filesystem state.
- Ensure bare `emacs` loads Alan Emacs after install.
- Ensure Alan terminal panes set `EDITOR=emacs` and `VISUAL=emacs`.
- Report daemon/config mismatches when useful without managing daemon services.
- Keep user configuration safe by refusing ambiguous or non-Alan-owned config
  entries.

**Non-Goals:**

- Do not wrap `brew services`, `launchctl`, `systemctl`, or Emacs daemon
  lifecycle commands.
- Do not write shell startup files.
- Do not shadow the `emacs` executable in `PATH`.
- Do not require editor variables to contain wrapper paths or extra arguments.
- Do not create both `~/.emacs.d` and `~/.config/emacs` by default.
- Do not add an Alan bridge, org preview, package manager layer, or
  Spacemacs/Doom-like layer system.

## Decisions

### `alan emacs` manages only Alan-owned state

The command surface should be:

```bash
alan emacs status
alan emacs install
alan emacs doctor
alan emacs uninstall
```

`status` reports whether Alan Emacs is installed and which config entry is in
use. `install` materializes the distribution and links the selected config
entry. `doctor` runs deeper checks, including bare `emacs` loading and daemon
observations. `uninstall` removes only Alan-owned links and installed data.

Alternative considered: add `daemon restart`, `brew-service`, `edit`, or
`init` subcommands. Those would make Alan a wrapper around Emacs and system
service tools. The first version should stay focused on the state Alan owns.

### Installation uses an Alan-managed copy, not the source checkout

The installer should materialize the distribution into:

```text
~/.local/share/alan/emacs/current
```

The source may be `tools/alan-emacs` in development or a bundled resource in a
release app/CLI installation. Release lookup should consider both the current
executable path and its resolved symlink target so PATH-visible command links
can still find `Contents/Resources/alan-emacs`. The selected Emacs config entry
points at the managed copy, not at the source checkout.

Source discovery is a hard requirement for `install`, but it should not block
`status`, `doctor`, or `uninstall`. Those commands can still inspect or remove
Alan-owned managed state when a previous development checkout was removed or an
app bundle resource is damaged.

Alternative considered: symlink directly to `tools/alan-emacs`. That is fast
for local hacking, but it leaks source checkout layout into user configuration
and does not match release distribution.

The install root should keep room for a later versioned layout:

```text
~/.local/share/alan/emacs/releases/<build-id>
~/.local/share/alan/emacs/current -> releases/<build-id>
```

The first implementation may write `current` directly if that keeps the slice
smaller, but the design must not require changing command semantics to add
release directories later.

### Config-entry selection is detector driven

The installer must choose one active Emacs config entry rather than writing both
`~/.emacs.d` and `$XDG_CONFIG_HOME/emacs`.

Selection rules:

1. If an existing Emacs config entry is already Alan-owned, keep using it.
2. If one candidate exists and is empty, use that candidate.
3. If no candidate exists, ask the installed `emacs` what user config directory
   it uses by default and use that result.
4. If multiple candidates contain non-Alan-owned user configuration, stop and
   report the conflict.
5. If a selected candidate is non-empty and not Alan-owned, stop unless a future
   explicit migration flag is added.
6. If Emacs probing fails, stop and report that Emacs installation must be fixed
   first.

Alan-owned entries include managed install links and legacy links to Alan Emacs
source checkouts. Legacy source links are detected by current source equality,
Alan distribution marker files, or a broken old `tools/alan-emacs` target shape.

Candidates include:

```text
~/.emacs.d
$XDG_CONFIG_HOME/emacs, or ~/.config/emacs when XDG_CONFIG_HOME is unset
```

This keeps the policy portable. On the current machine, probing points at
`~/.emacs.d`, but that is a result of the installed Emacs behavior rather than
a hard-coded rule.

### Bare `emacs` is the integration point

After install, the supported user experience is:

```bash
emacs
```

No wrapper or `--init-directory` should be required for normal use. `alan emacs
doctor` should verify this with a real startup-discovery probe, not by manually
loading the installed init file, and confirm that Alan Emacs loaded from the
managed install.

Alternative considered: inject `EDITOR` or `VISUAL` as a wrapper path with
extra arguments. That would solve some editor-call paths, but it would not make
plain `emacs` behave correctly and would create inconsistent behavior between
manual and CLI-launched editing.

Because bare-startup verification happens after Alan has linked the selected
config entry, install must treat materialization, config linking, legacy-link
cleanup, and verification as one rollbackable operation. If verification fails,
Alan restores the selected config entry and the previous managed `current` copy
when one existed, so a failed install does not leave plain `emacs` pointed at a
bad Alan-managed copy.

### Alan terminal panes set simple editor variables

Alan for macOS terminal boot profiles should include:

```text
EDITOR=emacs
VISUAL=emacs
```

These values intentionally contain no path, wrapper, or arguments. Their
correctness depends on `alan emacs install` making plain `emacs` load Alan
Emacs. This keeps command-line tools that invoke `$EDITOR` from inheriting
complex parser-dependent values.

The first implementation should not write shell rc files. If a user wants the
same environment in external terminals, `doctor` can print a small advisory, but
the command does not mutate global shell startup state.

### Daemon state is observed, not controlled

`alan emacs doctor` may inspect whether `emacsclient` can connect and whether
the connected daemon appears to have loaded Alan Emacs. If it detects a mismatch,
it should report a concise manual action such as restarting the user's Emacs
service.

It must not run `brew services restart emacs`, `launchctl`, `systemctl`, or
other service lifecycle commands. The user already has standard tools for that
layer, and Alan should not become a sugar wrapper around them.

## Risks / Trade-offs

- Config entry detection could overwrite user data -> only allow missing,
  empty, or Alan-owned entries in the first implementation and stop on
  ambiguous user-owned state.
- Release resource lookup may differ between dev and packaged apps -> isolate
  resource discovery behind a small resolver that reports source path and
  install source in `status`/`doctor`.
- Bare `emacs` verification could be slow or environment-sensitive -> keep it
  as a doctor/install verification step, not something run for every terminal
  pane.
- Daemon mismatch could confuse users -> report the mismatch and the observed
  daemon config, but leave restart commands to the user's service manager.
- `$EDITOR=emacs` may open a full editor process instead of reusing a daemon ->
  this is acceptable for the first slice because correctness matters more than
  speed, and daemon optimization can be revisited after install semantics are
  stable.

## Migration Plan

1. Keep `tools/alan-emacs` as the source distribution in the monorepo.
2. Add resource discovery for development and release contexts.
3. Add `alan emacs status/install/doctor/uninstall`.
4. Implement detector-driven config-entry selection.
5. Install the distribution into Alan-managed user data and link the selected
   config entry to it.
6. Verify plain `emacs` loads Alan Emacs after install.
7. Add Alan terminal boot-profile environment injection for `EDITOR` and
   `VISUAL`.
8. Add focused tests for selection, conflicts, repeat installs, uninstall, and
   terminal env projection.

Rollback should remove only Alan-owned config links and managed install data.
It must not delete user-owned Emacs files or stop system services.

## Open Questions

- Whether the first implementation should write `current` as a directory or as
  a symlink to a versioned release directory can be decided during planning.
- Whether `status` and `doctor` should be separate in the first implementation
  can be decided during planning; the user-facing command names should remain
  restrained either way.
- A future migration flag may support backing up non-empty user config, but the
  first implementation should refuse rather than migrate.
