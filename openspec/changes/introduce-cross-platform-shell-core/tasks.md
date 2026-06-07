## 1. Scaffold And Contracts

- [ ] 1.1 Add `crates/shell-core` as a pure Rust workspace crate with no Apple, GTK, Ghostty, daemon-server, or platform IO dependencies.
- [ ] 1.2 Define shell-core request/response envelope versioning, schema mismatch errors, and structured error envelope conventions.
- [ ] 1.3 Add fixture directory layout and Rust fixture loader/comparison helpers for Swift-exported parity cases.
- [ ] 1.4 Add scripts or just targets for shell-core unit tests and fixture tests.
- [ ] 1.5 Document the shell-core module ownership boundary in crate docs and Apple architecture docs.
- [ ] 1.6 Run `cargo test -p alan-shell-core` or the crate-specific equivalent and `cargo fmt --all`.

## 2. Workspace Model And Split Tree

- [ ] 2.1 Port platform-neutral workspace identity/value types for Spaces, Tabs, PaneSlots, ContentInstances, content kinds, lifecycle state, attention, and terminal metadata.
- [ ] 2.2 Port split tree structures and traversal helpers from the Swift shell model.
- [ ] 2.3 Implement split adjacency, spatial focus lookup, resize ratio, equalize, zoom metadata, and stable ID preservation semantics.
- [ ] 2.4 Export Swift parity fixtures from `test-shell-split-model` covering split/focus/resize/equalize/zoom behavior.
- [ ] 2.5 Add Rust fixture tests proving split tree behavior matches Swift-exported cases.
- [ ] 2.6 Run shell-core tests plus `clients/apple/scripts/test-shell-split-model.sh`.

## 3. State Reducer

- [ ] 3.1 Implement Rust reducer results with next state, changed IDs, domain events, runtime intents, manifest sync hints, and stable errors.
- [ ] 3.2 Port space and tab mutations: create, select, open terminal tab, duplicate, close, pin, unpin, reorder, move to Space, clear inactive temporary tabs, and selection repair.
- [ ] 3.3 Port pane/content mutations: split, close, lift, cross-tab move, in-tab move, direct focus, spatial focus, resize split, equalize, zoom, unzoom, and unsupported-content handling.
- [ ] 3.4 Port attention, agent activity, cwd/title/runtime metadata update, and active-task protection semantics.
- [ ] 3.5 Export Swift parity fixtures from reducer-focused shell scripts for success and stable-error cases.
- [ ] 3.6 Add Rust reducer unit tests and fixture tests for every Swift reducer branch planned for replacement.
- [ ] 3.7 Run shell-core tests, `test-shell-split-model.sh`, `test-shell-sidebar-tab-rows.sh`, and any affected reducer-focused scripts.

## 4. Workspace Manifest

- [ ] 4.1 Port workspace manifest schema, content contract version, default manifest, quick terminal restore record, and restore snapshot value types.
- [ ] 4.2 Port legacy terminal-only manifest upgrade into content-container manifest shape.
- [ ] 4.3 Port materialization from manifest into workspace state while preserving current JSON-visible semantics.
- [ ] 4.4 Port TTL pruning, active-task retention, empty-Space retention, selected Space/Tab repair, and pin/live snapshot selection.
- [ ] 4.5 Export Swift parity fixtures covering valid manifests, old manifests, malformed/corrupt inputs, missing profiles, quick terminal state, pruning, and materialization.
- [ ] 4.6 Add Rust manifest unit tests and fixture tests that preserve compatibility with existing macOS manifest JSON.
- [ ] 4.7 Run shell-core tests and `clients/apple/scripts/test-shell-workspace-manifest.sh`.

## 5. Action Registry

- [ ] 5.1 Port stable action IDs, target kinds, explicit/current/context target resolution, and action availability states.
- [ ] 5.2 Port shortcut metadata and action descriptors that are platform-neutral.
- [ ] 5.3 Port action-to-reducer/effect mapping while keeping macOS-only presentation commands outside shell core.
- [ ] 5.4 Export Swift parity fixtures from menu/context/keyboard/sidebar action tests.
- [ ] 5.5 Add Rust action registry unit tests and fixture tests for availability, target resolution, and effect mapping.
- [ ] 5.6 Run shell-core tests, `clients/apple/scripts/test-shell-sidebar-tab-rows.sh`, and `clients/apple/scripts/test-shell-action-registry.sh`.

## 6. Control Command Reducer

- [ ] 6.1 Port shell control command DTOs or request envelopes needed for workspace-domain command reduction.
- [ ] 6.2 Port command validation, required-field checks, stable error codes, and authoritative response projection for domain commands.
- [ ] 6.3 Separate runtime intents for terminal-dependent commands from platform terminal runtime outcomes.
- [ ] 6.4 Export Swift parity fixtures from automation/control command seam tests for applied and rejected commands.
- [ ] 6.5 Add Rust control reducer tests proving response compatibility and stable error semantics.
- [ ] 6.6 Run shell-core tests, `clients/apple/scripts/test-shell-automation-command-seams.sh`, and `clients/apple/scripts/check-shell-contracts.sh`.

## 7. Terminal Profile Domain

- [ ] 7.1 Port Terminal Profile document, launch modes, editor drafts/results, validation errors, redacted display detail, and deterministic resolution state.
- [ ] 7.2 Implement `TerminalLaunchIntent` construction without spawning processes or inspecting platform UI/runtime handles.
- [ ] 7.3 Keep store path selection, file IO, process spawning, Managed Terminal Account apply, sudoers writes, and AppleScript execution in macOS platform code.
- [ ] 7.4 Export Swift parity fixtures from terminal profile and managed terminal account dry-run tests.
- [ ] 7.5 Add Rust tests for profile validation, missing/unavailable profile handling, fallback resolution, and launch-intent output.
- [ ] 7.6 Run shell-core tests, `clients/apple/scripts/test-shell-settings-surface.sh`, and `clients/apple/scripts/test-terminal-account-dev-dry-run-smoke.sh`.

## 8. Settings Summaries

- [ ] 8.1 Identify settings/domain summaries that are reusable shell or host-domain state rather than SwiftUI presentation composition.
- [ ] 8.2 Port reusable summaries for terminal profiles, workspace context, capabilities, diagnostics metadata, and local state where they do not require platform UI.
- [ ] 8.3 Leave SwiftUI section/row layout, icons, local folder opening, update UI, and AppKit presentation code in the Apple client.
- [ ] 8.4 Export Swift parity fixtures for settings summary inputs and expected semantic summaries.
- [ ] 8.5 Add Rust tests for settings summary behavior and Swift adapter shape.
- [ ] 8.6 Run shell-core tests and `clients/apple/scripts/test-shell-settings-surface.sh`.

## 9. Swift Binding Facade And Replacement

- [ ] 9.1 Choose the first binding implementation for the coarse-grained facade: UniFFI over versioned byte/envelope functions or hand-written C ABI over versioned byte/envelope functions.
- [ ] 9.2 Add a dedicated binding facade crate or module that wraps shell-core without polluting the pure Rust API.
- [ ] 9.3 Pin binding generator/tool versions when generated Swift/header/modulemap output is introduced.
- [ ] 9.4 Add Swift `ShellCoreFFIAdapter` or equivalent that owns encode/decode, error mapping, version mismatch handling, and generated binding isolation.
- [ ] 9.5 Replace Swift reducer calls with Rust-backed adapter calls after reducer parity passes; remove or quarantine the replaced Swift logic.
- [ ] 9.6 Replace Swift manifest calls with Rust-backed adapter calls after manifest parity passes; preserve file IO in Swift.
- [ ] 9.7 Replace Swift action/control/profile/settings call paths module by module after each module's parity and adapter tests pass.
- [ ] 9.8 Run focused Swift scripts for every replaced module plus Rust shell-core tests and binding tests.

## 10. Architecture Burn-Down, Review, And Archive Readiness

- [ ] 10.1 Update `clients/apple/ARCHITECTURE.md` whenever a Swift warning class is reduced, removed, or intentionally left as adapter-only debt.
- [ ] 10.2 Tighten architecture checks to prevent new reusable shell domain logic from being added to replaced Swift shell model/controller files.
- [ ] 10.3 Run `bash clients/apple/scripts/check-architecture-maintainability.sh` and record warning-count changes.
- [ ] 10.4 Run `cargo test --workspace` or the agreed narrower Rust validation set for the implemented migration slice.
- [ ] 10.5 Run affected Apple shell scripts and `git diff --check`.
- [ ] 10.6 Request or perform code review for the completed migration slice before merge.
- [ ] 10.7 Sync accepted delta specs into `openspec/specs/` after implementation is merged.
- [ ] 10.8 Run `openspec validate introduce-cross-platform-shell-core --type change --strict --json` and `openspec validate --all --strict --json`.
- [ ] 10.9 Archive the change only after implementation, review, validation, and spec sync are complete.
