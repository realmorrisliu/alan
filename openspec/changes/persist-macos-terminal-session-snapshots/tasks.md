## 1. Manifest And Restore Model

- [ ] 1.1 Add optional bounded terminal transcript snapshot fields to the terminal workspace manifest payload.
- [ ] 1.2 Keep old workspace manifests decodable when transcript snapshot fields are absent.
- [ ] 1.3 Enforce transcript row and encoded-byte limits, tail truncation, and truncation metadata during manifest encoding.
- [ ] 1.4 Preserve pinned-tab structural snapshots while storing close-time transcript continuity metadata separately or as an overlay.

## 2. Runtime Snapshot Capture

- [ ] 2.1 Add a `TerminalTranscriptSnapshot` model with content identity, cwd, title, dimensions, viewport, transcript text, process summary, capture time, and truncation metadata.
- [ ] 2.2 Add a terminal runtime service capture API keyed by terminal ContentInstance identity.
- [ ] 2.3 Capture transcript state from Ghostty surfaces when available and fall back to a bounded service-owned transcript ring buffer when needed.
- [ ] 2.4 Return explicit capture failures and diagnostics without exposing Ghostty renderer objects, PTY handles, child process handles, or delivery queues.

## 3. Close Guard

- [ ] 3.1 Add a close-impact collector for PaneSlot, Tab, window, app, and Quick Terminal close intents.
- [ ] 3.2 Classify active terminal work from foreground command, alan session, pending yield, exited process, idle shell, and unknown live runtime metadata.
- [ ] 3.3 Route pane title-bar, sidebar, menu, keyboard shortcut, window close, app quit, and Quick Terminal close paths through the close guard.
- [ ] 3.4 Present one interactive confirmation per requested close scope when active terminal work is present.
- [ ] 3.5 Ensure cancelling confirmation leaves shell state, workspace manifest state, and terminal runtime state unchanged.
- [ ] 3.6 After confirmation, capture affected terminal transcript snapshots before applying close mutation and runtime finalization.

## 4. Control Plane Close Semantics

- [ ] 4.1 Add stable `requires_confirmation` result semantics to close command DTOs when the target contains active terminal work.
- [ ] 4.2 Route control-plane PaneSlot and Tab close commands through the close guard.
- [ ] 4.3 Ensure confirmation-required control close responses do not mutate shell state or finalize terminal runtimes.
- [ ] 4.4 Preserve existing authoritative close responses for idle, exited, missing, and non-terminal targets.

## 5. Restart Restore

- [ ] 5.1 Materialize manifest transcript snapshots into terminal runtime creation payloads.
- [ ] 5.2 Seed newly created terminal runtimes with restored transcript history before normal user input is accepted.
- [ ] 5.3 Start a fresh shell in the restored cwd after seeding transcript history.
- [ ] 5.4 Restore transcript history without adding normal-mode restored-session banners or warning chrome.
- [ ] 5.5 Degrade alternate-screen snapshots to captured readable transcript history without claiming the prior application is still running.

## 6. Verification

- [ ] 6.1 Add focused tests for active close confirmation, idle close bypass, cancel-no-mutation behavior, and one confirmation for multi-pane scopes.
- [ ] 6.2 Add control-plane tests for `requires_confirmation`, idle close success, and unchanged shell/runtime state after guarded close rejection.
- [ ] 6.3 Add manifest round-trip tests for old manifests, bounded snapshot payloads, truncation metadata, pinned-template overlays, and unmatched transcript discard.
- [ ] 6.4 Add runtime service tests for live snapshot capture, ring-buffer fallback, explicit capture failure, and restored transcript seeding.
- [ ] 6.5 Run a running-app smoke that produces visible output, quits through a confirmed path, relaunches a fresh installed app, verifies prior output is visible, and verifies new input goes to a fresh shell at the restored cwd.
- [ ] 6.6 Run `openspec validate persist-macos-terminal-session-snapshots --strict`.
- [ ] 6.7 Run the focused Apple build/test commands touched by the implementation.

## 7. Review And Archive Readiness

- [ ] 7.1 Document that true PTY/process survival and daemon-backed terminal attach remain future work.
- [ ] 7.2 Prepare PR review notes covering close guard scope, transcript bounds, old-manifest compatibility, and restart smoke evidence.
- [ ] 7.3 After implementation is merged, sync accepted spec deltas into `openspec/specs/` before archiving the change.
