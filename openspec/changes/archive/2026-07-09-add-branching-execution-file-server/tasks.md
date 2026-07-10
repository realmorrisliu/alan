## 1. Crate Setup

- [x] 1.1 Add `crates/branchfs` as workspace crate `alan-branchfs`.
  Done 2026-07-03: added `crates/branchfs` to the workspace and workspace
  dependency table.
- [x] 1.2 Export public `BranchFs`, `BranchRecord`, and branch command/status
  types plus mount/handle constants.
  Done 2026-07-03: `alan-branchfs` exports `BranchFs`, `BranchRecord`,
  `BranchStatus`, `BranchCommand`, and mount/handle constants.

## 2. File Surface

- [x] 2.1 Serve root directory entries `ctl`, `branches`, `selected`, and
  `events`.
  Done 2026-07-03: root reads list all four files.
- [x] 2.2 Serve `branches/` as the listing of visible branch ids and
  `branches/<id>` as inspectable JSON branch metadata.
  Done 2026-07-03: `branches/` lists visible ids and `branches/<id>` returns
  `BranchRecord` JSON.
- [x] 2.3 Serve `selected` as the explicit selected-branch JSON view.
  Done 2026-07-03: `selected` returns `null` before selection and branch id/root
  JSON after explicit selection.
- [x] 2.4 Expose `events` as a retained blocking-read stream.
  Done 2026-07-03: `events` is backed by `alan_ap::Stream`.

## 3. Branch Operations

- [x] 3.1 Implement bootstrap helpers for installing a visible base branch backed
  by an `alan-knowledge` checkpoint root.
  Done 2026-07-03: `install_base_branch` creates a checkpoint root and visible
  base branch.
- [x] 3.2 Implement `ctl` fork commands that name an existing visible source
  branch and write only divergent delta blocks through cheap knowledge forks.
  Done 2026-07-03: fork commands call `KnowledgeStore::fork_append_bytes` and
  tests verify one new block/node for a divergent branch.
- [x] 3.3 Reject fork commands whose source branch is not visible.
  Done 2026-07-03: unknown source branches fail with `ErrorCode::NotFound`.
- [x] 3.4 Implement explicit score commands and publish score/summary in
  `branches/<id>`.
  Done 2026-07-03: score commands update branch JSON and append score events.
- [x] 3.5 Implement explicit select commands and publish the selected branch
  through `selected`.
  Done 2026-07-03: select commands mark the selected branch and update
  `selected`.
- [x] 3.6 Implement discard commands that hide discarded branches while retaining
  discard events.
  Done 2026-07-03: discard removes the branch from `branches/` and appends a
  discard event.

## 4. Verification

- [x] 4.1 Add focused `alan-branchfs` integration tests for root listing, branch
  JSON, cheap fork sharing, unknown-source rejection, scoring, selection,
  discard, and blocking event reads.
  Done 2026-07-03: `crates/branchfs/tests/branchfs.rs` covers all listed cases.
- [x] 4.2 Run `cargo test -p alan-branchfs -- --nocapture`.
  Done 2026-07-03: focused branchfs tests passed.
- [x] 4.3 Run `cargo clippy -p alan-branchfs --all-targets --all-features -- -D
  warnings`.
  Done 2026-07-03: focused branchfs clippy passed with warnings denied.
- [x] 4.4 Run `openspec validate add-branching-execution-file-server --strict`.
  Done 2026-07-03: strict OpenSpec validation passed.
- [x] 4.5 Run `git diff --check`.
  Done 2026-07-03: diff whitespace check passed.
- [x] 4.6 Run `just verify`.
  Done 2026-07-03: full workspace verification passed.

## 5. PR Hygiene

- [x] 5.1 Mark `add-content-addressed-knowledge` follow-up 5.1 as covered by
  this separate change.
  Done 2026-07-03: `add-content-addressed-knowledge` now points this follow-up
  to `add-branching-execution-file-server`.
- [x] 5.2 Commit this slice separately from the `editfs` PR.
  Done 2026-07-03: committed as `5501fa92`.
- [x] 5.3 Open a stacked PR on top of `feat/northstar-editfs` and mark it ready
  for review.
  Done 2026-07-03: opened ready PR #594 on top of `feat/northstar-editfs`.
