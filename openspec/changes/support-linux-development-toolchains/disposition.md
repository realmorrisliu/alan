# Disposition — 2026-10-10

Implementation is authorized by the user's ordered development-qualification
goal. This independent second delivery follows merged UI implementation PR #1044;
post-merge UI/directory canonical closure remains separate in PR #1045.

Supported PATH, nested-mount safety, installed standard-Rustup selection, private
per-command environment, read-only project builds and descendant cancellation/
timeout are implemented and actually exercised on Linux. Live-service Bash runs
Git 2.53/Rust 1.97, semantic RED/EditFileTool/GREEN and exact diff/status. The
candidate additionally reads live same-Process read-only dependency grants while
retaining only the selected cwd grant's writable authority. Linux compiled tests
verify dependency write denial and other writable/foreign grant access denial;
source aliases and revocation after successful external compilation fail without
consuming canaries or accepting retained output as a new success.

Acceptance found and repaired two shared authority issues: invalid non-root cwd
could silently choose another writable project, and canonical root retargeting
could move sandbox authority. The common Host constructor now refuses those
states, missing roots and non-directories with public-only diagnostics. Root
validation is a reconciliation-time check, not an atomic concurrent Host-mutation
or inode-pinning guarantee. Native diagnostic projection continues to conceal
private backing ancestors without inventing authority.

Final root-checked candidate passes Linux engine 1430/0/1, Host 58/0/2 and
warnings-denied Clippy; macOS Host passes 57/0/2 portable shared-adapter coverage.
Pinned strict OpenSpec is 69/69. Exact source/binary/log hashes, retained failures,
unchanged 379-node Rust and 217-entry Git inventories, and limitations are in
implementation-evidence.md. macOS arbitrary-reader OS confinement is not inferred.
Original private-root Git PATH and native wrappers remain explicit refusals.

Draft PR #1046 remains unmerged. Its previously published ce7aa5dd head passed all
16 distinct checks; they do not qualify this new candidate. Publication requires
normal-commit full quality and standalone distribution, followed by its own
current-head CI. Tasks are **12/15 complete**; final gates, review/CI and user
merge/canonical closure remain open.

The ADR-0058 dependency extension is a reviewable draft candidate pending user
adoption/merge; the delivered main single-grant baseline is unchanged. Wider
qualification is independently planned in draft PR #1047: thirty real-model
slots remain NOT_RUN. Keep fallback/approval, network and automatic-routing
posture unchanged; do not start Alan self-development instances in place of
Codex-authored implementation.
