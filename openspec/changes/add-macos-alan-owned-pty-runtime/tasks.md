## 1. Dependency Source

- [ ] 1.1 Choose and document the repository path for the Alan-maintained Ghostty fork submodule.
- [ ] 1.2 Add the Ghostty fork submodule at the chosen path and pin the reviewed revision.
- [ ] 1.3 Update Ghostty setup scripts to prefer the pinned submodule source while keeping explicit developer overrides visible.
- [ ] 1.4 Record Ghostty artifact revision/cache metadata so setup checks can detect stale artifacts.

## 2. Alan PTY Runtime Foundation

- [ ] 2.1 Add Alan-owned PTY/process runtime protocols for allocation, launch, resize, input, EOF, signals, exit observation, and bounded transcript capture.
- [ ] 2.2 Add fake PTY and fake process-controller implementations for focused tests.
- [ ] 2.3 Add runtime selection plumbing that can choose the existing Ghostty-owned path or the Alan-owned PTY path without changing default behavior initially.
- [ ] 2.4 Route terminal launch profiles and environment projection into the Alan-owned PTY runtime path.

## 3. Ghostty Attachment

- [ ] 3.1 Identify the Ghostty fork seam required to attach a renderer to an externally owned PTY endpoint.
- [ ] 3.2 Patch the Alan-maintained Ghostty fork with the minimal external-PTY attachment support needed by Alan.
- [ ] 3.3 Update Alan's Ghostty bridge to attach to Alan-provided PTY endpoints in the Alan-owned runtime path.
- [ ] 3.4 Preserve renderer readiness, surface lifecycle, scrollback, input adapter, and metadata projection behavior across the new attachment path.

## 4. Process Control And Lifecycle

- [ ] 4.1 Move resize and text delivery for the Alan-owned path to Alan PTY handles.
- [ ] 4.2 Implement Alan-owned interrupt, terminate, kill, EOF, and forced-close result reporting.
- [ ] 4.3 Project Alan-owned process state, foreground work, exit status, and signal diagnostics into terminal ContentInstance metadata.
- [ ] 4.4 Keep app-restart restore semantics snapshot-based unless a later daemon-owned PTY runtime is specified.

## 5. Verification

- [ ] 5.1 Add focused fake-runtime tests for PTY allocation, launch, text delivery, resize, signal delivery, exit observation, and snapshot capture.
- [ ] 5.2 Add setup checks for missing Ghostty submodule, stale artifacts, explicit overrides, and pinned-revision metadata.
- [ ] 5.3 Extend the Ghostty integration lane to cover Alan-owned PTY renderer attachment when local artifacts are prepared.
- [ ] 5.4 Run `openspec validate add-macos-alan-owned-pty-runtime --strict`.
- [ ] 5.5 Run focused Apple shell contract or build checks relevant to dependency setup and terminal runtime boundaries.

## 6. Review And Archive Readiness

- [ ] 6.1 Keep the first PR spec-only unless implementation scope is explicitly requested.
- [ ] 6.2 After implementation lands, sync accepted delta specs into `openspec/specs/`.
- [ ] 6.3 Archive the completed OpenSpec change after synced specs validate.
