## Context

Alan's macOS shell already has the right primitive for terminal Emacs: a
Terminal Profile can bind a pane to a resolved launch command, working
directory, and non-secret terminal environment while the pane remains ordinary
Ghostty-backed terminal content. The product direction is terminal-first, so
Emacs should enter through this terminal profile boundary rather than through a
new editor renderer, a GUI Emacs embed, or a new shell content kind.

The near-term user need is simple: open a reliable Emacs TUI tab from Alan and
let Emacs own editing, keybindings, buffers, org-mode, and process state. Later
Alan-native org previews can build on explicit Emacs-to-Alan bridge messages,
but that is not part of this change.

## Goals / Non-Goals

**Goals:**

- Provide a first-party Emacs TUI Terminal Profile preset with a stable profile
  id and title.
- Add a direct shell action for opening a new Emacs tab without manual profile
  setup.
- Keep Emacs panes as normal terminal content so tabs, splits, restore,
  metadata, close guards, and automation stay within existing shell contracts.
- Preserve Emacs TUI input fidelity for Control, Meta, Escape, paste,
  alternate-screen, and terminal mouse workflows.
- Leave environment metadata sufficient for a later explicit Emacs Lisp bridge.

**Non-Goals:**

- Embed GUI Emacs, use Emacs.app as the renderer, or add native buffer editing.
- Add org-mode rich preview panes, media rendering, or automatic org parsing.
- Add a new `ShellTabKind`, `ShellLaunchTarget`, content payload, daemon API, or
  runtime provider capability.
- Automatically inspect arbitrary Emacs buffers or send buffer contents to Alan.

## Decisions

1. Treat Emacs as a managed Terminal Profile preset.

   The Emacs entry point should be a stable local Terminal Profile with id
   `emacs_tui`, title `Emacs`, presentation icon `rectangle.and.pencil.and.ellipsis`,
   and a `custom_command` launch. This uses the existing terminal profile
   resolution path, including Space defaults, explicit tab creation, workspace
   restore, and environment projection.

   Alternative considered: add `ShellLaunchTarget.emacs`. That would make Emacs
   a separate launch mode and force new restore/control-plane cases for behavior
   that is already covered by Terminal Profiles.

2. Launch through a small shell command, not a new binary dependency.

   The preset command should prefer `emacsclient -nw -a ""`, fall back to
   `emacs -nw`, and if neither command exists, print a short actionable message
   before returning to the user's login shell. The Terminal Profile validator
   should continue validating the `/bin/zsh` command runner rather than failing
   the profile solely because Emacs is not currently installed or not on PATH.

   Alternative considered: require an absolute Emacs path in settings. That is
   less useful across Homebrew, MacPorts, Nix, and custom installs, and it makes
   the first-run path heavier than needed.

3. Add a workspace action instead of a new tab type.

   "New Emacs Tab" should route through the same command handling as "New
   Terminal Tab", passing `terminal_profile_id = emacs_tui`. Sidebar rows,
   pane titles, process metadata, and close behavior should come from the
   terminal runtime and profile presentation rather than from an Emacs-specific
   shell model branch.

   Alternative considered: show Emacs as a separate sidebar content kind. That
   would make later GUI preview work harder to separate from the terminal editor
   and would blur the terminal-first product boundary.

4. Keep future Alan/Emacs communication explicit.

   The Emacs TUI process should receive the existing `ALAN_SHELL_*` and
   `ALAN_TERMINAL_PROFILE_*` variables. A later Emacs Lisp package can use
   those variables plus the shell control plane to ask Alan for previews or
   context transfer, but this change does not create that protocol.

   Alternative considered: infer Emacs state from terminal screen output. That
   is fragile and would create accidental context capture.

## Risks / Trade-offs

- Emacs Meta behavior can conflict with macOS Option handling -> Verify the
  terminal input adapter supports the expected Option-as-Meta behavior and
  document any existing limitation before marking the task complete.
- `emacsclient -nw -a ""` behavior varies with user Emacs installs -> Keep the
  fallback command self-contained and verify both daemon-present and no-server
  cases when possible.
- Long-running Emacs sessions look like foreground commands -> Ensure close
  guards treat Emacs panes as active terminal work and do not silently close
  them.
- A managed preset could be mistaken for provider/account config -> Keep it
  strictly channel-local Terminal Profile state and never write workspace,
  connection, credential, or agent definition files.
- Future org preview work could expand scope -> Leave it as a follow-up change
  with explicit bridge requirements.

## Migration Plan

1. Add the Emacs TUI preset to the active channel's Terminal Profile surfaces
   without changing the user's default profile.
2. Add the "New Emacs Tab" action and route it through existing terminal tab
   creation with explicit `terminal_profile_id`.
3. Verify restore and close behavior using existing terminal lifecycle paths.
4. If a problem is found, rollback is removing the managed preset/action while
   leaving existing user Terminal Profiles and terminal restore data untouched.

## Open Questions

- Should the first UI entry point live only in the command/menu surface, or also
  appear as a compact sidebar/profile menu item?
- Should a future setting let users override the Emacs launch command while
  keeping the managed profile id?
