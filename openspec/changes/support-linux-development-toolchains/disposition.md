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
could move sandbox authority; same-path directory replacement could disagree with
the retained file tree. The common Host constructor now refuses those
states, missing roots and non-directories with public-only diagnostics, reusing
the existing HostDirFs directory handle for identity comparison. Root
validation is a reconciliation-time check, not an atomic concurrent Host-mutation
or descriptor-bound native execution guarantee. Native diagnostic projection continues to conceal
private backing ancestors without inventing authority.

Final retained-root candidate has verified engine 1430/0/1 evidence reused from
unchanged sources/binary, fresh Linux Host 58/0/2 and HostFs 23/0/0 plus warnings-
denied Clippy. macOS Host is 57/0/2 and HostFs 23/0/0 portable shared-adapter
coverage. Native absolute-manifest builds use fresh output and actually compile.
Pinned strict OpenSpec is 69/69. Exact source/binary/log hashes, retained failures,
unchanged 379-node Rust and 217-entry Git inventories, and limitations are in
implementation-evidence.md. macOS arbitrary-reader OS confinement is not inferred.
Original private-root Git PATH and native wrappers remain explicit refusals.

PR #1046 remains unmerged and is prepared for formal review. Source candidate
cace913c passed all sixteen current-head checks, full quality and standalone
distribution. Its absolute-manifest and retained-directory identity evidence
remains scoped to the exact frozen source/binaries. The final boundary review
rechecks active same-PID grants, selected-only writable authority, invalid cwd,
retained-directory identity and per-launch reconciliation; no new finding or code
change is introduced by this status refresh. The new documentation head requires
its own normal-commit quality and current-head CI before promotion from draft.
Tasks remain **13/15 complete**: formal review and user merge/canonical closure
are outstanding; earlier-head checks are never substituted for new-head checks.

The ADR-0058 dependency extension is a reviewable draft candidate pending user
adoption/merge; the delivered main single-grant baseline is unchanged. Wider
qualification is independently tracked in draft PR #1047: all fifteen macOS
slots qualify on the final candidate through thirty-four development attempts;
all fifteen Linux slots remain NOT_RUN for missing generation readiness. These
are that change's receipts, not this delivery's native Tool tests. Keep
fallback/approval, network and automatic-routing
posture unchanged; do not start Alan self-development instances in place of
Codex-authored implementation.
