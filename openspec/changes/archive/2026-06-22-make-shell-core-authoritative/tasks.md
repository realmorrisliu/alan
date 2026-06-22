## 1. Authority Audit

- [x] 1.1 Inventory every `ShellCoreFFIAdapter` call site and nearby `try?`, `??`, `catch`, and fallback path in the macOS shell.
- [x] 1.2 Classify each remaining Swift fallback or duplicate algorithm as domain duplicate, adapter projection, platform recovery/effect, or parity fixture only.
- [x] 1.3 Record the classification in the implementation PR notes or `clients/apple/ARCHITECTURE.md` so reviewers can distinguish legitimate platform fallback from shell-domain fallback.
- [x] 1.4 Identify Swift tests that currently depend on duplicate manifest, reducer, action, control-command, profile, or settings-domain implementations.

## 2. Manifest Authority Slice

- [x] 2.1 Change workspace-manifest startup so default manifest creation, legacy migration, TTL pruning, and materialization require successful shell-core operations.
- [x] 2.2 Preserve Swift-owned manifest file IO, atomic writes, corrupt-file quarantine, and recovery diagnostics without recomputing portable manifest semantics in Swift.
- [x] 2.3 Delete or quarantine runtime Swift implementations of content manifest defaulting, pruning, and materialization after equivalent Rust and FFI tests cover the behavior.
- [x] 2.4 Update Swift workspace-manifest tests to verify adapter usage, corrupt-file recovery, persistence behavior, and app startup integration rather than the removed Swift domain algorithms.
- [x] 2.5 Add shell-contract validation that rejects runtime fallback from shell-core manifest operations to Swift manifest algorithms.
- [x] 2.6 Run `cargo test -p alan-shell-core`, `cargo test -p alan-shell-core-ffi`, `clients/apple/scripts/test-shell-workspace-manifest.sh`, and `clients/apple/scripts/check-shell-contracts.sh`.

## 3. Reducer Authority Slice

- [x] 3.1 Review reducer-backed macOS mutations and remove Swift fallback or post-computation that recomputes workspace focus, split layout, tab organization, pinning, lifecycle, zoom, resize, or attention.
- [x] 3.2 Keep only adapter projection and platform-state preservation passes such as transient terminal/runtime metadata, focus requests, and UI notifications.
- [x] 3.3 Convert remaining reducer failures to explicit diagnostics, stable failure results, or no-op user-visible failures derived from shell-core errors.
- [x] 3.4 Update reducer-focused Swift tests so portable mutation assertions live in Rust shell-core or FFI fixture tests and Swift tests cover app integration.
- [x] 3.5 Run affected reducer and split tests including `cargo test -p alan-shell-core`, `cargo test -p alan-shell-core-ffi`, `clients/apple/scripts/test-shell-split-model.sh`, and `clients/apple/scripts/test-shell-runtime-metadata.sh`.

## 4. Control Command Authority Slice

- [x] 4.1 Route all shell-core-covered workspace-domain control commands through shell-core validation, reducer dispatch, stable errors, and response projection.
- [x] 4.2 Split host-only macOS commands from portable shell-domain commands so platform diagnostics, export, and runtime-only commands are explicit Swift host behavior.
- [x] 4.3 Represent terminal-runtime side effects as core-derived intents plus Swift-owned delivery outcomes for send-text, focus, close, capture, and related runtime work.
- [x] 4.4 Remove duplicate Swift command-validation branches for covered portable commands.
- [x] 4.5 Update control-command seam tests to assert core-derived responses and Swift side-effect execution separately.
- [x] 4.6 Run `cargo test -p alan-shell-core`, `cargo test -p alan-shell-core-ffi`, `clients/apple/scripts/test-shell-automation-command-seams.sh`, and `clients/apple/scripts/check-shell-contracts.sh`.

## 5. Action, Profile, And Settings Authority Slice

- [x] 5.1 Make shared action title, descriptor, shortcut, keyboard mapping, availability, and effect dispatch fail closed on shell-core errors instead of using Swift registry fallback.
- [x] 5.2 Remove or quarantine duplicate Swift action registry domain tables for shell-core-owned actions while keeping Swift menu/context/keyboard presentation.
- [x] 5.3 Make Terminal Profile validation, editor-domain results, deterministic resolution, and launch-intent construction require shell-core results.
- [x] 5.4 Keep Swift-owned Terminal Profile storage, corrupt-store recovery, process spawning, helper readiness, and UI presentation.
- [x] 5.5 Remove Swift settings-summary fallback for shell-core-owned reusable settings rows, leaving SwiftUI layout and platform controls in Swift.
- [x] 5.6 Run `cargo test -p alan-shell-core`, `cargo test -p alan-shell-core-ffi`, `clients/apple/scripts/test-shell-action-registry.sh`, and `clients/apple/scripts/test-shell-settings-surface.sh`.

## 6. Validation Guards And Architecture Burn-Down

- [x] 6.1 Add focused shell-contract or architecture checks that reject new `try? ShellCoreFFIAdapter... ?? SwiftDomainImplementation` patterns in replaced areas.
- [x] 6.2 Scope validation allowlists so UI label fallback, Ghostty/runtime fallback, corrupt-file quarantine, pasteboard delivery, and diagnostics presentation remain permitted.
- [x] 6.3 Update `clients/apple/ARCHITECTURE.md` with the post-cleanup shell-core authority boundary and any remaining adapter-only debt.
- [x] 6.4 Run `clients/apple/scripts/check-architecture-maintainability.sh` and record warning-count or warning-class changes. Current report: 17 warning(s).
- [x] 6.5 Run `git diff --check` and the affected focused validation matrix for the completed implementation slice.

## 7. Review And Archive Readiness

- [x] 7.1 Review the implementation for accidental user-facing regressions in workspace startup, tab restoration, command responses, action availability, and Terminal Profile launch behavior.
- [x] 7.2 Ensure each removed Swift domain path has Rust shell-core or FFI coverage and any remaining Swift test is adapter/platform focused.
- [x] 7.3 Run `openspec validate make-shell-core-authoritative --type change --strict --json`.
- [x] 7.4 Run `openspec validate --all --strict --json` after coordinating with other active OpenSpec changes.
- [ ] 7.5 After implementation is merged, sync accepted delta specs into `openspec/specs/`.
- [ ] 7.6 Archive the change only after implementation, review, validation, spec sync, and any required follow-up PRs are complete.
