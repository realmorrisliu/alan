## 1. Dependency Source Slice

- [x] 1.1 Add the Alan-maintained Ghostty fork as a pinned submodule at `third_party/ghostty`.
- [x] 1.2 Update Apple Ghostty setup scripts to prefer `third_party/ghostty` by default.
- [x] 1.3 Keep `ALAN_GHOSTTY_REPO` as an explicit development override and print the override source/revision when used.
- [x] 1.4 Record Ghostty artifact source revision/cache metadata for `GhosttyKit.xcframework`, resources, and terminfo.
- [x] 1.5 Add setup checks for missing submodule initialization, stale artifacts, and rebuild guidance.
- [x] 1.6 Make local GhosttyKit setup build macOS `native` no-SIMD artifacts by default, clear Zig proxy variables, and resolve Metal tools through the pinned fork.

## 2. Boot Request And Fake Runtime Slice

- [x] 2.1 Add a structured `AlanTerminalBootRequest` for executable path, arguments, cwd, environment, launch strategy, and non-secret Terminal Profile metadata.
- [x] 2.2 Route Terminal Profile resolution into `AlanTerminalBootRequest` instead of treating `surfaceCommand` as the primary runtime contract.
- [x] 2.3 Add Alan-owned PTY/process runtime protocols for launch, resize, input, EOF, signals, exit observation, readiness, and bounded transcript capture.
- [x] 2.4 Add fake PTY and fake process-controller implementations for focused service tests.
- [x] 2.5 Verify fake-runtime behavior for launch metadata, text delivery, resize, EOF, signal result, exit result, and snapshot capture without requiring Ghostty.

## 3. Darwin PTY Backend Slice

- [x] 3.1 Implement an app-process-owned Darwin PTY backend for ordinary local terminal launches.
- [x] 3.2 Own PTY allocation, child launch, process group/session setup, cwd/env projection, and file descriptor lifecycle in Alan runtime code.
- [x] 3.3 Implement nonblocking PTY IO, bounded transcript capture, input writes, EOF handling, and window-size propagation.
- [x] 3.4 Implement interrupt, terminate, kill, wait/reap, exit-status recording, and stable failure reporting.
- [x] 3.5 Add focused non-UI tests or smoke checks using local shell commands to prove the backend before Ghostty rendering is attached.

## 4. Ghostty Fork Attachment Slice

- [x] 4.1 Identify the minimal Ghostty fork API needed to attach a renderer to an externally owned PTY endpoint.
- [x] 4.2 Patch the Alan-maintained Ghostty fork with external-PTY attachment support.
- [x] 4.3 Regenerate or relink Ghostty headers, `GhosttyKit.xcframework`, resources, and terminfo from the pinned fork revision.
- [x] 4.4 Add an integration check that clearly reports unsupported external-PTY attachment instead of falling back to Ghostty-owned launch.

## 5. Alan Ghostty Bridge Slice

- [x] 5.1 Split Alan's Ghostty bridge responsibilities between renderer attachment and PTY/process runtime ownership.
- [x] 5.2 Attach Ghostty rendering to Alan-provided `AlanTerminalPtyHandle` instances.
- [x] 5.3 Route terminal text delivery, resize, EOF, and signal requests through Alan PTY/process handles.
- [x] 5.4 Preserve renderer readiness, surface lifecycle, scrollback behavior, input adapter behavior, and metadata projection across the attachment path.
- [x] 5.5 Ensure renderer visibility is not required for Alan PTY input delivery or lifecycle observation.

## 6. Production Cutover Slice

- [x] 6.1 Make terminal ContentInstance construction create the Alan-owned PTY runtime before Ghostty renderer attachment.
- [x] 6.2 Remove the normal Alan terminal path that asks Ghostty to own command/cwd/env child-process launch.
- [x] 6.3 Keep app-restart restore semantics snapshot-based and avoid claiming cross-app PTY/process continuity.
- [x] 6.4 Do not add a long-lived runtime selector or fallback process owner.
- [x] 6.5 Keep managed-user privileged-helper PTY provider work out of this change and treat it as a dependent follow-up.

## 7. Verification And Archive Readiness

- [x] 7.1 Run focused fake-runtime tests for boot requests, delivery, resize, signals, exit observation, and bounded transcript capture.
- [x] 7.2 Run Darwin PTY backend tests or smoke checks for local shell launch, input, resize, signal, EOF, and exit behavior.
- [x] 7.3 Run the Ghostty integration lane after local artifacts are prepared from the pinned fork.
- [x] 7.4 Run focused Apple shell contract or build checks relevant to dependency setup and terminal runtime boundaries.
- [x] 7.5 Manually verify Alan dev terminal creation, input, resize, close, and restore behavior after production cutover.
- [x] 7.6 Run `openspec validate add-macos-alan-owned-pty-runtime --strict`.
- [x] 7.7 After implementation lands, sync accepted delta specs into `openspec/specs/`.
- [x] 7.8 Archive the completed OpenSpec change after synced specs validate.
