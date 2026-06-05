## 1. Remove Visible Mac Shell Entry Points

- [x] 1.1 Remove the sidebar `Ask alan...` command launcher and related layout/state from `ShellSidebarView`.
- [x] 1.2 Remove `Ask alan...`, `Command-P`, and `New alan tab` menu items from `AlanMacShellCommands`.
- [x] 1.3 Remove sidebar, context, or creation affordances that expose New alan Tab.
- [x] 1.4 Verify the default shell still exposes normal terminal tab creation and pane-scoped Find.

## 2. Remove Floating Command Input Plumbing

- [x] 2.1 Remove `ShellCommandTabView` and its command resolution model if no remaining active code uses it.
- [x] 2.2 Remove command input presentation state, opacity/click-away logic, and `host.commandInputRequestID` handling from `MacShellRootView`.
- [x] 2.3 Remove `requestCommandInput`, `setCommandInputActive`, and `commandInputActive` state from `ShellHostController`.
- [x] 2.4 Remove command-input-specific terminal command routing branches and tests that only protected the deleted text field.

## 3. Remove Alan Tab Domain Actions

- [x] 3.1 Remove `newAlanTab` from `ShellWorkspaceCommand`, `ShellActionID`, action descriptors, shortcuts, titles, availability, and execution routing.
- [x] 3.2 Remove command input action IDs and registry entries that only existed to open Ask alan.
- [x] 3.3 Remove `openAlanTab`, `openingAlanTab`, and first-party `.alan` tab creation call paths from shell controller and state mutation code.
- [x] 3.4 Remove `.alan` launch target cases and automatic alan-tab runtime boot branches from terminal launch resolution.
- [x] 3.5 Keep terminal activity metadata for user-launched Alan/Codex/agent processes in normal terminal tabs.

## 4. Remove Automation And App Intent Surfaces

- [x] 4.1 Remove Create Alan Tab App Intent definitions and generated metadata checks.
- [x] 4.2 Remove `createAlanTab` automation helpers and intent-router branches.
- [x] 4.3 Remove or rewrite automation command seam tests that only validate first-party alan tab creation.
- [x] 4.4 Ensure Create Terminal Tab and other supported App Intents still compile and retain their behavior.

## 5. Update Specs, Contracts, And Documentation Guards

- [x] 5.1 Update shell contract scripts to reject `Ask alan...`, Command-P command input toggles, `ShellCommandTabView`, `newAlanTab`, Create Alan Tab, and `.alan` tab creation paths in active macOS shell code.
- [x] 5.2 Update focused action-registry and keybinding tests to assert removed actions and shortcuts are absent.
- [x] 5.3 Update UI conformance smoke expectations to show a terminal-first shell without Ask alan or New alan Tab.
- [x] 5.4 Preserve CLI documentation and tests for `alan ask` and `alan chat` outside the macOS shell tab product surface.

## 6. Verification

- [x] 6.1 Run focused Apple shell scripts affected by action registry, automation, runtime launch, and shell contract changes.
- [x] 6.2 Run the focused build or typecheck command needed to catch Swift compile fallout from removed enum cases and views.
- [x] 6.3 Run `openspec validate remove-macos-ask-alan-and-alan-tabs --strict`.
- [x] 6.4 Run `openspec validate --all --strict` before packaging the implementation PR.

## 7. PR And Archive Readiness

- [x] 7.1 Prepare PR notes explaining that CLI remains intact while macOS Ask alan and first-party alan tabs are removed.
- [x] 7.2 Include verification evidence for absent UI/actions, preserved terminal tab creation, and preserved CLI references.
- [ ] 7.3 After merge, sync accepted deltas into `openspec/specs/` before archiving the change.
