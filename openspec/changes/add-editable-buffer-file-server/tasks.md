## 1. Crate Setup

- [x] 1.1 Add `crates/editfs` as workspace crate `alan-editfs`.
  Done 2026-07-03: added `crates/editfs` to the workspace and workspace
  dependency table.
- [x] 1.2 Export a public `EditFs` aP file-server type and execution policy
  types.
  Done 2026-07-03: `alan-editfs` exports `EditFs`, `ExecutionPolicy`,
  `AddressRange`, and mount/handle constants.

## 2. Buffer Files

- [x] 2.1 Serve root directory entries `body`, `tag`, `addr`, `ctl`, and `event`.
  Done 2026-07-03: root reads list the five buffer files.
- [x] 2.2 Implement `body` and `tag` as UTF-8 document files with
  commit-on-clunk writes.
  Done 2026-07-03: writes buffer per fid and become visible only at clunk.
- [x] 2.3 Reject invalid UTF-8 at commit time without changing visible content.
  Done 2026-07-03: invalid UTF-8 clunk returns `BadRequest` and preserves prior
  text.

## 3. Address And Control

- [x] 3.1 Implement `addr` writes using `rev:<revision> <start>..<end>` and
  reads using `rev:<revision> addr:<addr-revision> <start>..<end>`.
  Done 2026-07-03: `AddressRange` parses revision-bound range writes and
  displays the selected range snapshot with an address revision.
- [x] 3.2 Reject stale or invalid address snapshots when `ctl exec` consumes the
  caller-supplied range.
  Done 2026-07-03: `ctl exec` requires the current body revision, current
  address revision, exact active range, and UTF-8-safe range boundaries.
- [x] 3.3 Implement explicit `ctl exec` with default-denied and test-accepted
  execution policies.
  Done 2026-07-03: `ExecutionPolicy::DenyAll` is default and `AcceptAll` is
  available for harness verification.

## 4. Event Stream

- [x] 4.1 Append JSON-line events for body/tag edits, address changes, and exec
  accepted/denied outcomes.
  Done 2026-07-03: edit, address, and exec records are appended as JSON lines.
- [x] 4.2 Expose `event` as a retained blocking-read stream.
  Done 2026-07-03: `event` is backed by `alan_ap::Stream`.

## 5. Verification

- [x] 5.1 Add focused `alan-editfs` integration tests covering root listing,
  body/tag commit, invalid UTF-8 rejection, addr selection, stale addr rejection,
  accepted/denied exec events, and blocking event reads.
  Done 2026-07-03: `crates/editfs/tests/editfs.rs` covers all listed cases.
- [x] 5.2 Run `cargo test -p alan-editfs -- --nocapture`.
  Done 2026-07-03: focused editfs tests passed.
- [x] 5.3 Run `cargo clippy -p alan-editfs --all-targets --all-features -- -D
  warnings`.
  Done 2026-07-03: focused editfs clippy passed with warnings denied.
- [x] 5.4 Run `openspec validate add-editable-buffer-file-server --strict`.
  Done 2026-07-03: strict OpenSpec validation passed.
- [x] 5.5 Run `git diff --check`.
  Done 2026-07-03: diff whitespace check passed.
- [x] 5.6 Run `just verify`.
  Done 2026-07-03: full workspace verification passed.

## 6. PR Hygiene

- [x] 6.1 Commit this slice separately from the editable-buffer contract PR.
  Done 2026-07-03: committed as `6533e527`.
- [x] 6.2 Open a stacked PR on top of
  `feat/northstar-editable-buffer-contract` and mark it ready for review.
  Done 2026-07-03: opened ready PR #593.
