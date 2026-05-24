## 1. Shared Command And Test Seams

- [x] 1.1 Define shared shell command interfaces for create tab, split, focus, close, send text, read summary, and attention activation.
- [x] 1.2 Route existing shell controller, control-plane, menu, and command UI paths through the shared command interface where practical.
- [x] 1.3 Add fake shell controller/runtime fixtures for command, intent, and control-plane tests.
- [x] 1.4 Add privacy-safe summary helpers for pane/tab/window metadata.

## 2. App Intents

- [x] 2.1 Add App Entity types and queries for shell windows, spaces, tabs, panes, and attention items.
- [x] 2.2 Add intents for creating terminal tabs, creating alan tabs, splitting panes, focusing panes, closing panes/tabs, sending text, reading pane summaries, and opening attention items.
- [x] 2.3 Align intent success and failure results with shared shell command/control-plane result categories.
- [x] 2.4 Gate App Intent availability by supported macOS versions and document fallback behavior.
- [x] 2.5 Verify secure-input and privacy restrictions in intent summaries and logs.

## 3. Focused Tests

- [x] 3.1 Add Apple-client unit tests for shell model mutations, split/focus/close behavior, and privacy-safe summaries.
- [x] 3.2 Add runtime service fake tests for text delivery, unavailable runtime, teardown, and metadata snapshots.
- [x] 3.3 Add control-plane tests for query/mutation success, malformed requests, missing targets, runtime unavailable, timeout, and IO diagnostics.
- [x] 3.4 Add App Intent routing tests using fake shell state and fake runtime outcomes.
- [x] 3.5 Add a Ghostty-enabled integration lane that runs only when local Ghostty artifacts are prepared.

## 4. UI Smoke And Documentation

- [x] 4.1 Add a repeatable UI smoke or screenshot script for launch, space/tab switching, split creation, command UI, pane-scoped Find behavior for the focused pane, and basic terminal input when available.
- [x] 4.2 Add focused Apple test and UI smoke commands to `justfile` or Apple helper scripts.
- [x] 4.3 Update `clients/apple/README.md` with dependency setup, focused tests, UI smoke, and Ghostty integration commands.
- [x] 4.4 Ensure smoke artifacts avoid exposing private terminal content.

## 5. Verification

- [x] 5.1 Run `git diff --check`.
- [x] 5.2 Run the focused Apple shell test command added by this change.
- [x] 5.3 Run the UI smoke/screenshot command added by this change.
- [x] 5.4 Run the Ghostty integration lane when local artifacts are prepared.
- [x] 5.5 Run the renamed `alan-macos` macOS build command.

## 6. PR And Archive Readiness

- [x] 6.1 Review App Intent names, display representations, and privacy behavior in Shortcuts/Spotlight where available.
- [x] 6.2 Confirm tests are layered so everyday checks do not require real Ghostty artifacts.
- [x] 6.3 Before archive, sync accepted delta requirements into `openspec/specs/`.
- [x] 6.4 Archive the OpenSpec change after implementation is merged.
