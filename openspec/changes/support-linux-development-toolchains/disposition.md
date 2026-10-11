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

PR #1046 remains unmerged and prepared for formal review. Its previous
source candidate cace913c and documentation head 7702ce55 passed all sixteen
distinct checks; those checks do not qualify the subsequent production repair.
Current-head review found overbroad installed-Rust runtime projection. The repair
retains only runtimes selected by the actual command's bounded inspection and
hashes only their runtime manifests/executables. The reported direct `sh -c`
before-effects gap is not reachable in the shipped Linux entry: its full shape
guard rejects direct/nested shell evaluators before environment construction.
Actual native negative controls retain that refusal without admitting new wrapper
execution. Review follow-up and retained failures are in implementation-evidence.md.

The published `258a956a` head passed all sixteen checks, but five subsequent
inline comments require their own follow-up. Four valid gaps are repaired in
the existing Rustup inspector: bounded direct run dispatch/helper checks,
before-effects management refusal, no unrelated home/catalog inspection for
startup/non-Rust commands, and failed-cd branch retention. The env-options claim
is already prevented by the existing Linux Full guard; actual baseline and final
negative controls verify that boundary. Latest portable/native proof, original
failures and precise limits are recorded in implementation-evidence.md. Final
native Engine is 1434/0/1 with 31 actual case labels; final macOS Engine is
1420/0/1 plus 20 integration tests. These checks add no real-model slots and do
not reuse the earlier source receipt as final-candidate evidence.

Subsequent literal-target P1 `4240005485` prevents closure despite all sixteen
checks passing at `a14316c2`. It is reproduced with five native late-marker
failures. The shared component predicate now checks literal CLI targets,
including direct Rustup-run and Cargo-forwarded compiler options, while preserving
host-tuple and application/test argument semantics. Existing helper checks also
recognize Cargo global options. Final native Engine is 1435/0/1 with 38 actual
case diagnostics; final macOS Engine is 1421/0/1 plus 20 integration tests.
The follow-up still requires its own normal commit/current-head CI and review
reply/resolution. This does not qualify general Cargo configuration, custom
targets, cross-compilation or any additional real-model slot.

Tasks remain **13/15 complete**: formal review and user merge/canonical closure
are outstanding. The repaired candidate requires its own native regression,
normal-commit quality/distribution and completed current-head CI; no earlier
checks or real-model slots are substituted.

The ADR-0058 dependency extension is a reviewable draft candidate pending user
adoption/merge; the delivered main single-grant baseline is unchanged. Wider
qualification is independently tracked in draft PR #1047: all fifteen macOS
slots qualify on the final candidate through thirty-four development attempts;
all fifteen Linux slots remain NOT_RUN for missing generation readiness. These
are that change's receipts, not this delivery's native Tool tests. Keep
fallback/approval, network and automatic-routing
posture unchanged; do not start Alan self-development instances in place of
Codex-authored implementation.
