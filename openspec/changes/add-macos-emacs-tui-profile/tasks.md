## 1. Terminal Profile Preset

- [ ] 1.1 Add a managed Emacs TUI Terminal Profile preset with stable id `emacs_tui`, title `Emacs`, and presentation metadata.
- [ ] 1.2 Define the preset custom command so it prefers `emacsclient -nw -a ""`, falls back to `emacs -nw`, and returns to a login shell with a concise message when Emacs is unavailable.
- [ ] 1.3 Ensure the preset is available in Terminal Profile loading/settings surfaces without changing the user's default Terminal Profile.
- [ ] 1.4 Keep the preset channel-local and confirm no workspace manifest, agent definition, provider connection, or credential store writes are introduced.

## 2. Shell Workspace Action

- [ ] 2.1 Add a `New Emacs Tab` shell action/command entry that routes through existing terminal tab creation with `terminal_profile_id` `emacs_tui`.
- [ ] 2.2 Wire the action into the appropriate macOS command/menu or command-palette surface without adding a new `ShellTabKind` or content payload.
- [ ] 2.3 Preserve current cwd resolution for Emacs tab creation, including runtime cwd, pane snapshot cwd, explicit cwd, and home fallback behavior.
- [ ] 2.4 Confirm Space default Terminal Profile binding can use `emacs_tui` for ordinary new terminal tabs.

## 3. Terminal Lifecycle And Metadata

- [ ] 3.1 Verify Emacs TUI panes use the existing Ghostty-backed terminal boot profile path for command, cwd, and environment.
- [ ] 3.2 Confirm `ALAN_SHELL_*` and `ALAN_TERMINAL_PROFILE_*` metadata are present for Emacs TUI panes and contain no provider secrets or buffer contents.
- [ ] 3.3 Verify split, background tab, restore, title/metadata projection, and runtime identity behavior remain normal terminal ContentInstance behavior.
- [ ] 3.4 Verify active Emacs panes use the existing destructive terminal close guard and finalization path.

## 4. Focused Tests

- [ ] 4.1 Add focused Terminal Profile tests for the Emacs preset id, title, launch command, validation behavior, and default-profile non-selection.
- [ ] 4.2 Add shell model or control-plane tests proving New Emacs Tab creates terminal content with `terminal_profile_id` `emacs_tui` and normal focus/cwd state.
- [ ] 4.3 Add or update runtime metadata tests proving Emacs profile environment projection and terminal ContentInstance identity remain stable.
- [ ] 4.4 Add script-level or documented manual verification for Emacs TUI input: printable keys, Escape, Tab, Backspace, `C-x`, `C-c`, Meta input, paste, mouse mode, and alternate screen.
- [ ] 4.5 Add lifecycle verification for Emacs exit, backgrounding, split behavior, restore, and active-work close confirmation.

## 5. Validation And Archive Readiness

- [ ] 5.1 Run the focused Apple client scripts touched by Terminal Profiles, shell runtime metadata, and shell state mutation changes.
- [ ] 5.2 Run `openspec validate add-macos-emacs-tui-profile --strict`.
- [ ] 5.3 Run `git diff --check`.
- [ ] 5.4 Run the relevant macOS Xcode build command with signing disabled.
- [ ] 5.5 Capture implementation notes and manual verification evidence in this change before requesting review.
- [ ] 5.6 Before archiving after merge, sync accepted delta requirements into `openspec/specs/` and rerun strict OpenSpec validation.
