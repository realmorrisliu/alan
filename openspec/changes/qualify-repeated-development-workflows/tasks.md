## Latest Cargo root query dispatch repair — 2026-10-13

All sixteen checks passed on c9bfe9a3, but its completed review found valid
P2 #4249812214: root queries before a trailing frontend require unused children.
The direct repair uses one shared option parser on ordinary and Rustup Cargo;
root help precedes alias expansion, while version/list still inspect selected
aliases and refuse default-short-alias combinations that can dispatch. Named or
frontend help retains its own checks. Real208 child probes retain200 exit0 and
8 genuine default-alias failures, alongside40 portable/96 native checks and
450 original scripts/310 Cargo argument parity. Original30 slots/85 attempts,
first outcomes and all limits remain unchanged. Task4.4 stays open17/19 for the
new committed-head CI/completed review; user merge remains separate. Entries
below record historical delivery states, not current merge readiness.

## Latest Cargo frontend query repair — 2026-10-12

All sixteen checks passed on de04ab8d, but its completed review found valid
P2 #4249496754: ordinary cargo fmt --help unnecessarily required later-proxy
rustfmt. Known fmt/Clippy help and proven Clippy version queries now bind only
their actual frontend, while still inspecting configuration and selected aliases.
Fmt version/forwarded payload and custom commands retain conservative dependencies.
Eleven real owned-runtime child probes, 38 portable checks and 94 native namespace
checks pass, including eight actual confined queries without ordinary compilers.
Old portable/native false-refusal controls and harness failures are retained.
Original 30 slots/85 attempts and limits stay unchanged. Task 4.4 remains open
(17/19) for the new committed-head CI/completed review; user merge is separate.
Earlier delivery entries below are historical, superseded by this current status.

## Latest shell lookup, cwd and Cargo helper repair — 2026-10-12

All sixteen checks passed on b0034bd9, but its completed automated review found
three additional valid findings: #4248653241/function shadowing,
#4248653247/newly created cd targets and #4248653257/auxiliary Cargo frontends.
Merge readiness remains withdrawn and task 4.4 remains open (17/19). The direct
repair uses the existing Bash syntax parser to reject actual functions in Rust
requests while preserving quoted Rust source/comments/heredocs, rejects alias/hash
lookup changes and unqualified nested Rust shells, and treats missing cd targets
as uncertain. Ordinary Cargo freezes command-specific PATH helpers, including
rustdoc for documentation/default doctests and fmt/clippy/custom frontends.
Fresh private Cargo home is initially empty; in-request helper mutation remains
unqualified. No extra runtime or authority is granted. Original 30 slots/85
attempts and all first outcomes remain unchanged. Native and source applicability
are recorded in native-runs.md; committed-head CI/re-review and user merge remain.

## Latest compiler configuration and query repair — 2026-10-12

Completed review of 8c769439 reported P1 #4247811233 (compiler overrides) and
P2 #4247811240 (compiler-free Cargo commands). Both require repair before merge.
Compiler-using invocations now inspect inline, extra-file and visible ancestor
Cargo configuration, reject unsupported compiler/wrapper/context overrides and
selected aliases, and freeze validated bytes or absence for prelaunch revalidation.
Only proven compiler-free commands skip the implicit PATH rustc check. Real Cargo
1.97 probes show plain metadata/clean can invoke rustc; metadata --no-deps does not.
Custom help, aliases, directory remapping and bounded literal configuration writes
receive the same checks. Parent-traversing cd refuses uncertain native selection.
No extra authority or compiler installation is added. Opaque program-internal
configuration mutation within one shell request remains unqualified; separate
configuration editing from execution. The original 30 slots/85 attempts and their
first outcomes remain immutable. Task 4.4 stays open (17/19) until committed-head
CI and completed automated re-review; user merge remains separate under 4.5.
See [compiler/configuration evidence](native-runs.md#compiler-configuration-and-query-follow-up--2026-10-12).

## Latest effective-provider review repair — 2026-10-12

Completed review of 06f15be3 found valid P1 #4247205400: a later Rustup proxy
must not determine inspection for an earlier ordinary PATH executable. The direct
repair selects the first regular X_OK provider (or explicit absolute executable),
freezes its path/content/proxy classification, and rechecks before effects.
System Cargo's default PATH compiler decision is frozen too. Unproved alias
chains and context-changing wrappers refuse conservatively. No mount authority
or unrelated runtime is added. Earlier delivery entries below are historical;
current-head CI and completed automated review remain open under task 4.4 (17/19).
See [the latest native evidence](native-runs.md#effective-path-provider-follow-up--2026-10-12).

## Latest exact-alias review repair — 2026-10-12

Completed review of 10a475d1 found P1 #4246987405: raw cwd/Host-root comparisons
could discard an override keyed to an alias's exact canonical target before
selection was canonicalized. The direct repair snapshots canonical inspection
cwd/roots before override filtering, preserves original inputs for plan validation,
and starts implicit directory inspection from canonical cwd. Override keys and
selection priority are unchanged. Task 4.4 remains open and checklist is 17/19.
Seventeen portable regressions pass, including remapped and identity alias-only
cases without a masking canonical child mount. Actual 10a native RED and four-case
GREEN are retained; twelve old-10a/new complete environment comparisons pass for
already-canonical inputs. Original matrix applicability is semantic equality of
its already-canonical cwd/backing roots, not an unchanged raw identity code branch.
Current committed-head gates and completed automated re-review remain required.

## Latest post-repair automated review — 2026-10-12

The Cargo manifest P2 was fixed in d719f3b6 and resolved; that head passed all
sixteen checks. Its completed automated re-review found a valid Rustup remap P1
(#4246709180). Merge readiness remains withdrawn and task 4.4 remains open.
The direct repair preserves Host precedence checks and projects inspected effective
selectors at all validated equivalent namespace directories after inspection.
Identity mounts retain their original settings/selection branch; this does not
assume historical user override maps were absent. Portable and native regression,
new source applicability, independent review and committed current-head CI/review
must close before readiness is restored. Checklist remains 17/19; user merge and
canonical closure remain separate. No automatic merge is authorized.

## 1. Preconditions and executable evidence

- [x] 1.1 Verify predecessor Linux toolchain/ADR-0058 candidate authority qualification, code review and current-head CI; keep user merge/canonical closure as final-delivery gates; pin UI/toolchain revisions without substituting prior scripted/native UI receipts.
- [x] 1.2 Trace existing CLI/Process/AgentFS/rollout, Host grant controls, harness collectors and terminal evidence paths; freeze supported operator controls and actual owned stores.
- [x] 1.3 Prepare five-family disposable fixture variants, protected expected-result checks and exact baseline/effect inventories; self-check new nontrivial collection/assertion logic for missing and duplicate evidence.
- [x] 1.4 Verify actual model/profile/medium effort and enforcing backend on both Hosts; freeze all thirty slot prompts/inputs, bounds and measurement fields before generation, and each Host's source/binary/readiness identities before generation on that Host. Keep this task open until both Hosts are ready; ready-Host execution may proceed independently.

Linux's first native task freeze on 2026-10-12 pins unchanged candidate 2277b693,
the actual known ChatGPT catalog and gpt-6.1-sol/medium selection, Git 2.53.0 and
Rust/Cargo 1.96.0 in the selected task cwd under linux_reified_namespace. Shared
macOS auth is explicitly user-authorized; it is not credential independence.
Readiness and input preparation add no completed development slot.

## 2. macOS real-model tasks in Herdr

The checks below retain completed historical repetitions. The shared-projector
review repair reopens final-source applicability for all fifteen macOS rows.
Six linked F1/F2 refreshes passed on `465e3e47`. The subsequent prelaunch-source
F3 r1 a3 fails UI qualification despite independently correct code. Task 4.2
requires applicable final-candidate native evidence for all fifteen rows. The
notice repair now has three qualified F3 repetitions, including a fresh retry
after an insufficient-detail capture audit. F1 r1 a6 also qualifies on the
byte-identical documentation head ed6a65e6. F1 r2/r3 a4 qualify on the
byte-identical 95d6664c head. All three F2 refreshes now qualify on byte-identical
d97d7f70. All three F4 refreshes now qualify on byte-identical 19665cd9,
retaining one intervening collector-ordering FAIL and its fresh retry. Aggregate
v59 retains fifty-five attempts and twelve applicable slots. All three F5
refreshes also qualify on byte-identical 81b6f361: aggregate v63 retains
fifty-eight attempts and all fifteen macOS slots. All fifteen Linux tasks remain.
Historical checkmarks do not qualify new behavior.

- [x] 2.1 Run small RED-to-GREEN repair three times with real model-authored code, independent behavior/diff checks and retained first outcomes.
- [x] 2.2 Run cross-file changes three times with all required callers updated and no unrelated edits.
- [x] 2.3 Run actual compiler/test/tool failure correction three times with retained diagnostic and cause verification.
- [x] 2.4 Run cancel/revoke/explicit recovery three times, observing live descendants and once-only completed effects.
- [x] 2.5 Run long stdout/stderr/diff follow-up three times with retained details, result correlation and independent bounded edit/answer verification.

## 3. Linux real-model tasks in ordinary PTY

- [x] 3.1 Run small RED-to-GREEN repair three times under the qualified enforcing backend and exact selected tools.
- [x] 3.2 Run cross-file changes three times with exact sources, artifacts and granted authority.
- [x] 3.3 Run actual compiler/test/tool failure correction three times; refuse false passes and record unsupported slots separately.
- [x] 3.4 Run cancel/revoke/explicit recovery three times, verifying blocked later effects and no replay after reapproval.
- [x] 3.5 Run long stdout/stderr/diff follow-up three times with retained detail and exact follow-up effects.

All three Linux F2 repetitions qualify on unchanged be25825a. The original r1
checker-path failure and fresh retry remain retained; r2/r3 pass their original
first attempts. Protected library behavior, both binary results, exact three-file
effects, native detail/draft and exits pass. Task 3.2 closes.
All three Linux F3 repetitions also qualify on unchanged 23e169e6, retaining
actual E0425/E0583/E0308 diagnostics before model-only lib repairs, original
protected behavior, failed-Action details, Unicode draft return and native exits.
Task 3.3 closed at checklist 12/19. r1 retains its incremental-heading collector
correction; r2/r3 have none. Six Linux F4/F5 tasks were still open at that point.
Linux F4 now qualifies three assisted repetitions on unchanged 0f2958d9. Each
retains cancel/130 and native group exit, blocked late effects, revoke/actual denied
read, exact owned recovery, once-only ledger, model-authored lib repair, protected
behavior, native detail/draft and exits within 600 seconds. One corrective native
directory selection and one factual model acknowledgement are recorded per run;
none is zero-unplanned-intervention. Seven failed development attempts and two
zero-model collector preflights remain retained. Task 3.4 closes; checklist is
13/19. The three Linux F5 repetitions remained open at that historical point.

## 4. Findings, final candidate and delivery

- [x] 4.1 Review first outcomes and retained evidence by platform/family; reproduce any observed UI/reliability defects and identify existing owners before code edits.
- [x] 4.2 Fix observed shared causes, run affected Rust/collector checks and final-candidate native reruns, including narrow-terminal/cross-host smoke where implicated; retain original attempts and truthful unresolved limitations.
- [x] 4.3 Audit all thirty slots and corrected attempts for model authorship, lifecycle, terminal and exact effect proof; publish first-attempt/assisted/zero-unplanned-intervention outcomes, wall times and missing usage explicitly.
- [ ] 4.4 Run full quality, strict OpenSpec and applicable distribution checks, review the final diff, resolve findings and collect passing current-head CI in reviewable PRs.
- [ ] 4.5 After user merge, verify revisions/checks, sync the harness and observed-repair deltas to canonical owners, update disposition and verify archive-readiness.

## Final native and evidence audit — 2026-10-12

All three Linux F5 repetitions now qualify, retaining their original collector
failures and the additional 600-second diff timeout. No original bound, prompt,
authority, producer-once requirement or protected assertion changes. Checklist
is 17/19 after tasks 3.5 and 4.1–4.3 close. Aggregate v93 retains 85 development
attempts, first outcomes 21 PASS / 9 FAIL / 0 NOT_RUN, and 30 applicable slots.
The independent audit verifies all attempt receipt hashes and 4,694 artifacts
for the selected completions. Per-family wall times, assisted outcomes, collector
interventions and missing provider cost/usage are published in native-runs.md.

The two final review findings concern test placement; both are repaired without
production changes or widened visibility, and eight focused tests pass.
Unchanged production bodies and the other frozen inputs preserve existing native
runtime applicability; raw all-input equality is no longer claimed. Long detail
navigation remains laborious (Home and 10-line Space pages, no End); this is a
reported usability limit, not a missing proof or a newly implemented feature.
Task 4.4 still requires the committed head's review and full CI. Task 4.5 waits
for user merges and canonical closure. This matrix does not qualify general
unassisted self-bootstrap, automatic routing or statistical independence.

## Current-source delivery gates — 2026-10-12

Committed head ceb71ee6 passes all sixteen current-head checks, including both
platform suites, quality, coverage, release builds and CodeQL. Both final review
axes are resolved and normal quality/distribution plus strict OpenSpec 70/70
pass. Task 4.4 closes; checklist is 18/19. This documentation-only recording
still needs its own CI; no user merge or canonical adoption is claimed. Task 4.5
remains open for user merges of #1045/#1046/#1047 and subsequent canonical closure.

## Post-ready automated review follow-up — 2026-10-12

Marking #1047 ready triggered Codex review. Its subsequent P2
`discussion_r4246031780` is valid: Cargo selectors/global options hide the
subcommand from the manifest read-role check. The previous gate closure covers
its historical head, not completion of that later review. Task 4.4 reopens;
checklist is 17/19 until the repaired head's CI and completed automated re-review.

The existing Cargo parser is shared with Rustup inspection without changing its
body or surrounding selection code. Five manifest tests, two Rustup prefix tests,
six actual macOS Seatbelt prefix read/write boundaries and a fresh Linux native
fixture pass. Linux verifies three real prefix builds against read-only sources
and four negative pre-effect refusals, plus existing isolation/lifecycle guards.
A newly built standalone CLI also compiles a read-only manifest from writable
scratch with +1.97.0/--color, normal scoped approval, no model input and native exit0.

All 66 selected initial/resumed/verifier rollout references are validated. A
cache-only compiled comparison inspects 450 structured Bash argument occurrences
(including 193 completed records, duplicates and cancelled/proposed requests):
3,415 token roles agree with the original candidate, with no nested wrapper
scripts or parser refusals in this corpus. The unchanged Rustup parser/selection,
unchanged policy/rights owners, fresh 437/440-input binary freezes and targeted
native controls support inheritance of the same thirty slots. No original
receipt, first outcome or elapsed time is rewritten; no new model slot is added.
The independent applicability ledger and retained operational failures are in
native-runs.md. User merge/canonical closure remains task 4.5.
