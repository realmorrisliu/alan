## Why

Alan for macOS is terminal-first, but users who live in Emacs need a reliable
way to launch terminal Emacs as a first-class workspace surface instead of
manually configuring a generic custom command each time. Supporting Emacs TUI as
a built-in terminal workflow keeps editing inside the existing Ghostty-backed
terminal model while creating a stable foundation for later Alan-native previews.

## What Changes

- Add a first-party Emacs TUI Terminal Profile preset that launches terminal
  Emacs through `emacsclient -nw -a ""` with a safe fallback to `emacs -nw`.
- Add an explicit "New Emacs Tab" workspace action that creates an ordinary
  terminal tab bound to the Emacs TUI profile.
- Preserve existing terminal content, Space, split, restore, and close semantics;
  Emacs does not become a new `ShellTabKind` or separate renderer.
- Project non-secret Emacs profile metadata to the terminal environment so a
  future Emacs Lisp bridge can discover the Alan pane/window context explicitly.
- Add focused verification for Emacs-class TUI keyboard behavior, including
  Control/Meta/Escape sequences, alternate screen, paste, and process-exit
  state.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-terminal-profiles`: Terminal Profiles gain a managed Emacs TUI preset
  with deterministic launch, validation, and local storage behavior.
- `macos-shell-workspace-interactions`: The shell workspace gains an explicit
  command/menu action for creating an Emacs TUI terminal tab.
- `macos-shell-terminal-lifecycle`: Terminal startup and restore behavior must
  treat Emacs TUI as normal terminal content using the resolved profile.
- `macos-shell-build-test-contract`: Verification must cover Emacs-class TUI
  input and lifecycle behavior.

## Impact

- Apple client shell model, action registry, command handling, sidebar/menu
  surfaces, Terminal Profile store/defaults, and Terminal Profile resolution.
- Ghostty-backed terminal boot profile metadata and environment projection.
- Focused Swift/script tests for Terminal Profile presets, shell actions, and
  TUI input behavior.
- No daemon API, Rust runtime, provider connection, credential, or GUI Emacs
  integration changes.
