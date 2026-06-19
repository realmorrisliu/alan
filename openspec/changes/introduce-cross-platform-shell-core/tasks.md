## 1. Scaffold And Contracts

- [x] 1.1 Add `crates/shell-core` as a pure Rust workspace crate with no Apple, GTK, Ghostty, daemon-server, or platform IO dependencies.
- [x] 1.2 Define shell-core request/response envelope versioning, schema mismatch errors, and structured error envelope conventions.
- [x] 1.3 Add Rust contract-test layout for shell-core domains.
- [x] 1.4 Add scripts or just targets for shell-core unit and adapter tests.
- [x] 1.5 Document the shell-core module ownership boundary in crate docs and Apple architecture docs.
- [x] 1.6 Run `cargo test -p alan-shell-core` or the crate-specific equivalent and `cargo fmt --all`.

## 2. Workspace Model And Split Tree

- [x] 2.1 Port platform-neutral workspace identity/value types for Spaces, Tabs, PaneSlots, ContentInstances, content kinds, lifecycle state, attention, and terminal metadata.
- [x] 2.2 Port split tree structures and traversal helpers from the Swift shell model.
- [x] 2.3 Implement split adjacency, spatial focus lookup, resize ratio, equalize, zoom metadata, and stable ID preservation semantics.
- [x] 2.4 Add Rust split-tree contract tests covering split/focus/resize/equalize/zoom behavior.
- [x] 2.5 Add FFI-backed Swift adapter tests for split-tree replacement paths that cross the Apple boundary.
- [x] 2.6 Run shell-core tests plus affected FFI-backed Swift shell tests.

## 3. State Reducer

- [x] 3.1 Implement Rust reducer results with next state, changed IDs, domain events, runtime intents, manifest sync hints, and stable errors.
- [x] 3.2 Port space and tab mutations: create, select, open terminal tab, duplicate, close, pin, unpin, reorder, move to Space, clear inactive temporary tabs, and selection repair.
- [x] 3.3 Port pane/content mutations: split, close, lift, cross-tab move, in-tab move, direct focus, spatial focus, resize split, equalize, zoom, unzoom, and unsupported-content handling.
- [x] 3.4 Port attention, agent activity, cwd/title/runtime metadata update, and active-task protection semantics.
- [x] 3.5 Add Rust reducer contract tests for success and stable-error cases.
- [x] 3.6 Add FFI-backed Swift adapter tests for every Swift reducer branch planned for replacement.
- [x] 3.7 Run shell-core tests, `test-shell-sidebar-tab-rows.sh`, and any affected reducer-focused scripts.

## 4. Workspace Manifest

- [x] 4.1 Port workspace manifest schema, content contract version, default manifest, quick terminal restore record, and restore snapshot value types.
- [x] 4.2 Port legacy terminal-only manifest upgrade into content-container manifest shape.
- [x] 4.3 Port materialization from manifest into workspace state while preserving current JSON-visible semantics.
- [x] 4.4 Port TTL pruning, active-task retention, empty-Space retention, selected Space/Tab repair, and pin/live snapshot selection.
- [x] 4.5 Add Rust manifest contract tests covering valid manifests, old manifests, malformed/corrupt inputs, missing profiles, quick terminal state, pruning, and materialization.
- [x] 4.6 Add FFI-backed Swift manifest adapter tests that preserve compatibility with existing macOS manifest JSON.
- [x] 4.7 Run shell-core tests and `clients/apple/scripts/test-shell-workspace-manifest.sh`.

## 5. Action Registry

- [x] 5.1 Port stable action IDs, target kinds, explicit/current/context target resolution, and action availability states.
- [x] 5.2 Port shortcut metadata and action descriptors that are platform-neutral.
- [x] 5.3 Port action-to-reducer/effect mapping while keeping macOS-only presentation commands outside shell core.
- [x] 5.4 Add FFI-backed Swift action adapter coverage from menu/context/keyboard/sidebar action tests.
- [x] 5.5 Add Rust action registry contract tests for availability, target resolution, and effect mapping.
- [x] 5.6 Run shell-core tests, `clients/apple/scripts/test-shell-sidebar-tab-rows.sh`, and `clients/apple/scripts/test-shell-core-ffi-adapter.sh`.

## 6. Control Command Reducer

- [x] 6.1 Port shell control command DTOs or request envelopes needed for workspace-domain command reduction.
- [x] 6.2 Port command validation, required-field checks, stable error codes, and authoritative response projection for domain commands.
- [x] 6.3 Separate runtime intents for terminal-dependent commands from platform terminal runtime outcomes.
- [x] 6.4 Add Rust/FFI-backed coverage from automation/control command seam tests for applied and rejected commands.
- [x] 6.5 Add Rust control reducer tests proving response compatibility and stable error semantics.
- [x] 6.6 Run shell-core tests, `clients/apple/scripts/test-shell-automation-command-seams.sh`, and `clients/apple/scripts/check-shell-contracts.sh`.

## 7. Terminal Profile Domain

- [x] 7.1 Port Terminal Profile document, launch modes, editor drafts/results, validation errors, redacted display detail, and deterministic resolution state.
- [x] 7.2 Implement `TerminalLaunchIntent` construction without spawning processes or inspecting platform UI/runtime handles.
- [x] 7.3 Keep store path selection, file IO, process spawning, Managed Terminal Account apply, sudoers writes, and AppleScript execution in macOS platform code.
- [x] 7.4 Add Rust/FFI-backed coverage from terminal profile and managed terminal account dry-run tests.
- [x] 7.5 Add Rust tests for profile validation, missing/unavailable profile handling, fallback resolution, and launch-intent output.
- [x] 7.6 Run shell-core tests, `clients/apple/scripts/test-shell-settings-surface.sh`, and `clients/apple/scripts/test-terminal-account-dev-dry-run-smoke.sh`.

## 8. Settings Summaries

- [x] 8.1 Identify settings/domain summaries that are reusable shell or host-domain state rather than SwiftUI presentation composition.
- [x] 8.2 Port reusable summaries for terminal profiles, workspace context, capabilities, diagnostics metadata, and local state where they do not require platform UI.
- [x] 8.3 Leave SwiftUI section/row layout, icons, local folder opening, update UI, and AppKit presentation code in the Apple client.
- [x] 8.4 Add Rust/FFI-backed coverage for settings summary inputs and expected semantic summaries.
- [x] 8.5 Add Rust tests for settings summary behavior and Swift adapter shape.
- [x] 8.6 Run shell-core tests and `clients/apple/scripts/test-shell-settings-surface.sh`.

## 9. Swift Binding Facade And Replacement

- [x] 9.1 Choose the first binding implementation for the coarse-grained facade: UniFFI over versioned byte/envelope functions or hand-written C ABI over versioned byte/envelope functions.
- [x] 9.2 Add a dedicated binding facade crate or module that wraps shell-core without polluting the pure Rust API.
- [x] 9.3 Pin binding generator/tool versions when generated Swift/header/modulemap output is introduced.
- [x] 9.4 Add Swift `ShellCoreFFIAdapter` or equivalent that owns encode/decode, error mapping, version mismatch handling, and generated binding isolation.
- [x] 9.5 Replace Swift reducer calls with Rust-backed adapter calls after reducer contract and adapter tests pass; remove the replaced Swift logic.
- [x] 9.6 Replace Swift manifest calls with Rust-backed adapter calls after manifest contract and adapter tests pass; preserve file IO in Swift.
- [x] 9.7 Replace Swift action/control/profile/settings call paths module by module after each module's contract and adapter tests pass.
- [x] 9.8 Run focused Swift scripts for every replaced module plus Rust shell-core tests and binding tests.

## 10. Architecture Burn-Down, Review, And Archive Readiness

- [x] 10.1 Update `clients/apple/ARCHITECTURE.md` whenever a Swift warning class is reduced, removed, or intentionally left as adapter-only debt.
- [x] 10.2 Tighten architecture checks to prevent new reusable shell domain logic from being added to replaced Swift shell model/controller files.
- [x] 10.3 Run `bash clients/apple/scripts/check-architecture-maintainability.sh` and record warning-count changes.
- [x] 10.4 Run `cargo test --workspace` or the agreed narrower Rust validation set for the implemented migration slice.
- [x] 10.5 Run affected Apple shell scripts and `git diff --check`.
- [x] 10.6 Request or perform code review for the completed migration slice before merge.
- [ ] 10.7 Sync accepted delta specs into `openspec/specs/` after implementation is merged.
- [x] 10.8 Run `openspec validate introduce-cross-platform-shell-core --type change --strict --json` and `openspec validate --all --strict --json`.
- [ ] 10.9 Archive the change only after implementation, review, validation, and spec sync are complete.
