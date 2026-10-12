# Initial native development runs — 2026-10-10

The original three frozen macOS F1 inputs qualify through four normal-CLI real-model
executions in Herdr, without corrective model advice or operator source edits.
The original r1 attempt omitted prospective exact PATH/executable identity and
remains FAIL; its fresh linked retry passes without replacing that first outcome.
Historical aggregate v32 retains thirty-four development task attempts
and fifteen qualified macOS slots: eleven first PASS, four first FAIL and fifteen
Linux NOT_RUN. All fifteen matched the earlier frozen candidate, including the
three later F4 lifecycle reruns. One unintended verifier generation stays
separate. Task attempts are not API-call counts; F4 includes explicit continuation.
Parent `a6ea2518` changed the shared diagnostic projector, reopening final-source
applicability. Aggregate v38 retained six F1/F2 PASS refreshes on `465e3e47`.
The subsequent prelaunch publication repair `bdf14ab0` passes all sixteen CI
checks, but its native F3 r1 a3 fails UI qualification: the model's correction
and protected behavior pass, while the same retained error repeats in the live
footer. The narrow TUI repair `573cd12e` passes normal gates and all sixteen CI
checks. Aggregate v45 retains forty-five attempts, fifteen historical qualified
slots and three qualified F3 slots on this source. A detail-capture audit withholds
one earlier claimed PASS without changing its immutable receipt; a fresh retry
provides actual modal/page evidence. Byte-identical documentation head ed6a65e6
passes all sixteen checks, and F1 r1 a6 adds applicable small-repair evidence.
Aggregate v47 retains forty-six attempts and four final-source qualified slots.
Fresh F1 r2/r3 a4 qualify on byte-identical 95d6664c. Aggregate v50 retains
forty-eight attempts and six final-source qualified slots. Subsequent F2 r1 a5
and r2/r3 a4 qualify on byte-identical d97d7f70. Aggregate v54 retains fifty-one
attempts and nine final-source qualified slots. Three F4 refreshes qualify on
byte-identical 19665cd9, retaining one collector-ordering FAIL before a fresh
retry. Aggregate v59 records fifty-five attempts and twelve final-source slots.
All three F5 refreshes qualify on byte-identical 81b6f361; aggregate v63
retains fifty-eight attempts and all fifteen macOS slots. All fifteen Linux
tasks remain; older results do not qualify this repair. Original
eleven first PASS/four FAIL/fifteen NOT_RUN remain unchanged. Linux generation
readiness, task 1.4 and cross-platform final delivery remain open. The sections
below retain the chronological attempts.

## Candidate and receipts

The original four F1 executions use unchanged #1046 `cace913c`; the clean native checkout's CLI
SHA-256 is `b66362691d83e8f63867058b3a15afd9ddbbbc721ef151a23b93b2ef53c8d6fd`.
All four F1 admitted callable bindings and generation contexts confirm
`chatgpt-main`, provider `chatgpt`, model `gpt-6.1-sol`, reasoning effort `medium`.
These instances expose the same default Memory Store; no personal memory bodies
were collected or cleared, and statistical independence is not claimed.

Owned evidence root:
`~/Library/Caches/Alan/qualification/dev-20261010-10332030`.
The immutable thirty-input fixture manifest is `fixtures-v3/manifest.json`,
SHA-256 `b64ad0126be948a69d46f4936cf25caacc6ff8d69b61bcda6e2b37229c32b25e`.
The original `first-outcomes-v1.json` preserves the first two attempts, SHA-256
`d8689edaed159ed8777bc5fe3f3e2d35c1189ce65d6f00ce07d7195da387c81b`.
The preserved F1 thirty-slot snapshot is `first-outcomes-v2.json`, SHA-256
`8ce517dae2373e453e2585ce9fadcc57a365ccf7c97362a4b0484dc3f8babe1d`.

| Slot / attempt | Attempt outcome | Model task time | Submission through verified cleanup | Receipt SHA-256 |
| --- | --- | --- | --- | --- |
| macos-f1-r1 / a1 | FAIL: incomplete prospective tool identity | 31.88 s | 227.34 s | `343e968cf9d5f89eba55d1051974fd807262b2ccc0f02239d05cf440f3db2e2d` |
| macos-f1-r2 / a1 | PASS | 31.50 s | 228.62 s | `02cf4ac6e0c66483e771591db2f4882a3bb4d9742fbc7931e22adbb6d5f37841` |
| macos-f1-r3 / a1 | PASS | 29.20 s | 269.60 s | `9c9a1a6f46c6992d3e42e8ccfbb30623d5c7155a858cd06d2befc38cf2ab758b` |
| macos-f1-r1 / a2 | PASS retry; original FAIL retained | 31.30 s | 263.19 s | `71e2e6256265cefbbe25ddaba7eb6bae178a4a2d1809c285152d51aa2bfe2725` |

Receipts are `attempts/<slot>/<attempt>/receipt.json`. Model task time ends at the
retained final assistant message. Total time includes observation, planned detail
checks, instance exit and independent native verification. Model/Tool/wait time
partition, usage and cost remain unknown. All four attempts have zero unplanned model
steering and zero operator source edits; planned grants, the setup Tool approval,
detail observations and verifier lifecycle are recorded, not approval-free autonomy.

## Authorship, independent checks and lifecycle

Each execution first ran `cargo test --locked --offline`, retained a genuine contract
failure, changed only `src/lib.rs` with EditFile, then reran Cargo successfully.
The original tests, Cargo files and fixture data matched their frozen hashes.
The inputs exercise addition, absolute difference and an inclusive clamp; the
fresh retry repeats the original addition defect. Non-Git fixture command
failures and any failed absolute namespace operands remain in the receipts.

After every model invocation actually exited, a separate normal native invocation
received the project read-only, protected assertions read-only and dedicated
checker scratch read-write. From that scratch, ordinary explicit Cargo compiled
both the real project and protected checker into a previously absent target and
passed all protected behavior assertions. Project/assertion/driver hashes were
unchanged by verification. This F1 evidence does not claim general Host isolation
or canary coverage; those remain separate predecessor qualification evidence.

Runs use owned session `alan-development-10332030`, pane `w5:p1`, actual calibrated
80x24 PTY/Host geometry. r1 model boot is `a71a6d96-f62e-4a6a-a499-f2be1d48f895`,
native PID 73148, Root 8, rollout `d31c30b5-33ca-4a30-869b-6637d0699945`;
r2 model boot is `1a1f7b82-2193-4cec-b2a5-9b74e4c421b3`, native PID 76033,
Root 8, rollout `ba77ff93-5634-4d6a-8917-bc5e9d4bd338`. The full owning rollout filenames/hashes,
submission IDs, Action IDs and verifier boot/exit records are in each receipt.
Both model and verifier native PIDs were confirmed gone after `/quit`. r2 also
retains explicit exited status for every Tool Process before model exit and the
verifier Tool before verifier exit.

The subsequent r3 model boot is `bac46fe7-acb8-4e23-bc04-f4b089f8927c`, native
PID 86973, Root 8, rollout `c163ad6a-a07d-4177-b663-69952aaecce1`. The r1 retry
boot is `732b4f58-07ed-4b99-998d-f80d23e4be35`, native PID 88547, Root 8,
rollout `de12f59b-6629-423d-b2e3-935f2e8b2350`. Both retain exited Tool status,
draft/logical-cursor restoration and verifier call-ID correlation against the
actual SDK Action. Both model and verifier native PIDs exited after `/quit`.

r2 opened the final Cargo Action details with a nonempty draft and a cursor in
its middle. After returning, inserting a character produced `abcδXYZ`, proving
the same draft and logical insertion point survived. Raw detail and terminal
captures remain retained. This is bounded input/detail evidence; it does not
replace the later long-output family or broad UI qualification.

## Collector correction and observed product limitation

r1 did not freeze exact selected PATH/executable digests before generation. The
next runs captured selected paths, rustup proxy and real compiler/Cargo digests,
Git digest, versions and platform/kernel, then explicitly supplied that captured
PATH to the native invocation. Tool versions were independently checked through
the actual product before submitting generation. The fresh r1 retry uses only its
slot from `fixtures-r1-retry-v1`; duplicate unused prepared inputs add no matrix
slots. Its original prompt, source baseline and protected assertion hashes match
the first attempt, with distinct native project/checker paths and fresh targets.
The original first-attempt files/receipt remain unchanged. Selected proxy,
compiler, Cargo and Git hashes were rechecked unchanged after the later runs.

The qualification-only SDK observer now supports explicit bounded `read --list`
to discover actual Action/request IDs, using existing `Shell::ls_bounded` with
1,024 entries / 1 MiB limits. It still pins numeric Process paths and Host boot,
never boots an Agent, and never opens clone endpoints for allocation. Three
validation tests and warnings-denied Clippy pass. Observer source used by these
runs, before a formatting-only correction, has SHA-256:
`1b7834bddece99c07c8cd35f002d9f7691a19fbcb81f3ea865d87dde71228c9d`;
binary SHA-256 `6437f1fcb05ecca90f159083a103823321f3749f4ade0c3cad10303a810aaead`.
The runs recorded this then-uncommitted collector separately from production.
Its exact source and binary are preserved under `observer-builds/1b7834bd/` in
the owned evidence cache; future rebuilds cannot erase the original client.
Later runs use published collector `8540cf75`, with source SHA-256
`8346bb55e301572bcf042e14e6892d0a8a54fc2f0b00f424fa59c49022a61d4c`
and binary SHA-256 `6ff28b7f931ff806bdb837a8909e36af195e78d701dd1fee4127353165df9784`.
That head passed all sixteen CI checks, including both platform suites and CodeQL.

r1 Action a4 rejected an absolute `/mnt/project-request-1/src/lib.rs` operand in
a read-only `sed` command as outside host_mount; relative operands subsequently
succeeded, and EditFile addressed the same absolute namespace path successfully.
Tracing shows Bash passes command text directly to native Sandbox validation,
while file Tools use the Host adapter's namespace path resolution. Retain this as
an observed shell projection limitation to reproduce and review in the Host/Tool
owner before a repair. Do not replace strings indiscriminately, weaken containment
or grant native backing paths to the model. No production fix is claimed here.

The SDK-authorized r3 and r1 retry also expose a concrete UI location problem:
the successful namespace `cd` Action and subsequent Tools use the granted project,
but the context line still says `no project`. `observe_action_cwd` already retains
the correct public namespace cwd; `context_line` chooses `no project` whenever
the local picker receipt is absent. A repair belongs to `rust-inline-tui` and
must preserve local picker labels, narrow model/state priority and draft/detail
behavior while displaying observed cwd without claiming grant access. This
finding does not change F1 behavior outcomes or qualify the unfinished UI repair.


## Observed directory-context repair — 2026-10-11

The native r3 and r1 retry finding is repaired at the existing TUI owner. When
there is no local picker receipt, the header uses an observed namespace cwd other
than `/`; it does not manufacture a project label or access claim. Root replacement
clears previous cwd through the existing attachment reset. Picker receipt labels,
access and grant/control fencing retain their existing behavior.

Two new tests independently reproduce the original header and Root-reset failures.
After the repair, the complete TUI suite passes 370 unit and 12 integration tests;
normal `just quality` (including standalone/distribution) and CI-pinned OpenSpec
1.4.1 strict validation pass, 69/69. This owner delta preserves every existing
contextual-prompt scenario and adds external ordinary-cd/Root-reset cases.

Native acceptance uses a separate checkout based on `cace913c`, with only the exact
three-file TUI implementation/test patch; production execution policy is unchanged.
Its ordinary CLI SHA-256 is
`a16fec426a74682348ce293d741b569f4c8c14cd062a806dadef9ac99305abf3`.
Boot `eecf9e9d-b213-4710-847a-3ac462ab1928`, native PID `10608`, Root PID `8`,
ran in the existing owned Herdr pane. The public Host read-only grant is
`request-1`; actual completed Action `a0` selects `/mnt/project-request-1/src`.
The header displays that cwd and does not infer a friendly label or read-only
claim. Actual control frames prove 80x24 and 48x24 geometry. Detail retains Action
identity/original output, and cursor insertion produces `abcδXYZ` then `abcδεXYZ`
across detail return and resize. Completed Action `a1` returns cwd to `/`, restoring
the default `no project` context. `/quit` ends native PID 10608 and returns the
owned pane to its shell; the fixture marker and candidate/helper binary hashes
remain unchanged. Root replacement is proven by RED/GREEN regression, not claimed
as a native Root-switch test.

Owned cache `cwd-native-v1/` retains source/binary freeze, mount intent/ack,
public-SDK Action evidence, terminal captures/control frames and receipt SHA-256
`8dd76bdc57ad3ad8a0ebd5104e16793df5a5c26c576513e7c8afe89e46cda135`.
Two operator collection/control mistakes are retained separately: an initial wrong
Host readiness key failed before mount, and an unrecognized resize variant was
rejected before using the accepted terminal.resize command. The same native
instance was retained. This probe submits no generation, fills no matrix slot and
does not replace original model outcomes. Original four model executions remain
on the unchanged earlier runtime. Exact final-candidate applicability/reruns are
still required by the full qualification change.

Evidence head `2d700742` passed all sixteen current-head CI checks, retained in
`ci-2d700742-all16-final.json`. Those checks do not qualify the later UI repair.


## First cross-file model task — macOS F2 r1 / a1

First outcome is FAIL, with no qualified completion added. Receipt SHA-256:
`6c0336f1f3bc3712c30db4308ff05bad88112db6b490a4d7d15fb43954c1ac03`.
The original thirty-input manifest is unchanged. Updated complete snapshot
`first-outcomes-v3.json` SHA-256 is
`911b7cebb5d4f4371a925061be609097fa77c6b50dbb0bdbe18322dd68658857`:
four executed original slots, five real model attempts, two first PASS, two first
FAIL, twenty-six NOT_RUN, three qualified completions.

The new native candidate is `cace913c` plus the exact three-file cwd UI patch,
source diff SHA-256
`e5bc7140588213752878ba8da9d3c1322d97693fabde7a78666571048515666c`;
CLI SHA-256 remains `a16fec426a74682348ce293d741b569f4c8c14cd062a806dadef9ac99305abf3`.
The patch is delivered in qualification branch commit `3dd629bd`. Prelaunch and
ready receipts freeze prospective tools/PATH/config/source, actual Seatbelt,
model gpt-6.1-sol/medium and all original fixture hashes. Model boot
`e7060042-85fc-43f3-9ffa-2a8961eecb77`, native PID `17681`, Root PID `8`,
ran in the same owned 80x24 Herdr pane. The source diff changes exactly
`src/lib.rs`, `src/main.rs` and `src/bin/report.rs`. Actual Action `a7` compiles
library and both binaries, runs both Cargo binaries with output `20` and `-1`,
and reads back source. Original failed non-repository Tool evidence also remains.
No tests/config/input files changed, and all seven Tool Processes exited before
model `/quit`. Detail Action a7 and draft insertion `abcδXYZ` pass; the context
line shows the externally authorized cwd. Model task time is 37.35 seconds;
submission through observed verifier exit is 376.80 seconds, below the frozen
600-second bound. Zero corrective model advice and zero operator source edits.

Fresh verifier boot `6dcf80dc-d309-49a4-9345-4afb87d9b4fd`, native PID `18585`,
receives project/assertions read-only and checker scratch read-write. The original
compound command combines protected Cargo test and both binaries via explicit
manifest operands. Policy allows it as Write under Seatbelt/projected_host_paths;
Action a1 then fails preflight with `Command references path outside host_mount`
for the read-only project manifest before any checker compilation. The SDK and
rollout retain the original command, policy, result and exited Tool Process.
Current shared guard validates non-Read command operands as writable; this
read-only operand case must be reviewed separately from the earlier absolute
namespace operand in a Read command. No production path/authority repair is claimed.

A separately recorded library-only diagnostic command freshly compiles both
packages and passes three protected library assertions under the same verifier.
Its call ID correlates the public SDK Action to that verifier's rollout; all
protected/checker inputs and post-model source hashes remain unchanged. Both
verifier Tool Processes and the native instance exit. That narrower diagnostic
success does not erase the original full-check failure or qualify F2. The receipt
records observation-schema mistakes and this diagnostic follow-up separately;
no repeated generation or operator solution occurred. Task 2.2 and a full linked
retry remain open after owner-correct path/authority review and repair.


## Manifest preflight and temporary read-only root repair — 2026-10-11

The existing finite token-role parser now treats Cargo build/check/test/run
`--manifest-path` before `--` as an input read only with an OS-enforced backend.
Both direct-path and absolute-literal checks use the same role; overall command
policy remains Write. Other operands, outputs/redirections, unknown commands,
read-deny/protected paths, revocation/symlink escape and parser-only degradation
retain conservative checks. No command text or Host grant authority is rewritten.

The original-source positive regression fails before the repair. Added safety
coverage found a separate real Seatbelt gap: common temporary-directory write
allowances permit program-internal writes to an explicitly read-only source there.
The profile now denies writes to explicit read-only roots while preserving
explicit writable descendants. A Cargo-named fixture program actually reads its
manifest, writes only allowed scratch, attempts a source overwrite and receives
native denial; source bytes remain unchanged, including the read-only-ancestor
case. This is a security regression fixture, not a model or native Cargo success.
The initial failing profile test and corrected five-test runs remain in build logs.
A revocation test initially compared canonical roots with a noncanonical temporary
alias; that test fixture comparison was corrected without changing production
revocation behavior.

Complete engine tests pass 1406 with zero failures and one existing ignored test;
20 integration tests pass. Normal `just quality`, standalone/distribution and
CI-pinned OpenSpec 1.4.1 strict validation pass 69/69. Sandbox source remains below
the 1000-line gate, including the predecessor's retained-directory implementation.
Evidence head `5e600532` passed all sixteen CI checks; its immutable receipt is
`ci-5e600532-all16-final.json`. Those checks do not qualify this newer repair.

A separate native candidate combines cace913c, the prior UI patch and this exact
repair, source diff SHA-256
`ed033e32fd4f751dfad139440526233a65002d3c9e390c80165faed21be071fd`;
all nine changed/new file hashes are retained separately. CLI SHA-256:
`5715ccd9723a608cddc874806302d44370f17d63cdc07b21c5fe4681edf90654`.
Ordinary Herdr boot `d5f1e684-2c26-49de-989a-2357c4086a24`, native PID `40510`,
Root PID `8`, receives the original F2 project/assertions read-only and fresh
checker scratch read-write. The original compound check now freshly compiles
both packages, passes three protected library assertions and runs both real
Cargo binaries with output `20` and `-1`. Source, driver and assertion hashes
remain unchanged; actual Tool Process exits before native `/quit`, and PID 40510
is gone. No generation was submitted and no matrix slot is added by this probe.

Owned `manifest-native-v1/` retains freeze, grant intents/acknowledgements, original
command intent, public SDK Action output/result, terminal capture, exit and receipt
SHA-256 `98b35bd352c8c428aff6691480bb25ab3bfec9d668dc76ec71e9cd1872789597`.
Two initial collector source-equality assertions failed before CLI launch because
predecessor-specific sandbox files differ from main; the corrected collector pins
actual base/delta/file identities. It does not omit predecessor changes or alter
source. Original F2 failure remains immutable. Fresh linked input preparation
`linked-f2-r1-retry-input-freeze-v1.json` uses identical prompt/source/assertion
hashes in a new project and checker scratch; no retry generation or qualification
is claimed here. New head CI/review, final-candidate applicability and the other
families/platform remain required.

## macOS F2 r1 linked qualification retry — 2026-10-11

Fresh attempt `macos-f2-r1/a2` uses the same frozen prompt, source baseline and
protected assertions as original a1, in a new disposable project and scratch.
Original a1 remains FAIL with receipt `6c0336f1`; no source was operator-authored
and no corrective model advice was supplied. Candidate source and CLI identity
match the preceding native manifest/Seatbelt repair probe. Native model boot
`559f312a-86cd-4957-8458-ce748bf14289`, PID 47125, Root 8 runs actual
`gpt-6.1-sol`/medium. It updates only the requested three source files and verifies
the library and both binaries. Seven observed Tool Processes, including the planned setup command, exit. Detail return
and logical insertion preserve `abcδXYZ`; an immediate stale terminal read is
reobserved on the same instance without repeating input. Native `/quit` exits it.

A fresh normal-CLI verifier boot `f09780cd-4843-4fb7-9d60-d0ce8cc2a47f`, PID
49010, mounts project/assertions read-only and dedicated scratch read-write.
The original full compound Cargo command freshly compiles the protected checker
and both real binaries: protected behavior passes, outputs are `20` and `-1`.
Source, driver and assertions remain unchanged. Action a1's call ID is correlated
with its owned rollout's allow/Write/Seatbelt/projected-host-path policy audit.
Actual Tool `/proc/9` exits and native `/quit` leaves the owned shell foreground.
The collector first used the wrong audit field name; reading `sandbox_backend`
on the same Action corrects that assertion without executing another command.

Submission-to-final-message time is 37.656288 seconds; end-to-end receipt/cleanup
time is 566.260430 seconds, within the frozen 600-second bound. This total includes
operator observation/checking time; unavailable model/tool/wait partitions, usage
and cost remain unknown. Planned authorization and lifecycle controls are retained
and do not constitute approval-free autonomy. Receipt SHA-256:
`c9dc9be09fee288669b6560a21e5e69f3219a68c1cc371344fe56ca02eaabe12`.

Immutable aggregate `first-outcomes-v4.json`, SHA-256
`c8d502142da869017edf440b92221c8fa1336e2339b548db5f97f6864c3051ba`,
retains thirty initial slots: two first PASS, two first FAIL, twenty-six NOT_RUN;
six model attempts yield four qualified slots. Task 2.2 remains open at one of
three required F2 completions. Repair head `e9c5ace` passes all sixteen CI checks;
this new documentation commit still requires its own checks. Linux real generation,
other families, final-candidate applicability/review and user merge remain open.

## macOS F2 original r2/r3 complete — 2026-10-11

Both original frozen slots run through fresh ordinary Herdr Alan invocations on
the same cace913c + UI + manifest/Seatbelt repair candidate (CLI `5715ccd9`).
Actual selected and admitted model/effort are gpt-6.1-sol/medium. Each model
updates only the library and both callers, retains its non-Git inspection failure
and self-correction, and then runs library/binary verification without corrective
advice or operator source edits. Fresh projects/Processes do not imply statistical
independence: the same default Memory Store remains exposed and no personal
memory or credentials are collected.

| Original slot | Model boot / native PID | Verifier boot / native PID | Protected result / both outputs | Model seconds | End-to-end seconds | Receipt SHA-256 |
| --- | --- | --- | --- | --- | --- | --- |
| macos-f2-r2/a1 | ad6204d3-3c92-43ad-bd31-700542ecb8e4 / 49812 | deb42286-628f-4d45-b68b-91901831c5b9 / 56308 | PASS / 51, -20 | 33.284632 | 194.277512 | e3313a3ae4d0b1256ca2e35c359804494b3626d6f627f0dddf698a9b26cbea2a |
| macos-f2-r3/a1 | 41350ec1-b753-451e-b897-a784077c08b8 / 56534 | 40ff340e-f4e5-4efc-b5fd-b077170eb999 / 57058 | PASS / 3, -5 | 37.480905 | 251.475340 | 42c7ae59e90a6e32dab5d619041a28124802cf4beb2dc737595f94ad2d105420 |

Each separate verifier receives only project/assertions read-only and its own
checker scratch read-write. The original full compound checking command freshly
compiles the checker and both real binaries; source/assertion/driver hashes stay
unchanged. Actual Action/call IDs correlate to each verifier's owned rollout and
Seatbelt/projected-host-path audit. Seven observed Action Tool Processes (including setup) and the
verifier Tool exit, followed by both native `/quit`/PID-gone checks. Detail return
preserves the draft and insertion yields `abcδXYZ`. Existing Space paging handles
the multi-page r3 Action; a first-page-only collector assertion is corrected by
observing the same detail, not repeating the Tool. An r2 collector version check
initially used the observer's repository cwd; before any model launch it was
corrected to the frozen native cwd/PATH and the exact tool bytes/versions match.

Exact non-build source inventories match the frozen paths. r1 retry/r2 additionally
retain post-exit source-inventory supplements linked to their unchanged receipts;
these are later audits, not retroactive timestamps. Planned approvals/lifecycle
controls remain visible. Usage/cost and precise model/Tool/wait partitions remain
unknown; end-to-end time includes operator observation and independent checking.

Immutable `first-outcomes-v6.json`, SHA-256
`a298dabf883e4d446ef75a134b10deb94db8658760885ef688edeb4cd4cf53bd`,
retains thirty original slots: four first PASS, two first FAIL, twenty-four NOT_RUN;
eight real model attempts qualify six slots. All three macOS F2 slots now qualify,
closing task 2.2 and reaching 5/19 tasks. Earlier failed attempts remain unchanged.
F3/F4/F5 and all Linux real-model families, final-candidate applicability, formal
review/current-head CI and user merge remain required; this is not full self-bootstrap.

Linux readiness is rechecked on OrbStack ubuntu: the owning Connection metadata
file is still absent. An SSH query of the saved officelab machine is a separate
environment and supplies no Linux qualification evidence. No provider state or
private credentials have been copied between Hosts.

F2 tool-origin audits clarify the immutable receipts: their legacy
`model_tool_processes_exited` field counts all observed Action Tools, including
one planned version-setup Tool per invocation. Each run has six model-originated
Tool Processes and one setup Tool; all exit. This clarification changes no model
authorship, outcome or lifecycle proof and does not overwrite original receipts.

## macOS F3 original r1 compiler correction — 2026-10-11

Fresh original `macos-f3-r1/a1` uses the unchanged repaired native candidate and
actual gpt-6.1-sol/medium. Boot `fbaa9c96-c100-4981-9a4b-4780331d5346`, native
PID 62059, Root 8. Model Action a2 runs the frozen Cargo command and actually
fails with exit 101 / E0425, missing `BTreeSet`. Action a5 adds the import in
`src/lib.rs`; Action a6 reruns Cargo and passes the unchanged contract test.
Only that allowed source changes; no added non-build files appear.

Action a3 also reproduces the known absolute namespace Bash-operand refusal;
a4 self-corrects to relative paths, reads source/tests and retains the non-Git
status failure. These are real model self-corrections, not operator advice or a
claimed path repair. A collector initially compared lowercase Action names with
display titles; the same retained records correct that assertion before evaluating
the actual diagnostic/Edit/GREEN sequence. Public SDK activity is idle and queue
has no active/pending/deferred work after completion.

A separate normal-CLI verifier boot `204520ed-e79c-4743-9e4b-5133da7dbda6`,
native PID 63950, mounts project/assertions read-only and dedicated scratch
read-write. Fresh Cargo compilation passes protected behavior; exact project
inventory, public contract, protected assertion and driver hashes remain unchanged.
Action a1's call ID correlates to its owning rollout's Seatbelt policy audit.
Four model-originated Tool Processes, one planned version-setup Tool and the
verifier Tool exit; both native `/quit`/PID-gone checks pass. Detail Action a6
returns with logical draft insertion `abcδXYZ`.

Model time: 26.484270 seconds; submission through verified cleanup/receipt:
290.993904 seconds, within 600. No corrective advice or operator source edits;
planned controls remain recorded. Usage/cost and exact model/Tool/wait partitions
are unknown. Receipt SHA-256:
`41bc46fa1b861547a64de62b253514c67740d2039e684d68dd3449c1fb16c933`.
Immutable aggregate `first-outcomes-v7.json`, SHA-256
`f1527cca5ac0000d9f26533f21dc60b9b5253930c1ec68ee84984de753a93b66`,
retains thirty slots: five first PASS, two first FAIL, twenty-three NOT_RUN; nine
real model attempts qualify seven slots. F3 is one of three required completions,
so task 2.3 stays open and progress remains 5/19.

## macOS F3 original r2 module correction — 2026-10-11

Original `macos-f3-r2/a1` runs on the same frozen candidate with actual
gpt-6.1-sol/medium: model boot `fa76453a-914c-491c-8fa3-9cf576abcbcf`, PID
64258. Action a2 actually fails Cargo with exit 101 / E0583 (missing module
`operation`). The model reads the existing `operations.rs`, updates only the
module declaration/reference in `src/lib.rs` (a8), and reruns Cargo GREEN (a9).
Existing module content, tests, Cargo files and exact non-build inventory remain
unchanged. No source is operator-authored and no corrective advice is supplied.

Fresh independent normal-CLI verifier boot `c93f7379-155d-46b5-8827-0a8c6a874047`,
PID 65523, uses read-only project/assertions and dedicated writable scratch.
Fresh compilation/protected behavior passes, source/assertion/driver bytes remain
unchanged, and actual Action/call ID correlates to its Seatbelt policy audit.
Five model-originated Tool Processes, one planned setup Tool and the checker Tool
exit. Both native `/quit`/PID-gone checks pass; Action a9 detail returns to draft
`abcδXYZ` at the preserved insertion position. Model time 35.000430 seconds;
submission through verified cleanup/receipt 262.193911 seconds, within 600.
Usage/cost and precise model/Tool/wait partitions remain unknown.
Receipt SHA-256 `a6e6c0511e18296a200854317caecd43dd6664a35ec8a7cec81ced5a1e672f42`.
Immutable `first-outcomes-v8.json` SHA-256
`fb9f1c769329391093fcfb7fce3a1c9523a74bdb8b11762a26c69452833e71db`
retains six first PASS, two first FAIL, twenty-two NOT_RUN; ten model attempts
qualify eight slots. F3 is two of three completions; task 2.3 remains open.

## macOS F3 original r3 and family closure — 2026-10-11

Original `macos-f3-r3/a1` uses the unchanged candidate and actual
gpt-6.1-sol/medium. Model boot `dd4de43b-fae7-4aab-860f-ee7fac8b4c92`, PID
65748. Action a2 actually fails Cargo with exit 101 / E0308: `parse::<i32>()`
returns a Result instead of i32. Action a4 adds `unwrap_or(0)` only in the allowed
`src/lib.rs`; a5 reruns the unchanged contract GREEN, preserving successful
negative parsing and invalid-input zero behavior. No added non-build files,
operator source edits or corrective model advice.

Fresh verifier boot `1006ddf1-b879-4e30-a317-954dd26159a4`, PID 66203, receives
project/assertions read-only and scratch read-write. Fresh dependency/checker
compilation passes protected behavior; exact source/assertion/driver hashes stay
unchanged. The Action/call ID matches its owning rollout's Seatbelt audit.
Four model Tool Processes, one planned setup Tool and the verifier Tool exit;
both native instances exit normally and detail Action a5 preserves `abcδXYZ`.
Model time 28.682936 seconds; end-to-end 401.419416 seconds, within 600. Usage,
cost and exact model/Tool/wait portions remain unknown. Receipt SHA-256:
`0ef06fc97996cf963ca23952b167a73a31450dcb4ed021b914095a4b6bfea2d4`.

Immutable aggregate `first-outcomes-v9.json`, SHA-256
`79c2892f84eb61a127942a574bd6a4ab8be5e0080b8d61bafb7b23b396379b2b`,
retains thirty slots: seven first PASS, two first FAIL, twenty-one NOT_RUN; eleven
model executions qualify nine slots. All three macOS F3 repetitions now qualify,
closing task 2.3 and reaching 6/19 tasks. F4/F5, all Linux generation families,
final-candidate applicability, review/merge and the absolute namespace Bash
operand finding remain open. Published F2 evidence head `ebbeaffb` passed all
sixteen CI checks; this newer evidence commit requires its own CI.


## macOS F4 original r1 retained failure — 2026-10-11

Original `macos-f4-r1/a1` uses the frozen repaired CLI `5715ccd9` with actual
`gpt-6.1-sol/medium`. Initial boot `0a7b67f1-8ff7-4fb7-8080-e2d559657867`,
native PID 71326, Root 8, rollout `ff33f6fe-3c44-4057-a9be-a009c67ba789`.
The model invokes `start_work` once: the ledger records one completed effect,
and an actual delayed writer (PID 72760) is observed live before planned Ctrl+C.
The writer exits; after its deadline there is no late output and the ledger
still has one entry. Absolute namespace Bash operands reproduce the separate
known refusal; the model self-corrects without operator advice.

At the settled pause, `/project revoke` falsely reports no active grant: its
local picker receipt is absent, but public Host metadata confirms request-1 is
active at the current Root's selected project. An unplanned, recorded native CLI
revocation fallback makes that grant inactive. The frozen denied-read follow-up
returns no content and explicitly refuses revoked cwd authority. Planned queue
continuation dispatches only that follow-up, not the interrupted task again.

The operator explicitly verifies selection of the original owning rollout, exits
the first invocation and launches `--resume` in a fresh runtime. Boot
`5e146bdd-a0a5-4497-8a76-834cb71083e8`, native PID 73195, Root 8, rollout
`8d0f34ca-c99d-44ba-a9a3-5fbfc41286ec`, retains the original frozen task.
After explicit regrant and planned model confirmation, EditFile repairs only
`src/lib.rs`; model Cargo contract passes. Retained resumed Actions never invoke
`start_work`; the ledger remains one and no late write appears. All observed
Tools, the child writer and both native invocations exit.

This is FAIL, not a qualified F4 completion: the revoke UI requires unplanned
fallback and submission through observation/cleanup takes 697.506942 seconds,
exceeding the frozen 600-second bound. Operator observation is included; this is
not a model latency measurement. Independent protected verification is NOT_RUN
and detail/draft/cursor is NOT_CHECKED after qualification failure. There are no
operator source edits or corrective model advice; planned controls and the
unplanned revocation fallback remain separately recorded. Cost/usage and exact
model/Tool/wait partitions are unknown. Receipt SHA-256
`a7d875eae7d8c7df286182e4d18d9a7ff25292cea52e13df4bcc69baa7f6fc5f`.
Immutable `first-outcomes-v10.json` SHA-256
`c86a2e4a93470d9cabd4d4ea7fd41d08dbf11091e76c98d1b6b164d1811da3d0`
retains thirty slots: seven first PASS, three first FAIL, twenty NOT_RUN; twelve
real-model task attempts qualify nine slots. Progress stays 6/19; task 2.4 is open.
Evidence head `5583e2c0` passed all sixteen current-head CI checks before this
new finding/fix; those checks do not qualify the subsequent production diff.


## External project revocation repair and native probe — 2026-10-11

The TUI now issues a bounded asynchronous read of existing Host Mount grant and
request files when `/project revoke` has no retained local receipt. It matches
active authority, numeric Root requester and observed cwd by path components,
chooses the longest unique matching prefix and refuses unavailable, malformed,
ambiguous or stale context. Existing picker/recovery behavior is preserved. No
new grant, completion root, controller or inferred cwd access is introduced.
The existing settled-input predicate is reused at dispatch and reply settlement.
Actual directory control still checks Root before/after its write; only its
correlated successful Action arms Host revocation. Lost Host acknowledgments
retain the existing explicit-retry authority rather than claiming success.

The command regression failed before the fix. Eight new tests (command plus
seven grant tests) cover actual bounded file reads, longest/ambiguous/component
matches, inactive/other-Root grants, malformed/oversized/unavailable metadata,
Root/cwd/admission changes, confirmed leave-before-revoke, lost acknowledgment,
background correlation, retained draft/quit and lookup timeout. Full TUI passes
378 unit and 12 integration tests; warnings-denied targeted Clippy passes.
Full `just quality` including standalone distribution passes; pinned OpenSpec
1.4.1 strict validation passes all 69 current items. No new dependency/config or
public ProjectControl variant is needed. Direct review covers private Action/
Event dispatch, current-Root fencing, public Host schema/visibility, queue
boundaries, existing pending authority and real cleanup paths.

A separate detached cace913c + exact previous fixes + TUI repair checkout preserves
earlier native binaries. Fresh CLI SHA-256
`6e99146065cc0d2e8976ebce19e22055c362e4d624265964db389ff31a4e987e`,
source diff SHA-256
`107e10c65b88bc23bfd9591a6f95d166abd33fe596521816d1991b58a094561d`.
Probe `project-revoke-native-v1` prospectively freezes source/file/CLI/config/
observer/tool identity and uses ordinary native Alan in the owned 80x24 Herdr
pane. Boot `62773bf1-2d23-4a5b-9d4c-a2db6fa3097a`, native PID 88671,
Root 8. SDK grants request-1, ordinary cd selects its namespace cwd without a
picker receipt, and `/project revoke` produces actual successful cwd Action a1
(call `78a8f7d5-249d-439a-8794-f607a2c7afe4`) selecting `/` followed by
public Host `active:false` and truthful UI acknowledgment, without a fallback.

A second SDK grant remains active but unselected at cwd `/`. Repeating the
command truthfully reports no matching active grant and leaves that grant active,
proving the UI does not revoke arbitrary visible authority. Planned native CLI
cleanup then revokes only that probe grant. Both canaries stay unchanged; `/quit`
ends PID 88671 and foreground returns to the same owned fish PID 72808.
Receipt SHA-256 `00bf43484a9df328306f47972a23f9cf2d19a399de1bdd6324a75d17f2ae1027`.
This no-generation repair probe does not fill a matrix slot or replace F4's
original FAIL. A fresh linked real-model F4 retry and remaining repetitions,
final-candidate applicability, new-head CI/review and user merge remain required.


## macOS F4 r1 linked a2 — namespace shell failure retained

Fresh a2 prospectively freezes the repaired CLI `6e991460`, source diff
`107e10c6`, original prompt/follow-up hashes, identical original fixture bytes,
selected tools/model/backend and fresh runtime/project/checker paths. Boot
`72ad2103-b87f-4b85-a49b-ca3ad9b8666d`, PID 95495, Root 8, rollout
`f0d49628-b70f-45a1-a0c6-3bc2fb098619`; actual gpt-6.1-sol/medium.
The collector reobserves the same buffered setup rollout before generation and
corrects its backend enum expectation to the actual `seatbelt`; no model input
is repeated. Setup Tool versions and enforcing policy audit are frozen before
submitting the unchanged model task once.

Model Action a2 attempts namespace-absolute `ls`/`sed` inspection and receives
`Command references path outside host_mount: /mnt/project-request-1`. Unlike
prior self-correcting runs, the model then calls request_mount for the same
already-granted namespace. Host metadata shows original request-1 approved and
active, while new request-2 waits for approval. No duplicate grant is approved;
no corrective prompt or source edit is provided. The lifecycle test/writer never
starts, the ledger stays empty and the exact original source inventory is intact.

This attempt is FAIL before F4's required live-writer boundary. Recorded early
Ctrl+C safely cancels the pending duplicate request; the actual queue settles
paused with no active/pending work. `/project revoke` now succeeds from the
settled pause, with successful correlated cwd Action a3 selecting `/`, original
grant `active:false` and truthful terminal acknowledgment. `/quit` ends PID
95495 and foreground returns to owned fish PID 72808. No late effect appears.
Protected verification, explicit recovery and detail/draft/cursor are NOT_RUN or
NOT_CHECKED; no full lifecycle pass is inferred from cleanup. Submission through
verified failure cleanup takes 257.709124 seconds. Cost/usage remains unknown.

Receipt SHA-256 `2730d877697be9a081206e2f318624536a29db2fab1bce89fb44cbd60569d4bb`.
Immutable aggregate `first-outcomes-v11.json`, SHA-256
`06df3af55f5166c0d25543147026fa2170dd0ab817d3d813e57e45ff64712364`,
retains seven first PASS, three first FAIL and twenty NOT_RUN; thirteen real-model
task attempts qualify nine slots. The original a1 remains unchanged. The UI
revocation fix has positive and negative native proof; the namespace-absolute
Bash finding now prevents this real model's F4 task instead of being corrected
by the model. Resolve the path/authority contract before another linked retry;
do not approve redundant authority or replace the frozen task with an easier one.


## Bash guidance repair and macOS F4 r1 linked a3 PASS — 2026-10-11

The existing Bash description and command schema now distinguish supported
cwd-relative shell operands from file Tools' absolute Alan namespace paths.
ToolRegistry exposes those same definitions to the model; execution, policy,
Host authority and shell command text remain unchanged. This does not implement
arbitrary namespace-absolute shell mapping. Existing Tool tests pass 138/0/0;
strict OpenSpec passes 69/69. Normal commit hooks run full quality and standalone
distribution successfully. UI repair head beff562e passed all sixteen checks;
guidance/evidence head 8116a8d5 has its own required review/CI.

Fresh a3 keeps the original task/follow-up hashes and exact original project
baseline, with fresh project/checker paths. Native candidate retains the Linux
predecessor, previous exact repairs and new Bash guidance; prospective freeze
includes all source files/diff, CLI/config/observer, PATH, resolved/selected tool
executables and SHA-256, native Rust/Cargo 1.93.0 and Git 2.54.0 identities.
CLI SHA-256 `52325439c5958f531464969dcfa0979cb48eb4b56f2cccd28e730bc33b8cd4d1`,
source diff `4810c786729b77b8ab18bc6abce6427d2374c73bee4a0a319962179e16b0c634`.
Native originals and earlier-source receipts remain preserved.

Initial boot `edc569fb-f1bc-40f6-ac3d-49da9210c63c`, PID 16072, Root 8,
rollout `671bca2f-7af7-4cc6-b140-cc6103ba2327`. The actual real model uses relative
shell operands, self-corrects a non-Git inspection failure and invokes start_work
once without requesting duplicate authority. Live writer PID 16471, parent 16468,
process group 16426 are observed; planned Ctrl+C precedes its forty-second
scheduled write. The writer, parent and group exit; beyond the deadline there is
no late effect and exactly one completed ledger entry. The settled paused queue
has no active/pending work before `/project revoke`. Correlated successful cwd
Action a4 selects `/`, Host request-1 becomes inactive, and UI acknowledges
revocation without a CLI fallback. The unchanged denied-read follow-up is admitted
paused and explicitly continued alone: Read Action a5 returns no project content
and a truthful authority/adapter denial; no new grant or work is requested.

The operator verifies that metadata selects this exact owning rollout, records
its hash, exits PID 16072 and explicitly starts --resume. Boot
`b63581f0-5d5e-4fa6-a490-6279148e699e`, PID 16892, Root 8, new rollout
`6304d995-f7c7-47f4-80bb-7de0a4b35449` retains the original frozen task. Before
regrant there is no admitted/active work. Explicit regrant, actual cwd Action a6
and the unchanged continuation prompt are followed by a real structured request
r0 for reapproval; the planned operator confirmation is sent once. Read a7,
EditFile a8 and Cargo a9 update only src/lib.rs and pass the immutable contract.
The new rollout's actual Bash executions contain only that contract command;
start_work never reruns, lifecycle files stay intact and the ledger remains one.

Resumed AgentFS retains older Action IDs. The collector records an origin audit
and correlates only new a6-a9 to resumed execution, avoiding reused numerical
Process references as origin proof. Two Process references retained by new
Actions (10 and 11) are observed exited; the initial observed 9/10/11 statuses
and native cancellation-group exits are separately retained. Read result evidence
without a retained Process reference is not assigned an invented PID. Detail a9
shows its original Cargo result and returns to logical draft abcδXYZ; clearing
the draft precedes native exit. Both model invocations exit.

Separate ordinary native verifier boot `489985b9-36b9-4953-b0f9-ae4da1893e6c`,
PID 17466, Root 8, mounts project/assertions read-only and dedicated scratch
read-write. Fresh Cargo dependency/checker compilation passes protected_behavior;
its actual call ID correlates to Seatbelt policy audit. All project/protected/
driver bytes remain unchanged by checking, the referenced Tool exits and /quit
ends the verifier, returning foreground to owned fish PID 72808. Actual model
contexts remain gpt-6.1-sol/medium. Usage/cost and exact model/Tool/wait partitions
are unknown; the same default Memory Store remains exposed, not independent
statistical sampling. No operator source edits, corrective model advice,
unplanned regrant or revoke fallback occurs; planned controls remain counted.

Submission through verified cleanup/receipt: 528.041471 seconds, within the
frozen 600-second bound. Receipt SHA-256
`bd74087709fcc720fe77a987451bb8ef10dd4d60b9cc718f1719aad6524125ba`.
Immutable aggregate first-outcomes-v12.json SHA-256
`de7d799d38b5aa209461ad4bb859eea794bdfbbbe02c69346f15bf60949b2670`
retains seven first PASS, three first FAIL, twenty NOT_RUN; fourteen real-model
task attempts qualify ten slots. Original a1 and a2 remain immutable. F4 now has
one of three qualified repetitions; task 2.4 stays open and progress remains 6/19.
This supports this bounded corrected task, not general namespace-absolute shell
support, general autonomous development or final-candidate applicability for
older runs. F4 r2/r3, F5, all Linux generation, final review/CI/user merge remain.


## macOS F4 r2/r3 complete — 2026-10-11

Both remaining original F4 inputs pass on the same frozen guidance/repair native
candidate as r1 a3: CLI `52325439`, source diff `4810c786`. Each prospectively
checks exact source files, CLI/config/observer identities, selected/resolved
Rust/Cargo/Git and PATH, original fixture/assertion/driver inventories, prompt
and follow-up hashes. Actual setup Tool output and call-correlated Seatbelt
policy audit establish readiness before generation. No candidate source change,
model substitution, new authority contract or installed-runtime deployment occurs.

| Slot / attempt | First and attempt outcome | Submission through verified cleanup | Receipt SHA-256 |
| --- | --- | --- | --- |
| macos-f4-r2 / a1 | PASS | 251.302363 s | `4abcf29edccc22811cbc5128a58623660c2c1c2af06f8ee2be2a1f72dc5c9929` |
| macos-f4-r3 / a1 | PASS | 213.503111 s | `04dcb923c75cf79fe864aa80b5e9bf2fd89b07d55b3dac0a9166236aed0adc3e` |

Both actual models use supported relative shell inspection and invoke start_work
once, without duplicate mount requests or operator correction. r2 self-corrects
an initial non-Git inspection failure. Live writers, parents and process groups
are observed before planned Ctrl+C, then observed gone. Beyond each original
forty-second deadline, late.txt is absent and the completed ledger entry occurs
exactly once. The source still has its original defect before explicit recovery.
At a settled paused boundary, /project revoke succeeds through its actual cwd
Action and Host active:false acknowledgment. The original denied-read follow-up
is queued paused and continued alone; its real Read result returns an authority/
adapter denial without file contents, new grant or further work.

r2: initial boot `beb77926-5621-4a50-869b-14a3b07b2a93`, native PID 26084; writer/parent/group 26247/26246/26204. Explicit resumed boot `1ed52ff2-b82a-41fd-922d-ebc9414a04fe`, PID 26363; new Actions a6, a7, a8, a9, with observed exited Tool references 10, 11. Independent verifier boot `08b3d8f5-8baf-4bb1-a1ad-751a0bf8ac8b`, PID 26560. Every Root is instance-local Process 8.

r3: initial boot `61dabf9d-b47a-4e3a-9572-7aac97954558`, native PID 26805; writer/parent/group 26902/26901/26859. Explicit resumed boot `fe9c89f0-d96f-4492-9761-2b0358186fb8`, PID 27001; new Actions a5, a6, a7, a8, with observed exited Tool references 10, 11. Independent verifier boot `0010b771-fd41-4fb5-aca5-27c5bc2920c9`, PID 27192. Every Root is instance-local Process 8.

The selected durable rollout is checked against the exact interrupted attempt
before native exit and --resume. A fresh boot retains the original task without
active/admitted work before regrant. Explicit regrant and a newly correlated cd
Action precede the unchanged continuation prompt. Actual structured request r0
asks for reapproval; its planned confirmation is sent once. Read/EditFile/Cargo
then change only src/lib.rs to x + y and run the immutable contract successfully.
The resumed rollout's Bash commands contain only that contract command, with no
start_work replay. All initial/resumed model contexts confirm gpt-6.1-sol/medium.
Retained older Actions remain distinct from the newly observed Action IDs and
call IDs; no old numerical Process reference supplies resumed-origin proof.

Original Cargo detail correlates to a9 for r2 and a8 for r3. Both return to the
same logical draft abcδXYZ after insertion in its middle. Separate native
verifiers receive only read-only project/protected assertions and writable
checker scratch; fresh dependency/checker compilation passes protected_behavior.
Actual calls match Seatbelt allow audits, all project/assertion/driver bytes
remain unchanged by checking, and referenced Tool statuses are exited. Initial,
resumed and verifier native PIDs and cancellation descendants/group are gone;
foreground returns to owned fish PID 72808. Neither run uses operator source
edits, corrective model advice or an unplanned revoke fallback. Planned setup,
authorization, cancellation, queue/recovery and verifier controls remain counted.
Usage/cost and model/Tool/wait partitions remain unknown; the same default Memory
Store stays exposed, without claiming independent statistical samples.

r2's prelaunch collector initially dereferenced the rustup shim pathname before
comparing it to the prior literal invoked pathname. Actual selected binary hashes
and versions were unchanged; the corrected collector preserves both identities
before any native launch or generation. r3 retains its raw-text Action-ID artifact
and a separate valid JSON origin audit rather than overwriting evidence. These
operator collection corrections do not steer the model or change fixture inputs.

Immutable first-outcomes-v13.json SHA-256 `4b3599821506f4f012fbfa584142ac8190a6af87b30a7abbb3fade7acc6daaf2` records 15 model task attempts and 11 qualified slots; first outcomes 8 PASS, 3 FAIL, 19 NOT_RUN.

Immutable first-outcomes-v14.json SHA-256 `f2a164ca7e942a2679dd523b9192f3819e3d870c309f4167d0c5a11576465d83` records 16 model task attempts and 12 qualified slots; first outcomes 9 PASS, 3 FAIL, 18 NOT_RUN.

All three macOS F4 repetitions now qualify, closing task 2.4 and bringing this
change to 7/19 completed tasks. Original r1 a1/a2 and earlier aggregates remain
immutable. Evidence head ed3f7281 passed all sixteen current-head CI checks;
this new evidence commit needs its own review/CI. F5, all Linux generation,
final-candidate applicability for older runs and user merge/canonical closure
remain open. This bounded result does not qualify general namespace-absolute
shell support, unattended self-development or automatic routing.


## macOS F5 r1 first FAIL — retained-reference access and command readability

The original frozen stdout task uses the same CLI 52325439/source diff 4810c786,
with prospective source/file/config/observer/PATH/selected-tool identity, original
fixture/assertion/driver bytes and actual gpt-6.1-sol/medium/Seatbelt readiness.
Model boot `e4aa64ae-e108-4b30-9c64-51a0e26e901d`, PID 32541, Root 8. Action a2 runs the exact original command and retains 168363 bytes of original JSON, including QVALUE=42137 beyond the bounded preview.

The model follows the runtime-provided /agent/8/actions/a2/output reference
with ReadFile Action a3. NativeToolExecutionAdapter rejects it as outside delegated
Host Mounts. The model truthfully explains that it could not inspect the original
and reruns the producing test through rg QVALUE in a4. It then edits only src/lib.rs,
verifies Cargo and cites the separate original/read-denial/rerun Actions. The
expected value and exact production diff pass independent checks, but rerunning
the producer does not fulfill the frozen retained-original dependency. This
original attempt is FAIL and adds no qualified slot; no operator correction or
source edit was supplied.

Ctrl+O selects original a2. The bounded Command presentation shows initial stdout
and a truncation cue; beyond it, the original JSON is present but rendered with
escaped newline characters. Supported Space/b paging reaches QVALUE and preserves
literal server> ready and a > b, correlating the full original to a2. Detail return
and insertion restore abcδXYZ. Host scrollback retains the input, actions and final
explanation. An unsupported Herdr Home key was rejected before any input; the
retained failed collector intent is followed by supported b/Space navigation.
This is an observed readability defect, not evidence loss or permission to strip
literal content.

Independent verifier boot `ef151743-4e46-4744-8eff-1f981d5fd6e9`, PID 34422, Root 8, uses read-only project/assertions and writable scratch. Fresh dependency/checker compilation passes protected_behavior; its call ID matches Seatbelt allow audit. All project/assertion/driver bytes stay unchanged by checking; actual Tool status records are exited, both native PIDs are gone and foreground returns to fish 72808.

Submission through verified cleanup takes 351.374980 seconds, below the frozen 600-second limit. Receipt SHA-256 `c614c2284c1ec70f621f9babe5247083e17c30721a781414038f7bcc4d08b51b`. Immutable first-outcomes-v15.json SHA-256 `dd6cf36b3d76b8f77e506734d5e4cc1c1eb821e3dbe8cec122bfebe30ee1c96d` retains nine first PASS, four first FAIL and seventeen NOT_RUN: seventeen model task attempts, twelve qualified slots. Usage/cost and time partitions remain unknown.

The reader gap belongs to ordinary namespace evidence access and the existing
Tool Process/AgentFS boundaries, not Host Mount authorization. Do not add a native
store path, broaden Host grants, replace the producing command or claim the
published reference is usable merely because an operator SDK can read it. The UI
repair belongs to existing generic presentation of acquired retained Command
stdout/stderr, preserving raw evidence and all current display bounds. Both
findings need implementation, applicable tests/native acceptance and a fresh
unchanged-task retry before F5 can qualify. F4 remains 3/3 and checklist 7/19;
Linux generation and final-candidate/delivery gates remain open.

## Native retained Command readability repair — no model task

The shared acquired-content presenter now recognizes complete generic Command
stdout/stderr/exit-code JSON and displays the acquired streams as readable lines,
with separate labels and exit status. Original raw bytes remain separately
available. It does not inspect Tool identity or arguments, substitute bounded
metadata for original output, change the display bound or modify reference,
expiry/redaction or malformed-shape handling. The change is nineteen production
lines in the existing presenter, with three adjacent tests. Two actual Action
detail regressions fail on the old source before the repair. Complete TUI testing
passes 381 unit and 12 integration tests; warnings-denied Clippy passes and pinned
strict OpenSpec validates 69/69 surfaces.

A fresh detached candidate outside /tmp uses cace913c plus the exact earlier
qualification repairs and this presenter/test slice. CLI SHA-256
`c2eca9e549811d66a438e1b25ad69cc40757122c34dc7471c13699d2ec02236b`,
source diff SHA-256
`a2e471119ef5493a2eb3950ffa554c791127a5dc6b868efe509396461c452640`.
Native PID 41920, boot `03169084-2e8a-4af2-a6c2-05f152d7247e`, Root 8,
actual Herdr terminal 80x24. Public SDK grants two separately prepared projects;
ordinary explicit cd and Cargo commands produce independent 3000-line stdout
Action a1 and stderr Action a3. Actual retained outputs contain QVALUE=42137 and
QVALUE=42274 at stream line 2807. Ctrl+O shows the actual retained streams, and
supported Space paging reaches both values as standalone readable lines. Literal
server> ready and a > b remain content. Closing each detail and inserting at the
saved draft position produces abcδXYZ; Host scrollback retains both command/result
summaries. Original project inventories remain byte-identical, both Tool status
records are exited, each result call ID matches Seatbelt allow audit, and /quit
ends the native instance and returns foreground to fish 72808.

Receipt SHA-256
`4b4d2e9237bed8b1d2565296d150b7f240502a696811a71211914f2d91fa9e78`
at the owned qualification cache `command-detail-native-v1/receipt.json` retains
source/binary identity, original output hashes, captions, draft/cursor, audit,
unchanged inventories and lifecycle evidence. Collector CLI access spelling and
an unscoped observer /proc listing were refused before their requested operations;
the retained corrected intents use documented access spelling and actual numeric
Tool paths. The rollout contains only four explicit cd/Bash Tool calls and no
generation. This is a native UI repair probe, not a model-authored development
attempt: aggregate v15, twelve qualified slots and checklist 7/19 are unchanged.
The model's retained-reference reader remains unimplemented; this presentation
repair does not qualify F5 or repair its first failure. Fresh head review/CI and
the unchanged-task real-model retry remain required.


## macOS F5 r1 a2 FAIL — mixed range arguments and invisible guard stop

The fresh independently built reader candidate uses cace913c plus the earlier
qualification repairs and the actual Process-scoped namespace reader. CLI
SHA-256 `5629d84bf0218606ea978efabfac17ec807e7a2d37fb789ed31ad00b1e84ac17`,
source diff SHA-256 `ac0c9d9068b68bf06652f309c6e7c76d96a81f22b188d2cb7d646ad20aa61639`.
All five fixture files, the original prompt and protected assertions match the
frozen r1 input. gpt-6.1-sol/medium, actual Seatbelt, selected tools/config,
80x24 native terminal and source/binary identities are frozen before generation.
Native PID 74988, boot `2800371a-c96e-4a94-8047-a4465af11d49`, Root 8.

The original producer executes exactly once in a2, retaining the full stdout.
Four actual ReadFile Actions a3–a6 request that same /agent/8/actions/a2/output
with byte_offset=164300, byte_limit=4096 AND offset=1, limit=100. The original
mixed-mode validation correctly rejects all four; no additional authority,
producer rerun, operator advice or solution edit occurs. The fifth identical
invocation is stopped before Tool execution by the existing repeated-call guard.
The task settles to idle with an empty known queue and no visible stop notice.
Code inspection confirms both loop-guard branches publish only legacy events,
not AgentFS terminal error surfaces. Protected behavior checking is NOT_RUN:
the retained-original dependency already failed and no solution was authored.

The original Action detail remains readable, reaches QVALUE=42137 through
supported paging and returns to abcδXYZ with the saved logical cursor. Source
files remain byte-identical; actual observed Tools are exited, PID 74988 is gone,
and foreground returns to fish 72808. Submission through verified cleanup takes
378.941501 seconds, within the frozen 600-second bound. Receipt SHA-256
`98ad52944ccac9fb9b726b396d6c61e78416fb5c56a2ad7e70b15b351ee0e659`
at attempts/macos-f5-r1/a2/receipt.json. The retained collection notes distinguish
a pre-generation toolchain-cwd mismatch and asynchronous detail selection from
model steering; corrected collection changes no prompt, solution or authority.

Immutable first-outcomes-v16.json SHA-256
`2a7dc09dbf6f2c1a5223d661dc4037a9999adc614684348efd6cb17001a300d2`
records eighteen model-task attempts, twelve qualified slots, and original first
outcomes nine PASS, four FAIL and seventeen NOT_RUN. Usage/cost and time partitions
remain unknown. The shared Responses adapter's omitted strict field is a plausible
cause of required-looking optional arguments, not a captured server trace. The
local explicit non-strict wire repair passes serialized streaming/non-streaming
regressions and 212 LLM unit plus eight integration tests; existing provider-live
and doc checks remain ignored. Both provider and stop-visibility repairs require
fresh unchanged-task native qualification, new-head review/quality/CI and final
candidate applicability. Checklist stays 7/19; no F5 slot is qualified.


## Reader / provider / guard repair local verification

The existing AgentFS error_notice helper now publishes both loop-guard stop
branches as a current notice and retained terminal Error event. Thresholds,
legacy events, Process/Action effects and turn settlement stay unchanged; no
synthetic model answer or second history owner is introduced. Two production
calls and corrected unsupported-environment advice implement the visibility fix.
The actual AgentFS-backed orchestration regression first fails with notice None,
then passes both repeat and batch limits, verifies no refused Action, original
Tool exit, idle settlement and exactly one retained error after the next input.

Complete affected-crate testing passes Engine 1407 plus 20 integration tests
(one existing ignored unit), Tools 144, Service Manager 158 plus two integrations,
and LLM 212 plus eight integrations. Existing live-provider/doc cases stay ignored.
Normal full quality, standalone checks and distribution pass; pinned OpenSpec
1.4.1 validates 69/69. The initial new delta failed the legacy validator because
SHALL was not on its first requirement line; the corrected normative sentence
passes without changing implementation. Review traces all namespace-bearing
callers, owner/range/expiry/UTF-8 boundaries, shared official/managed wire mapper
and both guard surfaces. No new external runtime dependency or authority/config
is introduced. Publication/current-head CI and unchanged-task native evidence
remain separate gates; local verification adds no qualified matrix slot.


## macOS F5 r1 a3 PASS — original retained result acquired without rerun

Fresh detached cace913c plus the exact qualification, reader, Responses optionality
and loop-guard publication repairs uses CLI SHA-256
`1b609034eea7c8c2a7f7f5a1725d14fb6ed5165ec4776a196bd7237162a2495e`,
source diff SHA-256
`60fbb03ab93f2d1a6d94cc924a064fd12dae38052cd25c9615608e3da8085608`,
and a 35-file prospective source inventory. Both this candidate and the delivery
checkout pass normal full quality/standalone/distribution. Original fixture,
prompt, assertions, config, PATH/selected tools and 80x24 geometry are unchanged.
Model PID 13419, boot `4f8ac0f1-d150-4fc2-8f46-1c2383ae0295`, Root 8.
Own rollout Meta and every actual turn_context confirm gpt-6.1-sol/medium.

Original producer a2 executes exactly once. ReadFile a3 and a5 read its concrete
AgentFS output with only path/byte_offset/byte_limit; no line fields, extra Host
grant or producer rerun. a5's 4096-byte original range from offset 156900 contains
QVALUE=42137; the model cites original a2 and read a5, changes only src/lib.rs,
runs ordinary Cargo tests and corrects its own relative-include stdin verifier
error from a7 with successful a8 compilation/assertion. The original compiler
failure remains in visible history; it is not operator-corrected or erased.

Actual original detail a2 exposes the standalone QVALUE at line 2807 through
supported Space paging; server> ready and a > b stay literal. Separately delivered
Escape returns to abcXYZ and subsequent unicode insertion produces abcδXYZ at the
saved logical cursor. Host scrollback retains task, original/read/edit Actions,
compiler error/correction and final answer. The collector notes retain early
not-yet-published observations, virtual cd's still-running Root mistaken for a
Tool, expected Cargo compilation stderr, combined Escape/unicode ambiguity and
Ctrl-U prefix-only clearing. Corrections affect collection/terminal gestures only;
there is no new model prompt, operator solution edit or corrective advice.

After model exit, independent verifier PID 21470, boot
`c8baf160-e738-4d5c-9190-6c6a80582340`, receives the actual project and protected
assertion tree read-only, fresh checker scratch read-write. Ordinary Cargo test
compiles the real dependency and passes protected_behavior. Its exact Action call
ID correlates to the owning rollout's Seatbelt allow audit. All source/assertion/
driver inventories stay unchanged by checking. Actual observed Tool Processes are
exited, both native PIDs are gone and foreground is fish 72808.

Submission through verified final cleanup takes 389.100920 seconds (bound 600).
Receipt SHA-256 `ffa059564404cac3902df9c09616dd90bd90db4b61ae5064d7c06e4a1ffe3ad8`
at attempts/macos-f5-r1/a3/receipt.json. Immutable first-outcomes-v17.json SHA-256
`64e8ed842109b41f1a6b3f716e458eee5828a90c4d2215e57ebc371483d5b86b`
records nineteen model-task attempts and thirteen qualified slots; first outcomes
remain nine PASS, four FAIL and seventeen NOT_RUN. Earlier F5 a1/a2 and aggregates
remain immutable. Usage/cost and model/tool/wait partitions remain unknown.

This native success qualifies the corrected stdout task and actual reader/provider
integration, not a direct server-normalization trace or unattended self-development.
It does not independently force a native loop-guard stop; both guard branches have
actual AgentFS RED/GREEN regression evidence and reuse the existing terminal-error
renderer. F5 stderr/diff repetitions, all Linux generation, final-candidate
applicability for older tasks, review/current-head CI and user delivery remain.
Checklist remains 7/19; automatic routing stays disabled.

## macOS F5 r2 PASS — original long stderr

The unchanged 35-file reader/provider/guard candidate, CLI SHA-256
`1b609034eea7c8c2a7f7f5a1725d14fb6ed5165ec4776a196bd7237162a2495e`,
executes the original frozen stderr task with gpt-6.1-sol/medium, 80x24 geometry
and the same config/tool identities. Model PID 29675, boot
`fc4b26e0-f30a-4ad8-9428-0176b4d7c7c5`, Root 8. Original producer a2 executes
once; QVALUE=42274 is in stderr, not stdout. ReadFile a3 reads the original a2
output at byte offset 156900 with byte limit 4096 and obtains the value. Only
src/lib.rs changes. The model's a7 stdin include verification fails, then its own
a8 correction passes; no operator solution or corrective advice is supplied.

Original a2 detail exposes line 2807 after 282 Space page steps, preserves literal
server> ready and a > b, and returns to abcXYZ. Separate Unicode insertion at the
saved cursor produces abcδXYZ; 160-line host scrollback is retained. The collector
records acquisition/paging lag and a short capture that omitted the header.
Actual real Tool Processes 9–16 are exited; virtual cd's Root 8 is excluded.

After actual model exit, independent verifier PID 30806, boot
`16da11fd-479f-4fed-9942-2396fc7eef8c`, receives project/protected roots read-only
and checker scratch read-write. Explicit Cargo compilation passes protected_behavior
with its actual exited Tool and owning Seatbelt audit. Exact source paths, all
other baseline files, assertions and checker driver remain unchanged. Both native
PIDs are gone and fish 72808 is foreground. Elapsed time is 324.910807 seconds.
Receipt attempts/macos-f5-r2/a1/receipt.json SHA-256
`e151420044a076f3ff31eeaac6d19e30916424efc759860fca8b1d0a1b200b2d`.
Immutable first-outcomes-v18.json SHA-256
`1e66a4d536b935dcf39bb0a5c775f550d5c6890a28b77150645e79f5dfa3d016`
records twenty development attempts, fourteen qualified slots and first outcomes
ten PASS/four FAIL/sixteen NOT_RUN. Usage/cost and time partitions stay unknown.

## macOS F5 r3 PASS — original long diff, with collector intervention disclosed

The same unchanged candidate executes the original frozen long-diff task. Model
PID 31284, boot `94281dbd-a1e7-4ebd-806b-419f856700f7`, Root 8, and its own Meta
and turn context confirm gpt-6.1-sol/medium. Initial Git HEAD is
`300cd5f1468b15d6cec170c52e3253259c035371`; the 138113-byte setup diff SHA-256 is
`0957f9f97b77fa0c87ad8a0f6cadc1cdf5617b0fa58ba03b7f633b0522c65a1a`.
Both are frozen before generation and unchanged after development and checking.

Original a2 runs git diff -- data/long.txt exactly once. ReadFile a3 reads its
last range without the key. Auxiliary a4 also rg-reads QVALUE from the already
granted project, alongside source/config/status inspection. This lookup is retained
and is not represented as original-result acquisition. Before editing, a5 reads
4096 original a2 bytes from offset 135000 and obtains +QVALUE=42411. The final
answer cites a2/a5. Only src/lib.rs changes; a7 Cargo succeeds with zero project
tests, a8's namespace include fails, and a9's model-authored assertion harness
succeeds. Independent protected behavior, not zero-test Cargo exit, proves correctness.

Native original a2 detail retains both removed and added diff lines. 582 Space
steps reach new line 2808; one b step reveals +QVALUE=42411 with its adjacent
literal server> ready/a > b. Escape and subsequent Unicode insertion separately
prove abcXYZ and abcδXYZ; host scrollback is saved. The initial page timeout and
Herdr's atomic rejection of an invalid multi-character key are retained collector
errors, corrected without restarting or steering the development task. Actual
model Tool Processes 10–17 are observed exited; source inventory has no extra
non-target/non-.git paths. Native model exit and foreground fish are verified
before independent checker launch.

Independent verifier PID 32730, boot `7150c67e-18d8-4eea-809d-f0efb63e8cdd`,
receives project/protected roots read-only and checker scratch read-write. The
collector mistakenly submits cd without the explicit ! marker: one unintended
model turn unsuccessfully attempts Bash cd and directory ReadFile. The subsequent
explicit Cargo command also fails outside delegated cwd. These failures and the
extra model turn remain in the receipt, with no source/assertion changes. The
supported !cd route then sets checker cwd, and explicit Cargo a4 actually compiles
the dependency and passes protected_behavior under its own correlated Seatbelt
allow audit. Tool /proc/12 is observed exited. The failed verifier calls lack a
complete pre-exit Process inventory; do not claim that missing observation passed.
Both native PIDs are verified gone, foreground is fish 72808, and all project,
protected assertion and driver bytes stay unchanged by verification.

Elapsed submission through verified cleanup is 398.497340 seconds, within the
frozen 600-second bound. Receipt attempts/macos-f5-r3/a1/receipt.json SHA-256
`95dbbfd95aae9427a16f82aed6675eee8abd1a05e5f85366abd34b192e7564ef`.
Immutable first-outcomes-v19.json SHA-256
`ebd336f637cc627e9556aa182cd8adb89890ecbc07e20e470a5b4584147b06a1`
records twenty-one development attempts and fifteen qualified slots; first outcomes
are eleven PASS/four FAIL/fifteen NOT_RUN. The unintended verifier model turn is
an additional generation, not another development repetition or a hidden retry.
This run is not zero-unplanned-intervention. Usage/cost/time partitions are unknown.

All macOS families now have three qualified repetitions, closing task 2.5 and
bringing the checklist to 8/19. Earlier-candidate applicability and complete
cross-Host qualification remain open. Actual Ubuntu candidate connection list
returns no configured profiles and connection current reports effective_profile
none; filesystem catalog absence is no longer the only readiness evidence.
Head 186358e3 passed all sixteen checks before these evidence-only updates; the
next published head needs its own checks/review. Linux generation/profile setup,
final audit, user merge and canonical closure remain; no automatic routing or
production self-development is enabled.

## Final-candidate F1 r1 applicability — collector failure retained, fresh a4 PASS

The first applicability rerun a3 has correct separately frozen original F1 inputs
and actual source/tool identities, but its copied prelaunch-freeze.json still
contains F5 runtime/project/input fields. This conflicts with the task identity;
do not retrospectively rewrite the freeze or count a3 as qualification. The model
actually observes contract RED, changes only src/lib.rs and obtains ordinary
contract GREEN. No protected checker qualification is attempted after the collector
finding. All observed Tool Processes are exited, native PID 39754 is gone and
foreground is fish. Cleanup elapsed 75.788421 seconds. Failed receipt SHA-256
`7f7ddbdb3a0ee1e7a675ee4953128c48ae9485048b354e3471c31737c766a3a5`
at attempts/macos-f1-r1/a3/receipt.json. Immutable v20 SHA-256
`d4ab13450a2a650eea9054fee237a5f725ac09eebada6e235a0942b9d75b18b0`
records twenty-two development attempts, retaining the original first outcomes
and fifteen historically qualified slots. Pre-task approval syntax rejection is
also retained separately; it did not submit a model task.

Fresh a4 preserves the exact original F1 source/test/prompt/assertion hashes in a
new disposable fixture. All prospective task fields are reconciled with slot.json
before generation; assertions reject a stale project and runtime. The same final
35-file candidate, binary/config, PATH/tool digests and actual 80x24 geometry are
verified. Model PID 40172, boot `daa7186c-9989-4323-a746-991111008d64`, Root 8.
Own Meta and every turn context confirm gpt-6.1-sol/medium. Original a2 contract
failure records left 5/right 9, a3 retains the actual diagnostic that this fixture
has no Git repository, a4 changes only src/lib.rs, and a5 passes the unchanged
original contract. No repository is synthesized, test weakened, operator solution
edited or corrective model advice supplied.

Native detail exposes the original assertion failure; closing detail restores
abcXYZ and insertion at the saved cursor produces abcδXYZ. Host scrollback retains
failure/edit/GREEN/answer history. All actual model Tools are observed exited;
exact source paths remain unchanged apart from the allowed file. After actual
model exit, verifier PID 40578, boot
`b3b402d6-7b3c-42ef-9dc3-a87cc00cad0a`, receives project/protected roots read-only and checker
scratch read-write. Supported explicit !cd and Cargo a1 compile the actual library
and pass protected_behavior under the correlated Seatbelt allow audit. Tool /proc/9
is observed exited; all source/assertion/driver bytes stay unchanged. Both native
PIDs are verified gone and foreground is fish. Cleanup elapsed 117.915001 seconds.
Receipt attempts/macos-f1-r1/a4/receipt.json SHA-256
`7b2e4c0cfdebea90bfd94b479825b04b73cf5fe23af8fd5e6c1c2ca62f37f2e6`.

Immutable first-outcomes-v21.json SHA-256
`bec151bf36c520cce0b236266bc3160447a135acbe0f73047e0953555206aa7c`
records twenty-three development attempts plus the separately disclosed unintended
diff-verifier generation. Historical qualification remains fifteen slots and
original first outcomes eleven PASS/four FAIL/fifteen NOT_RUN. Comparing the actual
qualified receipt source-file inventories and binary hashes proves four slots on
the matching final candidate: F1 r1 and F5 r1/r2/r3. Eleven older macOS slots still
need applicability reruns; all fifteen Linux slots remain unexecuted. Usage/cost
and exact time partitions remain unknown. The checklist stays 8/19; this is not a
final cross-platform qualification or unattended self-bootstrap claim.

## Final-candidate F1 r2/r3 PASS — all three small repairs requalified

Both fresh repetitions keep the original fixture source/test/prompt/assertion
hashes. Their prospective input metadata agrees with the separately frozen slot
records; stale project/runtime negative assertions pass. Before generation, the
same 35-file candidate, CLI/config, actual selected/resolved tool digests and
80x24 geometry are checked. No earlier qualified receipt is replaced.

F1 r2 a2 model PID 48350, boot `e1f6439e-8336-4385-af48-2d8f7f4a9113`, Root 8:
a2's actual contract RED records left 6/right 5, a3 retains the real non-Git
diagnostic, a4 edits only src/lib.rs to remove the extra addition, and a5 passes
the original contract. The answer acknowledges that Git verification is unavailable;
the collector does not synthesize a repository. Verifier PID 48696, boot
`aafddd99-b53b-43f7-8b87-167c6c198e91`, receives project/protected inputs read-only
and checker scratch read-write. Explicit Cargo compiles the actual library and
passes protected_behavior under its own correlated Seatbelt allow audit.
Cleanup elapsed 130.840066 seconds. Receipt attempts/macos-f1-r2/a2/receipt.json
SHA-256 `1e499e15c3c49d7fa137e7db9980b5aac8ef441f432c86d85042252456f9c63d`.
Immutable first-outcomes-v22.json SHA-256
`f2654fa0b3a3b774b7418c2fcaccd706874d2be3fa34ee8208b6d6f301a3cfe8`.

F1 r3 a2 model PID 48825, boot `695e328b-fb1d-4e1b-8586-2dfcec3d1da1`, Root 8:
a2's actual contract RED records left -5/right 5, a3 inspects the project, a4
edits only src/lib.rs to clamp x and ignore y, and a5 passes the original contract.
Verifier PID 49201, boot `09f56ac5-e0a9-4db7-8214-cb6bcb7ce108`, independently
compiles the read-only library and passes protected_behavior, with the actual
exited Tool and correlated Seatbelt allow audit. Cleanup elapsed 199.570367 seconds.
Receipt attempts/macos-f1-r3/a2/receipt.json SHA-256
`997fd0ddcc9e11ba9489803de52842015543e58b1fc3417f941b53539abb36ba`.
Immutable first-outcomes-v23.json SHA-256
`eb5a7c125953ca5215a3f89cf64a5cc646b1009c3db275b2ea3d412091fa9c09`.

For both tasks, original detail shows the actual failed assertion, Escape restores
abcXYZ, insertion at the saved logical cursor produces abcδXYZ, and host scrollback
retains RED/edit/GREEN/answer. Exact source paths are unchanged except the allowed
file; all project/assertion/driver bytes stay unchanged by independent checking.
All observed model/checker Tool Processes are exited, both pairs of native PIDs
are verified gone, and fish 72808 is foreground. There is one planned setup approval
per model instance, no corrective advice, operator solution edit, restart or extra
verifier generation. Own Meta and every turn context confirm gpt-6.1-sol/medium.
Memory remains enabled, stores are not cleared, and repetitions are not claimed
as independent statistical samples. Usage/cost and exact time partitions remain unknown.

Aggregate v23 retains twenty-five development attempts, fifteen historically
qualified slots, first outcomes eleven PASS/four FAIL/fifteen NOT_RUN, and the one
separately disclosed earlier unintended verifier generation. Actual qualified
receipt source-file inventories and binary hashes now match six slots on the same
final candidate: all macOS F1 and F5 repetitions. Nine older macOS slots and all
fifteen Linux slots remain open; checklist remains 8/19. Head 1d6aa632 passes all
sixteen CI checks before these evidence-only updates; the next head needs its own
checks. Actual Ubuntu connection current still reports effective_profile none.

## Final-candidate F2 r1/r2/r3 PASS — all declared callers independently checked

All three fresh projects preserve the original source/prompt/assertion hashes from
fixtures-v3 (manifest SHA-256
`b64ad0126be948a69d46f4936cf25caacc6ff8d69b61bcda6e2b37229c32b25e`).
Only project/cache/Process locations change. Task fields, including the actual
runtime, are reconciled before submission; stale project/runtime negative checks
pass. The same cace913c plus 35-file candidate, CLI SHA-256
`1b609034eea7c8c2a7f7f5a1725d14fb6ed5165ec4776a196bd7237162a2495e`,
config, selected/resolved tools and actual 80x24 geometry are checked. Own Machine
Meta and every turn context confirm gpt-6.1-sol/medium.

Each model records an actual non-Git inspection failure, inspects through its
granted project, then edits a4's library API/body and a5/a6's two callers. Exact
file inventories show only src/lib.rs, src/main.rs and src/bin/report.rs changed.
The recorded source calls the new API with (17,3) and (-5,4), rather than replacing
the programs with hardcoded answers. Model a7 compiles the library and runs both
binaries. Zero library tests are acknowledged, not used as sole success evidence.
After model exit, a separate native instance mounts project/protected assertions
read-only and checker scratch read-write. The prospectively frozen compound check
compiles the real library, passes protected_behavior and separately compiles/runs
both actual binaries under its correlated Seatbelt allow audit. All source,
assertion and driver bytes remain unchanged by checking.

| Slot / attempt | Model PID / boot | Verifier PID / boot | Actual binary outputs | Submission through verified cleanup |
| --- | --- | --- | --- | --- |
| F2 r1 / a3 | 56279 / fe0b07ad-3259-4fd2-b066-be54f1665c65 | 56662 / 1f02c063-9782-4240-8024-e37f56014879 | 20, -1 | 266.367256 seconds |
| F2 r2 / a2 | 58132 / 5324a47e-a5c8-4392-8be1-9b0a3f590f12 | 59220 / 07939874-c661-443e-a3fa-1eb6076f7558 | 51, -20 | 120.020108 seconds |
| F2 r3 / a2 | 59607 / 3c7528f9-9476-4a66-93a0-f57dce30d53b | 60243 / 999403c4-d4d9-477e-baf0-d2be907a392b | 3, -5 | 197.772676 seconds |

Native Ctrl+O details expose all three actual edits with removed/added markers.
The callers' substring edits correctly show changed calls rather than an invented
whole-function hunk. The r3 library's closing brace wraps at 80 columns but remains
present. Escape restores abcXYZ; insertion at the saved logical cursor yields
abcδXYZ. Host scrollback retains inspection failure/edit/verification/answer.
Actual Tool Processes are observed exited; both native PIDs per attempt are
verified gone and fish 72808 is foreground. Each task has one planned setup
approval, no corrective model advice, operator solution edit, task restart or
unintended verifier generation. Memory remains enabled, stores are not cleared,
and no statistical-independence claim is made. Collector notes retain rejected
observer paths/metadata queries and corrected diff assertions; the r3 verifier's
initial unpublished host.json is reconciled with the same live PID, not restarted.

Receipts under the owned qualification cache:

- attempts/macos-f2-r1/a3/receipt.json SHA-256
  `c1db69522a272104a9b8bad32372ae4aa18bd09add90d5e8b8da506a95aaa42b`;
  first-outcomes-v24.json SHA-256
  `8ff02c5f7631db6fff24b1be56f00ca0271dac0bdcb172d6b4fcfe84b206fe94`.
- attempts/macos-f2-r2/a2/receipt.json SHA-256
  `330cd58dab826450513999cca544c0754864be9754ecd7170a3555ca9b514242`;
  first-outcomes-v25.json SHA-256
  `47c1619f29f6287df489d5044a1935772ed53c2c806db00c6b4f441b76e1c3f7`.
- attempts/macos-f2-r3/a2/receipt.json SHA-256
  `a8f7da36f872ba858265311573136deff65fa16abb11637c9560dbff13eccdb3`;
  first-outcomes-v26.json SHA-256
  `2d74fe3cf8e2b79ec8d94b035975abe9f6c1770372564af20e6eeb12afd3002b`.

Aggregate v26 retains twenty-eight development attempts and fifteen historically
qualified slots, original first outcomes eleven PASS/four FAIL/fifteen NOT_RUN,
and one separately recorded earlier unintended diff-verifier generation. Actual
receipt source inventories and binary hashes match nine slots: all macOS F1/F2/F5
repetitions. Six older macOS F3/F4 slots and all fifteen Linux slots remain open;
checklist stays 8/19. Usage/cost and model/Tool/wait partitions remain unknown.
Head db94f855 passes all sixteen checks before these evidence-only updates; their
new head needs its own CI. No unattended self-bootstrap or merged delivery is claimed.

## Same-source Linux candidate build and affected Rust checks — no model task

An archive starts from exact cace913c and replaces/adds the same 35 frozen native
candidate files, including untracked repair modules. Every archive member is a
unique relative regular file/directory, with no traversal or links. Only public
repository source is transferred to the owned Ubuntu UID501 cache
/home/morris/.cache/Alan/qdev-final-candidate-20261011-v1. No private Stores,
credentials or model solutions are copied, and no installed runtime is deployed.
Source archive SHA-256
`228b952c00512adcf62ee60f0243cb67e73bad53923a2498e09529d7f8916540`;
all 35 file hashes match both after extraction and after tests/Clippy.

Actual cargo build --locked --offline -p alan --bin alan passes. CLI SHA-256
`3754e37f1660489f170d67242563c09e50a3ceace16b63afa5a29d9553c70ec0`;
--version succeeds, and actual connection current reports effective_profile none.
The selected qualification Git prefix and existing task-owned Cargo registry
cache are reused. A first offline test command fails because arrayvec 0.7.6 is
absent; the log is retained. The same locked five-crate test command then downloads
locked registry dependencies and passes 2372 tests, with ten existing provider/doc
or other ignores retained: Engine 1435 unit + 20 integration; LLM 212 + 8;
Service Manager 158 + 2; TUI 381 + 12; Tools 144. Workspace cargo fmt --check and
those five crates' all-targets/all-features Clippy with warnings denied pass.

The qualified PATH still lacks rg, just, openspec, node, npm and npx. Thus these
affected Rust checks are not a complete Linux normal-quality gate. No Linux
generation, matrix slot or final cross-Host qualification is inferred. The
candidate combines #1046's base with the observed repair files; final merged-source
reconciliation still remains. Cached linux-final-candidate-v1/receipt.json
SHA-256 `9282ddb02551cb8f8c08ed874344f678e1289bd93f1fd7f5dd594260f3713e3a`
pins public source/transfer/build/test/readiness/quality artifacts and the first
offline failure. Linux model/profile readiness and all fifteen tasks remain open.

## Final-candidate F3 r1/r2/r3 PASS — actual compiler causes corrected

All three fresh tasks preserve the original fixture, prompt and protected assertion
hashes. The task/slot/runtime fields agree before generation, stale project/runtime
negative checks pass, and the same 35-file cace913c candidate, CLI/config/tool
digests and actual 80x24 geometry are checked. Own Machine Meta and every turn
context confirm gpt-6.1-sol/medium. No original receipt or first outcome is replaced.

F3 r1 a2: a2's actual Cargo failure is E0425, cannot find BTreeSet. After reading
the library/contract, a4 adds its missing std::collections import, retaining the
original collection/counting body; a5 passes the unchanged contract. Only
src/lib.rs changes, with no new files, dependency edits or weakened tests.

F3 r2 a2: a2 retains E0583 for the missing operation module. a3 reads the library,
contract and manifest but ends with the real non-Git failure; a4/a5 inspect the
actual source directory and existing operations.rs. a6 changes the module
declaration and call to operations, leaving operations.rs and its generic
value * 2 + 1 implementation byte-identical. a7 passes the original contract.
Exact inventories again show only src/lib.rs changed.

F3 r3 a2: a2 retains E0308 because parse returns a Result rather than the declared
i32. a3's actual source/test inspection and non-Git failure stay recorded. a4
retains parse::<i32>() and adds unwrap_or(0), following the invalid-input contract
instead of the compiler's panic-producing expect suggestion. a5 passes the
unchanged contract. Only src/lib.rs changes.

| Slot / attempt | Model PID / boot | Verifier PID / boot | Submission through verified cleanup |
| --- | --- | --- | --- |
| F3 r1 / a2 | 68195 / db9ec378-7711-42f4-8775-14b7ad971bfc | 69298 / fb97e97c-1070-4c2e-a8e9-c6987f91743b | 237.970639 seconds |
| F3 r2 / a2 | 69786 / 307057fd-15da-474f-8fe1-37c2e21d3f98 | 70151 / 703003fc-5784-4e1e-8380-d93f4fb1f363 | 200.697360 seconds |
| F3 r3 / a2 | 70295 / 9c16b5e9-c965-4f44-a1af-0608785300dd | 70573 / b8788162-eb89-4f80-b4ac-217dbfb7fadf | 159.668232 seconds |

Each native Ctrl+O error detail shows the original Command stderr and actual
compiler code. Escape restores abcXYZ; insertion at the saved logical cursor
produces abcδXYZ. Host scrollback retains RED/inspection/edit/GREEN/answer.
All model Tool Process paths come from actual Action records and are observed
exited. After actual model exit, a separate native verifier receives project and
protected assertions read-only, with checker scratch read-write. The frozen Cargo
command compiles the actual library and passes protected_behavior under its own
correlated Seatbelt allow audit: duplicate/empty counting, the existing operation,
and valid/invalid parsing respectively. Project/assertion/driver inventories stay
byte-identical through checking. The actual checker Tool is exited, both native
PIDs per task are verified gone, and fish 72808 is foreground. One planned setup
approval per task, no corrective advice, operator solution edit, task restart or
extra verifier generation. Memory remains enabled, user stores are not cleared,
and no statistical-independence claim is made.

Receipts under the owned qualification cache:

- attempts/macos-f3-r1/a2/receipt.json SHA-256
  `8386abe992537ed45b0574b686f98e1f65aa37f76fa15afbf3e9120fa930452d`;
  first-outcomes-v27.json SHA-256
  `5344193277316665b9122a5acd4c6da9dcf16714e99bf65cba24fd685a09d1e9`.
- attempts/macos-f3-r2/a2/receipt.json SHA-256
  `c1eaa46363161dfa684bc0141a4224a7c09578c223b9c11cb6a7c198002b3000`;
  first-outcomes-v28.json SHA-256
  `9f8bd835947e26c23fb7b754fe90c2d1c14934df47b5da85683223f51f4dc83b`.
- attempts/macos-f3-r3/a2/receipt.json SHA-256
  `58da21c8e9b1d3acc81523f8180d66ba9995967f751577a3141713541607cd96`;
  first-outcomes-v29.json SHA-256
  `28c360fce3f8d37276caca49bbda88189412645af3f685be5920270afdb67357`.

Aggregate v29 retains thirty-one development attempts, fifteen historically
qualified slots, original first outcomes eleven PASS/four FAIL/fifteen NOT_RUN,
and the one separate earlier unintended diff-verifier generation. Comparing
actual qualified receipt inventories and binary hashes proves twelve slots on
the same final candidate: all macOS F1/F2/F3/F5 repetitions. Three older macOS F4
slots and all fifteen Linux slots remain open; checklist stays 8/19. Usage/cost
and model/Tool/wait partitions remain unknown. Head c5f9b924 passes all sixteen
checks, including completed CodeQL, before these evidence-only updates. Their
new head requires its own checks; no final merged-source/cross-Host qualification
or unattended self-bootstrap is claimed.


## Final-candidate macOS F4 lifecycle reruns complete — 2026-10-11

All three original cancel/revoke/explicit-recovery tasks now pass on the same
candidate as the final F1/F2/F3/F5 runs: CLI SHA-256 `1b609034`, tracked diff
`60fbb03a`, and the unchanged 35-file source inventory on cace913c. Each fresh
project is reconstructed with the existing fixture generator and checked against
the original fixtures-v3 project, prompt, follow-up and protected-assertion hashes
before generation. Original fixture defects, allowed files, 80x24 geometry and
600-second bounds are unchanged. Native versions, executable hashes, config,
observer and call-correlated Seatbelt setup approval establish actual readiness.
All initial and resumed model contexts retain gpt-6.1-sol/medium. Memory remains
enabled and uncleared; these repetitions are not independent statistical samples.

Each initial model invokes the frozen start_work command once. Actual delayed
writer, parent and process group are observed live before planned Ctrl+C, then
observed gone. After the original forty-second deadline, late.txt is absent and
the completed ledger entry occurs exactly once. Source remains defective before
recovery. At a settled paused boundary, /project revoke selects cwd / and the
owning Host grant becomes inactive. The unchanged denied-read follow-up is
continued alone; actual Read output contains an authority/adapter error and no
ledger content. Neither new authority nor additional work is requested.

Before /quit and again before --resume, the shared durable selector is compared
to the exact owning initial rollout. A fresh boot retains the frozen task, with
no active or pending work before explicit regrant. A newly correlated cwd Action
precedes the unchanged continuation and one actual structured reapproval request.
After planned approval, Read/EditFile/Cargo change only src/lib.rs to x + y and
pass the original contract. Newly owned Action IDs and call IDs distinguish new
execution from copied recovery history and virtual cd. The only resumed native
Bash command is the contract test; start_work is not replayed, the ledger stays
one entry, and writer.pid/writer.started stay unchanged through recovery.

| Slot / attempt | Initial PID / boot | Resumed PID / boot | Verifier PID / boot | Submission through verified cleanup |
| --- | --- | --- | --- | --- |
| F4 r1 / a4 | 78910 / 4d424946-cae0-4ca0-b431-01cafe39728a | 79205 / 5d262211-6d1e-441a-b3f6-82cd75d53628 | 79491 / 9f5b8599-8cf6-4ac7-98be-8040998dd00c | 270.090501 seconds |
| F4 r2 / a2 | 80506 / 96ac4b3b-9493-4b8c-abe3-1317bcdaa868 | 80685 / 07cc1491-5da5-495b-b196-98fd24cacbaf | 80857 / bb11f58a-a993-4d8f-92c3-7c8727813cfd | 197.574830 seconds |
| F4 r3 / a2 | 81020 / a33067c9-38d5-47e9-8adc-7af62145ab45 | 81200 / 662ac31f-31e9-40c9-8b60-d2d118742ac8 | 81355 / 9ea9b6bd-264c-4452-be63-50b715747e91 | 191.562333 seconds |

Cancelled writer/parent/group identities are respectively 79087/79084/79041,
80598/80596/80552 and 81124/81122/81079. Each resumed Root is instance-local 8;
new Read/EditFile/Bash references 9/10/11 are observed exited in that boot.
Original Cargo detail correlates to a7/a8/a9 respectively. Escape returns the
draft abcXYZ; insertion at its retained logical cursor produces abcδXYZ, and
Host scrollback retains task/control/answer history. The emptied draft precedes
actual native exit. Separate native verifiers grant project/assertions read-only
and dedicated checker scratch read-write. Fresh protected_behavior compilation
passes, with actual call ID matched to its Seatbelt allow audit. Project,
assertion and driver inventories stay byte-identical through checking; each
verifier Tool exits, all nine native invocation PIDs are gone, and fish 72808 is
foreground before reuse. No operator solution edits, corrective model advice,
unplanned revoke fallback, task restart or verifier model generation occurs.
Planned setup approvals, cancellation, revoke, queue control, regrant and
reapproval remain explicit human interventions.

Collection limitations stay explicit. The cancelled native Tool has no published
Action, and its individual Kernel Tool status was not collected before initial
instance exit; that status is not claimed as observed. Actual cancelled native
descendants/group and initial-instance exit are independently verified. r3 also
collects exited statuses for the published initial Tool references. r1 retains
pre-generation source-path/event-shape/queue/model-field corrections and a later
valid observation beyond its deadline. Recovery command checking initially
included copied history and virtual cd, then used actual new Action/call origin.
r2 retains an early approval observation and an incorrect expected detail ID;
the same live request was reobserved and actual a8 derived from its own Action.
None of those collector corrections restarts or steers a model task.

Receipts under the owned qualification cache:

- attempts/macos-f4-r1/a4/receipt.json SHA-256
  `ec82476c586d4bf88a7575634d5492e300b9aa7ffbf2fa1d63c70a645121e7d8`;
  first-outcomes-v30.json SHA-256
  `dee0a8cfd8a70c155fc16af1f0167910c16f1c81e10c4534744e0147a53055c5`.
- attempts/macos-f4-r2/a2/receipt.json SHA-256
  `2884136889bdbdf7f9422ff80bab83e485def86cc6c1675e16870a02eff65cd1`;
  first-outcomes-v31.json SHA-256
  `ef76a94c4f61bb5b705ea9dfaa6c4399338ffc5b379069c0b35db6e522012be9`.
- attempts/macos-f4-r3/a2/receipt.json SHA-256
  `104cf4014dae776123a1603b3b724a343f3c024b6c8f0805b431529fab36fd49`;
  first-outcomes-v32.json SHA-256
  `2e0ce1b0772255aa1accb777d08051e09fd46329af6a648ef90e6ecea121466c`.

Aggregate v32 retains thirty-four development task attempts, fifteen qualified
slots and original first outcomes eleven PASS/four FAIL/fifteen NOT_RUN, plus the
one separate earlier unintended diff-verifier generation. All fifteen macOS
slots now match the exact final candidate; all fifteen Linux slots remain open.
Checklist stays 8/19: cross-Host readiness, final repair/qualification audit,
complete quality and user merge/canonical closure remain requirements. Usage,
cost and model/Tool/wait partitions remain unknown. Previous head 803b49be passes
all sixteen checks, including completed CodeQL; this evidence-only update
requires its own current-head checks. No merged-source/cross-Host qualification,
general autonomous self-bootstrap or automatic routing is claimed.

## Complete Linux candidate quality and fresh native preflight — 2026-10-11

The same frozen cace913c plus 35-file production candidate now passes actual
`just quality` in a real, owned public Git checkout and separate output directory.
The terminal command exits zero; Rust architecture/size, format, Clippy, rustdoc,
script checks, Host-source boundary, current OpenSpec guard and standalone CLI /
distribution checks pass. All 2283 tracked source files and the 35-file inventory
match the earlier public archive and remain unchanged after the gate. This uses
the cace913c candidate's quality script; it does not substitute for phase-specific
fixture checks or final merged-source qualification. Actual quality-built CLI
SHA-256 is `6b7565827d0b8479d2a6d6315deb51e71a1c9c2eaa8c8982171fd9f4f95ae125`.

The tooling setup retains its first stale-APT-index failure before refresh and
its incomplete private-prefix Git HTTPS helper failure. Standard Git 2.53.0 with
its required runtime dependencies restores actual public HTTPS clone. This Host
clone is not confined Tool Git-network qualification. Isolated OpenSpec 1.4.1
uses Node/npm for the repository workflow only; no Node/Python task qualification
is implied. A first diff-hash comparison used Linux's default seven-character
abbreviation versus the original eight; explicit `git diff --binary --abbrev=8
HEAD` matches `60fbb03a`. File identities stayed unchanged; the initial comparison
failure remains retained rather than presented as a source mismatch or a pass.

A fresh ordinary native 80x24 PTY uses the actual quality-built CLI, boot
`014268f4-674b-4a99-ba44-da3ebfbfdf7e`, Host PID 3561564 and instance Root 8. Three
acknowledged grants mount Source and protected assertions read-only and dedicated
checker scratch read-write. These operator-authored preflight files are separate
from all frozen matrix inputs. Explicit cd settles at `/mnt/project-request-3/`.
One compound version/status/diff command receives actual human approval once;
Action a1 and call `e4cdcabd-5e94-48bd-bb15-27dcc8a04bae` report exit zero and
retain the before/after Git diff. Its policy is `safe_degradation`, unknown,
`human-bash-unconfined`, under `linux_reified_namespace`. This is the existing
conservative classification boundary, not an unattended/full-confinement claim.

The next explicit `cargo test --offline --locked --target-dir target` is
classified write and allowed by `builtin_autonomous` under that same backend.
Action a2 and call `398ac58e-4deb-4d98-84e0-b50d7ab34d3a` report exit zero.
The actual compiled protected test reads both read-only trees and requires
PermissionDenied/ReadOnlyFilesystem on attempted append to source and assertions.
All seven original file hashes remain unchanged. Published Tool references
`/proc/9` and `/proc/10` are observed exited before native exit; the actual queue
has no pending/active/uncertain submissions. `/quit` exits the PTY zero and Host
PID 3561564 is gone. Neither command is duplicated or sent to a model.

Actual Tool stdout selects Git 2.53.0 and Rust/Cargo 1.96.0 in checker scratch;
the repository quality gate uses its Rust/Cargo 1.97.0 toolchain. Future model
readiness must freeze actual task-cwd selections, not assume these are identical.
There is no selected Linux generation profile, no model turn context and no
credential/private-store copy. The displayed fallback model is not a substituted
qualification model. Collector errors are retained: the older SDK observer has
no `--list`, Action a1 has no `record` leaf, and durable `agent_action_v1` belongs
to `event_type`, not `type`; the corrected collection contains all three Actions.

Immutable receipts in the owning qualification cache:

- `linux-quality-cli-v2/receipt.json` SHA-256
  `c142a35bd7791b8baf06d9c5fba5e117ff510a9c2bcc7d055684a81816a55ac9`;
  setup/source identity, with quality still running at that receipt's creation.
- `linux-full-quality-v1/receipt.json` SHA-256
  `b33b46b59663460a1699df7e1a85fcba0c36d54820c0dca27ecb3d7c8adcd179`;
  complete candidate normal gate, source-after-gate proof and retained log hashes.
- `linux-final-native-preflight-v1/receipt.json` SHA-256
  `587a914083904b6db03c3d1670db66751b423e6e067a9bcefe8f1f1ba69ae2ed`;
  27 artifacts including policy/call-correlated Actions, Tool statuses, PTY,
  queue, baseline proof, collector corrections and actual exit.

No matrix count changes: thirty-four development attempts, fifteen qualified
macOS slots on the final candidate and fifteen Linux NOT_RUN slots. Checklist
stays 8/19. Head 02010e6a passes all sixteen completed checks before this
evidence-only update; the new head requires its own checks and review. Linux
generation readiness, full cross-Host qualification, final phase/merged-source
validation, formal review, user merge and canonical closure remain open.

## Linux bounded observer and candidate branch alignment — 2026-10-11

The existing public SDK observer is built in a separate owned Linux source/output
tree, preserving all 35 production source hashes. Observer source SHA-256
`8346bb55e301572bcf042e14e6892d0a8a54fc2f0b00f424fa59c49022a61d4c`
matches #1047; its three validation tests, offline locked build and warnings-denied
Clippy pass. Actual Linux observer SHA-256 is
`9e47877d62ca50440504edc425cfe410413ad8e753bbd3713560cc8322264368`.
This executable attaches through the existing public SDK and never replaces the
native CLI or generates model work.

A fresh normal CLI invocation uses unchanged native binary `6b756582`, boot
`2257937c-ca5c-41e4-9f7c-7482beacd023` and Host PID 3581959. Bounded numeric-Agent
listing initially returns static clone/events/help entries. The initial queue
has revision zero and known false; no settled-empty claim is made from it.
The SDK rejects an incorrect boot before attachment and refuses /agent/root
observation. One frozen operation UUID grants the owned Source read-only; a
same-ID reconciliation returns exactly the same acknowledged grant. One explicit
`!cd /` publishes Action a0. A bounded reread includes a0; the numeric Action
result exits zero, and the actual queue becomes known with no pending/active/
uncertain work. Its exact call ID selects only the owning durable rollout,
which has one cd Action and no model turn contexts. `/quit` exits the child zero
through `script --return`; the Host PID is verified gone, both binaries unchanged.

Collector/setup errors remain retained. The first launch lacked the exact owned
runtime parent and terminated with an error before starting an instance; the
original script did not propagate its child failure. A new launch follows only
after the actual terminal failure, creates the owned parent and enables explicit
child-exit propagation. Initial assumptions that the Action directory was empty
and that its untouched queue was already known fail before any mount or input.
Their observations are retained and corrected on the same live instance, without
duplicating a model task or replacing an uncertain operation.

Immutable owning-cache receipts:

- `linux-sdk-observer-v1/receipt.json` SHA-256
  `412c5ee9f1b08be0431a64aa7f285e36aab12026b8990743bc03567f7c03dc66`;
  source identity, actual binary, build/test/Clippy log; native attachment was
  not run at that receipt's creation.
- `linux-sdk-attachment-v1/receipt.json` SHA-256
  `f4b35c680aae0bee3ae08c4c5b562c091965cfc86181f7ec05dcb4e21889b618`;
  34 retained artifacts, boot/grant/Action/queue correlations, failure/correction
  history and actual child/Host exit. Both receipts count zero model tasks.

#1047 previously targeted main while qualifying a composed Linux-predecessor
candidate. The reviewed predecessor #1046 head cace913c still passes all sixteen
checks. A prospective conflict-free merge compares all 827 crate/workspace
manifest files with the actual frozen native source, with no absent or differing
files; only the qualification observer example is additional. The qualification
branch incorporates that exact predecessor without rewriting published history.
The existing complete CI workflows filter pull_request bases to main/master, so
#1047 keeps main as its base and explicitly requires #1046 to merge first. This
makes source applicability explicit before Linux generation while preserving
current-head CI; it does not adopt ADR-0058 or merge/deploy either PR. Parent
first, user merge and final main-source validation remain gates.
The new integration head requires full quality, strict OpenSpec and its own CI.
All fifteen Linux real-model slots, cross-Host audit, usage/cost and final delivery
remain open; checklist and matrix counts are unchanged.

## Linux review repair integration and macOS applicability — 2026-10-11

Parent #1046 head `258a956ae72449145485c1bc82ed1f34f57edfe0` completes all
sixteen current-head CI checks, including coverage. It fixes overbroad Rustup
runtime projection by retaining only command-selected runtimes, without admitting
shell evaluators that the existing Linux entry already refuses. Its subsequent
Package concurrency change is test-only: busy has no installation effects, and
retry uses a new request ID after the peer releases the lock. The production
500 ms lock budget is unchanged. Parent native engine evidence is 1431/0/1;
both Hosts' Service Manager suites pass 157 unit and two integration tests, and
the affected LLVM-instrumented concurrency case passes. These are regression
checks, not model development slots.

The qualification branch incorporates that parent through a conflict-free merge,
preserving published history and main as the PR base. A complete comparison
against the frozen macOS source finds exactly four differences among 827 files:
`tools/reified_namespace/rustup.rs`, its adjacent tests, Sandbox Rustup tests,
and Package concurrency tests. The first is Linux-only in a production build;
the other three are tests. The actual frozen CLI's `alan.d` lists 437 workspace
source/resource inputs. Every input exists and matches the integrated source,
and none is one of those four files. The actual CLI still hashes to
`1b609034eea7c8c2a7f7f5a1725d14fb6ed5165ec4776a196bd7237162a2495e`.
This establishes unchanged macOS compiled-input applicability for the fifteen
retained qualified slots, not a new binary build or fresh model execution.

Immutable applicability receipt `linux-review-alignment-v2/receipt.json` has
SHA-256 `4cda62271f0c938c98380babdb7a3d3757b5af97f62cad3101baf8939c51daee`.
It retains the dependency record, all input hashes and four old/new source hashes.
The first collector incorrectly required more than 500 inputs although the actual
record has 437; `linux-review-alignment-v1/collector-error.json` retains that
collector failure. The corrected check requires concrete CLI/engine/service
inputs and exact equality, rather than an invented count threshold.

The older Linux binary `6b756582` contains the pre-review runtime implementation.
Its quality, PTY and observer receipts remain historical and cannot qualify the
updated Linux candidate. A new integrated build/native check is required before
Linux generation, which still has no configured Connection profile. Counts stay
34 development attempts, 15 qualified macOS slots and 15 Linux NOT_RUN slots;
all original failures, the separate verifier turn and missing usage/cost remain.
Normal commit quality, strict OpenSpec and this integration's own CI remain gates;
parent CI does not qualify this child head. Neither PR is merged by this record.

## Updated integrated Linux candidate — 2026-10-11

Integration head `8e9e29f2441a541de253df8b1ae3c70b5964a656` completes its normal
macOS commit quality/standalone/distribution and pinned strict OpenSpec 70/70.
Its public source tree `b427ee3a87f073aa365c5ee8701b1bfec8c202eb` is archived
without Git metadata, credentials or private Stores into a fresh owned Linux
cache. A native snapshot checkout has exactly that tree, 2307 tracked files,
and clean source before and after verification. Its build output is registered
only to that native checkout; no foreign output registry or old frozen source
is modified. The first quality launch exits before Cargo because the operator
PATH points to a nonexistent OpenSpec bin directory. Its log remains retained;
the corrected task-only PATH uses the existing isolated node_modules/.bin.
No global PATH, package defaults or production lock budget changes.

Complete normal Linux `just quality` passes, including workspace format,
all-targets/all-features warnings-denied Clippy, private documentation, repository
Python/fixture checks, host/OpenSpec boundaries, standalone CLI and distribution.
New debug CLI SHA-256 is
`b09570dcad1dfb1efc5a43af2195be674cbd99f9f3e6bba9464ded93e45ca693`;
its actual dependency record includes the repaired Linux Rustup production file.
The subsequent Engine suite passes 1436 unit and 20 integration tests, retaining
one existing ignore; Service Manager passes 158 unit and two integration tests.
Explicit native Rustup proxy/runtime inputs cause all 23 adapter cases to run;
the selected-only visibility check and direct/nested wrapper refusals actually
pass. The Package busy/no-effect/new-ID retry check also passes. Tests finish
normally with no failures, and the CLI remains unchanged.

Updated candidate receipts in the owning cache:

- `linux-review-quality-v1/quality-receipt.json`, SHA-256
  `3ded8c5cbd0e52b6e8e849cc32328f58692f12c482c3b280dc79801418993c5c`,
  retains all public source hashes, exact tree/native snapshot/CLI identities,
  original failed environment log and successful quality/distribution log.
- `linux-review-native-v1/native-receipt.json`, SHA-256
  `770be93af6ef2e1673ec355c04656faf4694dd7ce1672571b00a4e6c28460464`,
  retains the native log, actual Engine ELF digest, test results and explicit
  runtime/proxy identities. Engine ELF SHA-256 is
  `f32525364c07fcab476cc709597a9f43a1c8feed8edf5642927465be82eed4ce`.

The actual new CLI's `connection list` still returns no configured profiles.
No model is substituted and no credentials are imported. These updated build and
native regression receipts add zero model slots, preserve all 34 task attempts
and first outcomes, and do not replace the previous ordinary-PTY receipts with
a claim of a new PTY run. Actual task-cwd tools/profile/medium effort must still
be frozen before Linux generation. The following evidence-only documentation
commit keeps production source unchanged but needs its own normal quality,
strict OpenSpec, current-head CI/review and final user-merged source verification.


## Shared-projector final-source refresh — 2026-10-11

The normal ancestry-preserving integration incorporates #1046 `a6ea2518`, whose
sixteen checks pass. Its leading-boundary repair is reproduced and fixed in the
shared Host projector; native Host/Clippy and normal commit gates pass on both
applicable scopes. The punctuation suggestion is evaluated rather than widening
path recognition to legal unknown sibling names. Parent CI/review receipt SHA-256
is `267b3cfb4b095296ad92b5529ea31006dbc97ab3503fea2d7a6906820da12d65`.
This does not grant formal approval, merge a PR or adopt the proposed grant policy.

Actual frozen-CLI input audit compares all 437 inputs and finds exactly one
change, `crates/os-host/src/host_mounts/text_projection.rs`, from
`7386df2971a2698262c81a5e4689d8fd1e5cbee6d904dc5db83e3f5b7bc9de05` to
`df211150cdfbe614259152223abc7475ed8a101e69f3eadcf14353b949a7ec36`.
Because Tool stdout/stderr/error projection is shared, conservatively refresh all
fifteen macOS rows. Earlier 437-input equality and prospective `499b352c` Linux
build remain historical. Source-applicability receipt SHA-256 is
`f582c6344da458c9090954e67b20f95ff3ded535b222ca5ca9f592ae6a4adbc4`.
The owning cache is `dev-boundary-20261011-v1`; new candidate binary/readiness,
normal integration quality/CI and all real-model reruns remain pending.

Fresh materialization of the same thirty original slots has manifest SHA-256
`4894a588888396e6f88ae8aefc21adf10f4f0cef62519eabcb23b2a002b46e7e`.
Every prompt/follow-up, project baseline inventory, protected assertion inventory,
allowed-change list, 80x24 geometry bound, 600-second limit and initial long-diff
hash matches the original `b64ad012` manifest. Operator checker backing paths and
local setup Git commit identities are fresh; no model prompt gains backing paths,
no protected assertion changes and no corrected solution enters a new project.
This adds zero model attempts; original thirty slots, thirty-four attempts and
first outcomes remain retained. Linux generation remains unconfigured; no private
Store or credential copy, model substitution, automatic routing or production
Alan self-development is authorized by this qualification.

## Integrated candidate freeze and first refreshed task — 2026-10-11

Integrated head `465e3e474ee8af98a7647b5eee06e748b9fd9aa6` has public tree
`2fac9b16c2ba4a00edd1ed1ad643475ff7a22d6d`. Its normal commit hook passes
quality/standalone/distribution; pinned strict OpenSpec is 70/70 and fixture
self-checks are 6/6. All sixteen checks pass at this exact head. The immutable
`current-head-ci-v1/receipt.json` SHA-256 is
`909dba16bd039d61d666b0ae49e13762677c9bebb10645276467d9ba9979ca84`.
Formal review, user merge and canonical closure remain distinct gates.

The separately frozen macOS CLI SHA-256 is
`81cff69cc17c47ac66c3fdb2fe369f6515f63a273a35d7b490da1a862073b1a6`;
all 437 recorded compiler inputs match committed blobs. Candidate receipt
`macos-candidate-v1/receipt.json` SHA-256 is
`1c8c17f80085644e25b977eb8d4e508d234e79dd23f44b5873eddb893fb56577`.
The native Linux public checkout is clean with the exact same tree. Its frozen
CLI SHA-256 is `c59e8b0bee3785a5908da0857e4e386f834cc894cadfad313d4ef8bac59b2d22`.
Actual native CLI build passes using the registered task-owned debug-info-disabled
output; this is not a claim of complete default-profile Linux quality or fresh
PTY qualification. `linux-candidate-v1/receipt.json` SHA-256 is
`4d78190351e58b8dfcdea7a4334bfd41de341d1aebc48a0ed21965a47048457d`.
The actual frozen CLI still reports no configured Connection profiles. Public
source is copied without credentials, private Stores or original Git metadata;
the Linux build adds zero model slots.

Fresh `macos-f1-r1/a5` uses the unchanged original addition prompt, project
baseline, protected assertions and 600-second limit. Actual stdin PTY and Host
layout are both 80x24. Before generation, public numeric-Process observation
confirms `chatgpt-main`, `gpt-6.1-sol`, medium effort and a settled queue. The
planned version command selects Rust/Cargo 1.93.0 and Apple Git 2.54.0, matching
the prospective executable/runtime hashes from the actual project cwd; its
approval record confirms Seatbelt. Definition/config and default Memory exposure
are recorded without clearing memory or claiming independent statistical samples.
The separately frozen observer retains source `8346bb55` and binary `6ff28b7f`;
it attaches through the existing SDK rather than booting or generating work.

The real model retains contract RED in a2 (exit 101, observed 5 versus expected 9),
recovers without advice from a3's non-Git diagnostic, changes only `src/lib.rs`
in a5, and passes the unchanged original contract in a6. Model boot
`de657a34-3de5-407a-8426-b1b87df08719` owns Root `/proc/8`, submission
`182bb272-2be1-49bb-9858-f7ad952bf26c` and rollout
`dc570914-da3e-44f0-874b-286948cacde4`; its actual generation metadata confirms
the model and effort. All six Tool Processes are observed exited before `/quit`,
and the native invocation PID is gone before the separate verifier starts.

The verifier receives only project/protected assertions read-only and checker
scratch read-write. Ordinary explicit Cargo compiles the real model-authored
source with the original protected assertions and passes `protected_behavior`.
Source/tests/config/protected assertions/driver hashes remain unchanged through
checking, and both verifier Tool and native invocation exit. Behavior verification
takes 170.14 seconds; submission through verified cleanup takes 212.50 seconds,
within the predeclared bound. Time partitions, provider usage and cost remain unknown.

Raw text/ANSI captures retain both original failed/successful Action details,
Space paging, Chinese/emoji draft preservation after Esc, and Host scrollback.
Exact cursor coordinates are not measured by this refreshed task. One planned
`home` detail-navigation key is rejected by Herdr before delivery; its error and
supported-arrow continuation are retained, not hidden as a flawless UI operation.
One immediate verifier readiness read precedes publication and is reobserved on
the same launch without restarting. There are zero operator source edits,
corrective model messages, task restarts or unintended verifier model turns.
One planned model-project grant, one setup approval and three checker grants are
recorded; no approval-free autonomy or general macOS Host read isolation is claimed.

Receipt `attempts/macos-f1-r1/a5/receipt.json` SHA-256 is
`134320095f628e6547edea982282b1214adae806ed45dafc3e7b833e50330046`;
separate `cleanup-timing-v1.json` retains the verified-exit timing.
Aggregate `first-outcomes-v33.json` SHA-256 is
`97988ec9bce2aa187c2ebff25b81d3f9b7db68d545a693046cbfe716ed24e466`.
It retains all thirty slots and eleven first PASS/four first FAIL/fifteen NOT_RUN,
adds development attempt 35, and distinguishes fifteen historical qualified slots
from one final-source-qualified slot. Fourteen macOS refreshes and all fifteen
Linux real-model tasks remain open. No old outcome or receipt is overwritten.

## Remaining F1 refreshes on the same integrated source — 2026-10-11

Fresh `macos-f1-r2/a3` and `macos-f1-r3/a3` repeat the original absolute-difference
and inclusive-clamp tasks on new baselines and separate native invocations.
Before each generation, all 437 compiled input hashes and the frozen CLI digest
still match; original prompt/project/protected-checker bytes and allowed changes
are unchanged. Actual 80x24 geometry, effective `chatgpt-main`/`gpt-6.1-sol`/medium,
project-cwd selected executable/runtime hashes and Seatbelt are recorded. These
do not reuse already-corrected projects or replace either first PASS outcome.

Each real model records a2 contract RED, recovers from its own non-Git diagnostic,
edits only `src/lib.rs` in a5 and passes the unchanged Cargo contract in a6.
The r2 RED observes 6 instead of 5; r3 observes -5 instead of 5. The separate
native verifiers receive only read-only project/protected assertions and writable
checker scratch. Fresh compilation and original `protected_behavior` pass;
source/tests/config/assertions/driver hashes stay unchanged through checking.
All observed model/verifier Tool Processes exit, both owning native invocations
actually exit, and the owned pane returns to its shell. No production edits,
corrective model advice, generation restart or unintended verifier turn occurs.
Planned grants/approval/detail/lifecycle actions remain counted. Immediate
readiness observations precede publication once for r2 and twice for r3; the same
live launches are reobserved, never replaced to manufacture readiness.

Raw detail/scrollback captures preserve the failed and successful Action streams
and Space paging. r2's draft middle survives details: inserting delta after Esc
produces `abcδXYZ`, proving the logical insertion point. r3 retains its Chinese/
emoji draft after returning. Exact terminal cursor coordinates, general macOS
Host read isolation, provider usage/cost and model/Tool/wait partitions remain
unmeasured; the sample does not establish general self-bootstrap eligibility.

| Slot / attempt | Behavior verification | Submission through verified cleanup | Receipt SHA-256 |
| --- | --- | --- | --- |
| macos-f1-r2 / a3 | 152.10 s | 186.92 s | `4144cce18aa6cf6d0bbcd2af4cf3d6361721cc2640558070e304de2c9c5b90bd` |
| macos-f1-r3 / a3 | 180.61 s | 212.19 s | `d41c3bdb1deff8910b67e88908fb673b0400c25d32416b01aa55611e8dace7d1` |

Both finish within the original 600-second bounds. Numeric Process, fresh boot,
submission UUID and owned rollout identities are in the receipts; exact admitted
submission UUIDs are checked against each retained owned rollout, avoiding another
invocation's global selection. Receipt artifacts and every earlier linked receipt
are checked against their hashes. Aggregate `first-outcomes-v35.json` SHA-256 is
`dfaeaf9ba0958a55a6ad3262607752dd785ef30b9fa7d57345cfcb758ceb6b6a`.
It retains thirty slots and original eleven PASS/four FAIL/fifteen NOT_RUN,
with thirty-seven development attempts, fifteen historical qualified slots and
three final-source-qualified slots. Twelve macOS refreshes and fifteen Linux
tasks remain open; the checklist stays 8/19. This evidence-only follow-up changes
no compiled production input and needs its own normal commit quality and CI.


## Integrated-source F2 refresh and prelaunch UI finding — 2026-10-11

All three unchanged cross-file tasks pass on frozen integrated source `465e3e47`,
CLI `81cff69c`, with all 437 compiled inputs matching committed head `34fd4910`.
Each real `gpt-6.1-sol` medium invocation updates the function and both callers,
modifying only `src/lib.rs`, `src/main.rs` and `src/bin/report.rs`. The original
non-Git command failure remains; r1 also retains its absent-tests-directory
failure. Cargo's zero-test result is recorded honestly rather than qualifying
behavior by exit zero. Separate normal native verifier invocations receive
project/protected assertions read-only and fresh checker scratch read-write.
Original `protected_behavior` passes, and actual independently run binaries
produce respectively `20`/`-1`, `51`/`-20` and `3`/`-5` as complete stdout lines.
Source/tests/config/assertion/driver inventories remain unchanged by verification.

| Slot / attempt | Behavior verification | Submission through verified cleanup | Receipt SHA-256 |
| --- | --- | --- | --- |
| macos-f2-r1 / a4 | 275.29 s | 335.91 s | `e4b12f0bbb537eab066e00de6b0250528440c395d8a04c1863f5a397ec5fbbaf` |
| macos-f2-r2 / a3 | 195.30 s | 247.71 s | `6485ba9ee89a1507c67c19d2e1768b779f1b04ad22390a9da56eb2eaa6b89fcf` |
| macos-f2-r3 / a3 | 505.86 s | 516.03 s | `06a9b4c8ef2408d289427e2c869a0f2aab24a49b600067114400f30aad4a9d6a` |

All observed model and verifier Tool Processes exit, both owning native PIDs
actually exit, and the pane returns to its original shell within each unchanged
600-second bound. Actual 80x24 PTY/Host captures retain successful Action details,
Space paging, scrollback and logical draft/cursor restoration (`abcXYZ` -> details
-> Esc -> insert delta -> `abcδXYZ`). Failed ordinary diagnostics remain in Action
files; these F2 runs do not claim that a failed Action detail was opened. No
operator source edits, corrective model advice, task restart or unintended
verifier generation occurs. One model-project grant, one setup approval and three
checker grants are planned per run. Exact cursor coordinates, provider usage/cost,
time partitions and general macOS Host read isolation remain unmeasured.

Each of these F2 runs also attempts `invoke_delegated_skill`; launch rejects cwd
`/mnt/project-request-1` as outside the child's explicit delegated namespace. The
model falls back directly without operator steering. This failure is a virtual
Tool result, not any ordinary aN Action or successful child Process. A supplemental
raw-rollout audit confirms the same rejection in all three F1 refreshes, preserving
their immutable receipts. Its missing file-backed failure publication is now a
reproduced UI finding: legacy Tool events and the model's explanation alone do
not provide the terminal's independent notice/history. Preserve the authority
refusal; the owning rust-inline-tui delta covers the narrow prelaunch publication
repair, original Tool result and parent continuation. New production source
requires affected qualification; these six receipts do not qualify that repair.

Supplemental `delegation-history-audit-v1.json` SHA-256 is
`a44ba8d1048423ebb8de522d1e72ecd6ba26551200640293f44ee469e1219094`.
Aggregate `first-outcomes-v38.json` SHA-256 is
`567a43455d4b859ee25f95bddad4469fd565b83fc9a695e6f1255978a96a3ef9`:
forty development attempts, fifteen historical qualified slots and six applicable
slots on `465e3e47`, with original eleven first PASS/four FAIL/fifteen NOT_RUN
unchanged. Nine macOS refreshes and all fifteen Linux tasks remain; task 4.2 also
requires qualification of the prelaunch UI repair. Checklist remains 8/19.
The preceding documentation-only head `34fd4910` passes its normal quality and
all sixteen current-head checks; no compiled input changes in that commit.
Its commit and CI receipts are respectively
`c51aa64fc1bfc5e66cdcc3ac8655666aecbf622fb891afd4ce1d81a0df040a43` and
`442897f18725811dd427db733d56f22d9f17be1e004e67e0e70040431398087e`.
That CI does not qualify the later prelaunch repair or close Linux readiness.


## Delegated-skill prelaunch publication repair — 2026-10-11

The file-backed regression first fails on the old implementation: an invalid
invocation produces its original Tool result and recoverable legacy Error, but
AgentFS notice remains None. The revised five-case regression covers actual
launch refusal text, invalid input, unavailable delegation, target mismatch and
unwritable presentation files. Existing AgentFS notice/error publication is
reused after the original result is recorded. A best-effort publication failure
is diagnosed without replacing that result, changing continuation or retrying
child launch. The successful-launch regression keeps an empty notice. A terminal
regression checks that a retained delegated-operation error does not override the
parent's correlated Completed receipt or its independently verified answer.
No child namespace, grant, Tool policy, parent generation, Action or Process
identity is added or changed by this repair. Launched-child wait/cancel/exit paths
remain unchanged. All six previously refreshed real tasks attempted rejected
launches, so their receipt scope is historical for this new UI behavior and each
requires affected fresh acceptance after the new candidate is frozen.

Initial full engine/fixture checks fail for native disk exhaustion (LLVM IO failure
and linker errno28); they are environment failures, not passing checks. The
existing managed quality output belongs to this exact worktree and quality
purpose. With its exclusive output lock held and no live build, only its two
reconstructible incremental subdirectories are removed. Frozen CLI/observer,
receipts, original failed checks and all source/Git state remain. Source porcelain
is unchanged across that cleanup; subsequent checks must run normally.


After exact cache cleanup, full all-feature engine tests pass 1428 unit tests and
20 integration tests, with one existing ignored live-provider test. The fixture
self-check retry passes all six; strict change validation passes. The complete
terminal suite passes 382 unit and 12 integration tests, including recovered
delegation error versus correlated parent completion. Normal commit quality/
distribution, new-head CI and fresh native repair acceptance are recorded
separately when terminal, never inferred from these passing regression gates. Initial environment failures remain in
`target/delegated-prelaunch-engine-tests.log` and the retained qualification notes.


## Native F3 prelaunch publication and duplicate footer finding — 2026-10-11

Normal commit quality/distribution and all sixteen checks pass on `bdf14ab0`.
Its frozen native CLI is `8307682a`, with 437 compiled inputs matching committed
source; only the two delegated-skill runtime inputs differ from `465e3e47`.
The original F3 r1 task runs through the native Herdr pane with gpt-6.1-sol medium,
actual compiler RED, model-authored correction only in src/lib.rs and Cargo GREEN.
A rejected delegated cwd now independently publishes the actual reason through
AgentFS notice and one retained Error event. The parent completes its direct
fallback; rejection remains authority refusal, not successful child support.

Separate native verification passes the original protected_behavior assertions.
Original tests/config/project boundaries and protected assertion/driver inventories
remain unchanged. Both model and verifier Tool Processes and native owners exit;
behavior verification takes 279.89 s and submission through verified cleanup
363.69 s, within the original 600-second bound. Detail paging, scrollback and
logical draft/cursor restoration remain observed. However, a settled parent
Completed still shows the same error twice: retained history and persistent live
footer. Required UI qualification fails, so this attempt is retained as FAIL and
adds no qualified slot. Model boot is `008795bd-7ca4-4dac-94d7-2413460197f6`,
native PID 50772, Root 8, submission `350b0092-b52e-4474-b997-0e2d3e0edf8e`.
Receipt attempts/macos-f3-r1/a3/receipt.json SHA-256 is
`497573d4f1f7cf33d3d7080691ecc1c4e6bf4224c49f7f381a428687728303e9`.
Aggregate first-outcomes-v40.json SHA-256 is
`8dce2d6de301f928fc034c0c932771b3e5529dff5580b3ed7d1432f9d4bda999`:
41 model attempts, original 11 first PASS/4 FAIL/15 NOT_RUN, 15 historical qualified
slots, zero qualified slots on bdf14ab0. No old outcome or denominator changes.

Collector limits remain explicit: raw UI JSONL was saved before the single-JSON
decoder rejected multiple events; the observer/model was not restarted, and
line-by-line inspection confirms the real event pair and Completed receipt.
Current definition/memory metadata was collected after submission, although
prospective config/compiler identities were frozen. No memory bodies were read or
cleared, and no statistical independence or complete prospective metadata proof
is inferred. There was no model advice or operator source edit.

The shared TUI Notice value now distinguishes runtime-owned from local notices.
A matching runtime Error notice retires only when its retained Error is handled;
identical local text, runtime Warning and unrelated runtime Error survive. Error
history, parent completion, draft/cursor and underlying AgentFS evidence remain
unchanged. The focused regression first reproduces the duplicate on old code;
its initial test-import compile error is retained separately, not called RED
behavior evidence. The corrected regression fails its retirement assertion before
the production repair. Full TUI tests after the repair pass 384 unit and 12
integration tests, including actual file-backed hydration of Notice/Error/Completed.
This is regression evidence; fresh native acceptance, normal gates and current-head
CI for the additional TUI repair remain separate delivery requirements.


## Runtime notice retirement and audited F3 refresh — 2026-10-11

Production repair `573cd12eb0ddec1e1b19d2fbfb492fe8c3d729e1` passes normal commit
quality/standalone distribution, full TUI 384 unit plus 12 integration tests,
strict OpenSpec 70/70 and all sixteen current-head checks. Frozen CLI SHA-256 is
`950e1ce5338b60f73e07131bd233c9193da0aa01221acf05f8902138e92b1497`.
All 437 compiled inputs match the clean committed source; only app/history.rs
and app/notice.rs differ from bdf14ab0. The existing private Notice records
runtime ownership; a matching runtime Error retires when its Error is retained.
Local identical text, runtime Warning and unrelated runtime Error remain. There
is no protocol/config/dependency change or lifecycle/authority mutation.
Freeze receipt SHA-256 is
`8cf1873907184399e959955ff7071777fe80888c36020aa2e1a661f531b776eb`;
retained gates/CI receipt is
`283d952e4394b54b55461e722a62e5fd571a5bc951155c479325293619326191`.

An initial F3 r1 a4 receipt claimed PASS after correct model source, independent
protected behavior, one retained Error/no duplicate live footer and draft/caret
checks. Later artifact audit finds its detail-open/page frames captured before
the terminal redraw; delivering controls is insufficient evidence of retained
details. Keep the immutable primary receipt ad41407a and append the qualification
audit rather than rewriting it. Aggregate v42 withholds its qualification and
requires a fresh unchanged-task retry. Audit SHA-256 is
`bd2b0f66022e21a076f6ef9e711e3a66302bac1d512ce062c98b6880c39cccef`.
The earlier read-only missing-path observations, rejected mount access spelling
before Host mutation and draft observation lag remain disclosed. These are
collector/procedure limitations, not an erased model outcome or invented UI pass.

All three F3 tasks now qualify on the same frozen repair. r1 uses a fresh v2
fixture with the original missing BTreeSet import; r2 retains the missing module
file/compiler diagnosis and corrects only lib's module declaration/reference;
r3 fixes the actual Result-versus-i32 error with unwrap_or(0). Model edits only
src/lib.rs, preserving original tests, operations.rs where present, Cargo files
and fixture data. Independent fresh native verifiers pass the original protected
assertions from read-only project/assertion grants and private writable checker
scratch. Source/assertion/driver inventories remain unchanged by verification.

| Slot / attempt | Behavior verification | Submission through verified cleanup | Receipt SHA-256 |
| --- | --- | --- | --- |
| macos-f3-r1 / a5 | 187.59 s | 229.57 s | `9113f5199e59eb9146b0d5d1834f1278028e44ac9fdeb51b8061ca2627ffdd2b` |
| macos-f3-r2 / a3 | 217.81 s | 259.02 s | `d29cf3a0a2316e63d11436a7ebe4715b1574b7e3a2b0efcd6b172f83a3763447` |
| macos-f3-r3 / a3 | 273.97 s | 333.27 s | `b2ef5a920cc81c6a01e91497aa4bcecd404285eb40205b546fd54fac94d30466` |

Each attempt has actual gpt-6.1-sol medium admission/generation, a prospective
selected-tool/config/definition/memory exposure record, actual 80x24 PTY/Host,
real RED and model-authored GREEN. Every rejected virtual delegated launch now
has one retained UI Error and no duplicate live Error after its parent's correlated
Completed. None is successful child qualification. All discovered Tool Processes
and both native owners exit within the unchanged 600-second bound. Actual retained
Action detail frames are captured; r1/r3 additionally observe the modal before
Space and capture a changed page before Esc. Returning to the middle of abcXYZ
and inserting delta produces abcδXYZ. Host scrollback remains captured. No
corrective model advice, operator solution edit, task restart or unintended
verifier generation occurs; one model grant, one version-check setup approval and
three verifier grants are planned per run. r2's prelaunch fixture parent-directory
preparation failure is retained separately, without adding a model attempt.

Fresh v1/v2 fixture inputs preserve all thirty original prompts/follow-ups,
baseline source bytes, protected assertions, allowed effects, 80x24 geometry,
600-second bounds and long-diff bytes; only operator backing paths/Git setup
identity differ. Memory bodies are not collected or cleared, and statistical
independence, exact cursor coordinates, general macOS read isolation, provider
usage/cost and model/Tool/wait time partitions are not claimed. Recorded total
wall time includes operator observation and verification, not only product latency.

Aggregate first-outcomes-v45.json SHA-256 is
`d4fd2c7c5b04da26e059a7f47f3cc635f490e30298b5df91b22b67f5b979750c`:
45 model attempts, 15 historical qualified slots, three applicable F3 slots on
573cd12e, original 11 first PASS/4 FAIL/15 NOT_RUN unchanged. The superseding audit
and all original receipts remain linked. Twelve macOS refreshes and fifteen Linux
real-model tasks remain; checklist stays 8/19. This documentation-only follow-up
needs its own normal commit gates and current-head CI, with 437-input applicability
rechecked rather than rerunning native tasks merely for documentation changes.

## Notice-source F1 r1 fresh small repair — 2026-10-11

Head `ed6a65e6fd8f21cd88daf8815b80b15e619199b1` passes all sixteen checks;
its 437 compiled inputs and freshly gated CLI remain byte-identical to 573cd12e.
Retained CI receipt SHA-256 is
`4cfeda42d2615630ac2d509aaac39dd7776e1e8177dbfec598c96923d4821d33`.
F1 r1 a6 uses the original addition fault/prompt in a fresh disposable baseline.
Actual gpt-6.1-sol medium generation first records assertion RED (5 versus 9),
changes only src/lib.rs from subtraction to addition and passes the original
contract. A separate native verifier, with project/assertions read-only and
fresh writable scratch, passes four protected addition cases. All project,
protected assertion and driver inventories remain unchanged by verification.

Model boot `95971c9b-5868-4ae8-a819-820adfeaebc9`, native PID 74543, Root 8,
submission `723dbf9d-6cbc-421d-9330-4e49f5e6efb9` and all actual Tool result
Processes are correlated. A6's retained original stdout/stderr detail is observed
before paging, then a changed page is captured before closing. The draft returns
to its middle insertion position (abcXYZ to abcδXYZ), and host scrollback remains
available. Virtual delegation is refused without child authority; its Error
appears once in history without a duplicate live footer. Direct parent fallback
actually completes. The non-Git command failure remains retained. Both native
owners and all discovered Tools exit; the pane returns to its original fish.
There is no corrective model advice, operator solution edit, restart or verifier
generation. Prospective config/tool/definition/memory metadata is captured, with
zero context items and no personal memory-body collection or clearing.

Collector corrections remain explicit: Process status has a trailing newline;
Tool result/Process is top-level in the rollout; the content Action title scrolls
while the modal header remains fixed. One early missing Action read is retained.
The actual saved frames and lifecycle files prove completion; no controls or task
are replayed because an assertion or observation failed. Behavior verification
takes 151.39 seconds and submission through verified cleanup 165.53 seconds,
including operator observation within the unchanged 600-second bound. Usage/cost,
model/Tool/wait partitions, exact cursor coordinates, general macOS read isolation
and statistical independence are not claimed.

Immutable receipt SHA-256 is
`b1cf57f6b1dc189b5682876d1eeb2295dd5e6550aa09efb3c3405d10bc5da3bf`.
Aggregate first-outcomes-v47.json SHA-256 is
`9e68d65479b5d69a0dea1666f86053b71db7ca0391a6710d94aaea643cbac40c`:
46 model attempts, 15 historical and four final-source qualified slots;
11 first PASS/4 FAIL/15 NOT_RUN remain unchanged. Eleven macOS refreshes and
fifteen Linux tasks remain. This evidence-only follow-up requires its own normal
gates, CI and committed-input applicability proof; checklist remains 8/19.

## Notice-source F1 r2/r3 family refresh — 2026-10-11

Both fresh tasks use documentation head 95d6664c, the same frozen CLI and all
437 unchanged compilation inputs. Actual gpt-6.1-sol medium admission/generation
is correlated to each original prompt. r2 retains assertion RED (6 versus 5),
removes the extra increment from absolute difference, then passes the original
contract. r3 retains assertion RED (-5 versus 5), repairs the inclusive clamp,
then passes the original contract. Each model changes only src/lib.rs. Separate
native verifiers pass the original protected behavior from read-only project and
assertion grants, compiling into previously absent writable checker targets.
Source/assertion/driver inventories remain unchanged by verification.

| Slot / attempt | Model submission | Behavior verification | Through verified cleanup | Receipt SHA-256 |
| --- | --- | --- | --- | --- |
| macos-f1-r2 / a4 | bc9e15e5-571d-4ac2-ac1b-bdb5366c4390 | 116.07 s | 144.88 s | `babcb125f6638091160f570be9ac5e13f28327c707ffa70416cde1df0c0df349` |
| macos-f1-r3 / a4 | bb148480-2ea0-4a51-90e8-bd2eeec29d5f | 111.06 s | 135.04 s | `c1df2cea20215bef49c4e2d0e7248defc0f23c731d01b32f08e9fb8f3fb07ab2` |

Both runs capture actual original Action a5 details before paging and a changed
stdout/stderr page before closing. Returning to the middle of abcXYZ and inserting
delta produces abcδXYZ; host scrollback is retained. Delegation refusal is one
retained Error with no duplicate live footer, followed by actual Completed parent
fallback. Neither is successful child qualification. Non-Git failures remain
retained. Every discovered Tool Process and both native owners exit; the pane
returns to original fish. Planned grants/setup approval are recorded. There is
no operator solution edit, corrective model advice, restart or verifier generation.
Prospective tool/config/definition/memory metadata is captured; context items
are empty and no personal memory bodies are collected or cleared. Existing limits
on usage/cost, time partitions, exact cursor coordinates, general macOS read
isolation and statistical independence remain. Wall times include observation.

All forty-eight linked receipt identities and both new artifact inventories pass
hash audit. Aggregate first-outcomes-v50.json SHA-256 is
`79c15a42768306a62c47fe61a23f5bb3ccf857294c7fe1ee07a09d043aaa6ea1`:
48 attempts, 15 historical and six final-source qualified slots (all macOS F1/F3);
11 first PASS/4 FAIL/15 NOT_RUN remain unchanged. Nine macOS refreshes and fifteen
Linux tasks remain. At this observation 95d6664c has fourteen successful checks
and Rust CodeQL running, with no failed check. This evidence-only follow-up needs
normal commit gates, CI and committed-input applicability proof; checklist is 8/19.

## Notice-source F2 refresh and current-head gates — 2026-10-11

All three frozen cross-file tasks now qualify on documentation head `d97d7f70`
with the unchanged `573cd12e` production inputs and frozen native CLI
`950e1ce5338b60f73e07131bd233c9193da0aa01221acf05f8902138e92b1497`.
The actual admitted binding and generation context confirm `chatgpt-main`,
`gpt-6.1-sol`, medium effort and an empty context-item list. Exact native PATH,
selected Rust/Cargo 1.93.0 and Git 2.54.0 identities were frozen before generation
and checked through the actual Tool entry. Memory remains enabled; empty context
items do not prove statistical independence.

| Slot / attempt | Model boot | Independent binary stdout | Submission through behavior check | Submission through verified cleanup | Receipt SHA-256 |
| --- | --- | --- | --- | --- | --- |
| macos-f2-r1 / a5 | bb1e74b8-21ab-4104-82da-87b77f00ed50 | 20, -1 | 172.636043 s | 213.396169 s | `9855d329e65af8c2541695c84f73f7e8bedf35192676b97e221cdf03aa52b417` |
| macos-f2-r2 / a4 | 044376ae-5a65-463f-9b05-78a19d6f5a3b | 51, -20 | 251.453896 s | 308.698140 s | `b143cd83d2dfb17fbf3d25040d108977bb38fec2898bb83ae0c90e6964c1fe50` |
| macos-f2-r3 / a4 | 4aeff782-c75b-4235-8589-08c664cdd2ff | 3, -5 | 423.277697 s | 440.607847 s | `8974f584184f7105059f334a3c8735e86916bed5ab7ffd38c649225665065867` |

Receipts are `dev-boundary-20261011-v1/attempts/<slot>/<attempt>/receipt.json`.
Each real model changes exactly `src/lib.rs`, `src/main.rs` and
`src/bin/report.rs`, implementing addition, multiplication or minimum respectively.
Model library tests truthfully report zero tests; qualification instead requires
separate protected library assertions and execution of both actual binaries.
The original full independent command retains offline/locked flags and both
read-only manifest executions from fresh writable checker scratch. Project,
protected assertion and driver inventories remain unchanged by checking.

Actual original Action details and a changed page are captured before close:
r1/r3 a7 at 8/8; r2 a6 at 7/7. Returning to the middle of `abcXYZ` and inserting
`δ` yields `abcδXYZ`; readable stdout/stderr and Host scrollback remain available.
The delegated prelaunch rejection is retained once in history before any new
input, with no duplicate live Error and actual parent Completed/idle evidence.
This proves direct parent fallback, not successful delegated execution. r3's
failed non-Git inspection is retained; r2 has no failed ordinary Action and its
receipt explicitly avoids inventing one for the virtual rejection. Actual Tool
Processes exit before model exit, and each model and independent-verifier native
PID is gone with the original fish shell foreground after `/quit`. Verifiers
have zero model turns. No source edits, corrective model advice or model-task
restart were supplied by the operator. Planned setup approval, explicit grants
and UI/lifecycle controls remain interventions rather than approval-free autonomy.
Read-only collector reference/filename errors in r1/r3 are retained and corrected
without repeating the model task or changing its source.

These wall times include observation, UI/lifecycle checks and operator waits;
r2 also includes independent Linux environment work. They are not pure model or
product latency. Usage, cost, time partitions, physical cursor coordinates and
general macOS Host-read isolation remain unqualified or unknown.
Immutable aggregate v54 SHA-256
`fd3fffc5ca9a946b0ae09eb171c317a4fcc71a7d8e7cc864ab425ab3fbf12c21`
retains fifty-one development attempts, fifteen historically qualified slots and
nine final-source slots, preserving eleven first PASS, four first FAIL and fifteen
NOT_RUN. Its preceding v53 and every earlier receipt remain retained. Six macOS
F4/F5 refreshes and all fifteen Linux tasks remain; checklist stays 8/19.

All sixteen checks pass on both `95d6664c` and `d97d7f70`, including Rust CodeQL.
The latter immutable gates receipt is `gates-d97-v1/receipt.json`, SHA-256
`a291c88021c570d343141fe58b071d3d9818ff5e009a674ff5ca4af0f0f1c74c`.
The 437-input/CLI equality proof is
`notice-f1-family-source-applicability-v1.json`, SHA-256
`52910a4e73afb2b717fafd79cfb1ef35ce524c8054c1b57ba151ae7612e8f197`.
Transient GitHub reads were reobserved at the same head/job rather than restarting
checks. New evidence-only commits require their own normal gates and CI.

## Fresh Linux candidate, missing dependency and exact cleanup — 2026-10-11

The task-owned Ubuntu guest receives only the public Git archive of `d97d7f70`,
public tree `bde81b765b8a94bf0ce703e84c55b96155456770`, under
`/home/morris/.cache/Alan/qdev-notice-candidate-20261011-v1`. Original Git metadata,
credentials, private Stores and tracked `.env`/`.env.local` are not transferred.
A fresh snapshot Git identity verifies that same tree and clean source state;
it is not the repository's original commit identity.

The first locked offline build fails because cached `anstream v1.0.0` is absent;
its original exit/log record stays retained. A locked retry permits registry
downloads on the unchanged source and lockfile. Native build succeeds in
54.119429 seconds with dev debug information disabled and incremental compilation
disabled. The frozen Linux CLI SHA-256 is
`4fe74466033731061a798a16bfadba72d61ce014fbe40d8aef637f1e36255fdb`.
All 440 actual native workspace inputs match committed public source; this Linux
input map is distinct from the 437-input macOS map. Build-only receipt
`linux-candidate-notice-v1/receipt.json` SHA-256 is
`b75c37bbbefe3deca387a2eef0339edf6ea6b6995ba97c79a20b220ac711307e`.
It claims neither full default-profile quality nor native PTY/model qualification.
Actual CLI connection inventory says `No connection profiles configured.`
All fifteen Linux slots remain NOT_RUN and task 1.4 remains open.

Initial exact build-output cleanup conservatively refuses because `lsof` is
missing. Its original sealed receipt is not rewritten. After a simulated package
transaction, install only `lsof` and `liblsof0`, both `4.99.4+dfsg-2build2`, with
no recommended packages. Before/after package inventories prove exactly those
two additions, zero changed existing packages and zero removals; `lsof -v` passes.
Setup receipt `linux-candidate-notice-v1/lsof-setup/receipt.json` SHA-256 is
`f10a2bdca840da172efb2af868a5e94882025bcec3267879245ebb24489aeb0f`.
No Node/Python installation or user toolchain-default change is implied.

Reusing the existing build-artifact helper, inspect registrations, completed
build lease, Cargo-only ownership, locks and live handles, then clean only
`source/target/notice-cli`. Its preview is would-clean, actual state cleaned and
that exact target is absent. The frozen CLI hash, clean source, build log and
original evidence remain intact. Global service scratch and other targets are
not touched. Supplemental cleanup receipt SHA-256 is
`737a910ef9b9aaf0f6a6cf85861cdf42c4f774bf7edb329f15fc1b4500fd70c8`.
Environment setup and cleanup add no model attempts or qualified slots.

## Notice-source lifecycle refresh and collector-ordering failure — 2026-10-11

All three F4 repetitions now qualify on unchanged documentation head 19665cd9,
with actual chatgpt-main / gpt-6.1-sol / medium, Rust/Cargo 1.93.0, Apple Git 2.54.0,
80x24 Herdr and the original 600-second bound. CLI SHA-256 remains 950e1ce5;
437 macOS and 440 Linux compiled inputs match. Prompts/follow-ups, protected
assertions and allowed effects are unchanged. r1 a5, failed r2 a3 and r3 a3 use
fresh v1 fixtures; r2 a4 uses equivalent fresh v2 after its failed predecessor.
Actual Tool audit is Seatbelt, without claiming general macOS Host-read isolation.

| Slot / attempt | Outcome | Independent verification elapsed seconds | Submission through verified cleanup seconds | Receipt SHA-256 |
| --- | --- | --- | --- | --- |
| macos-f4-r1 / a5 | PASS | 368.427215 | 401.000461 | `f9fb329aa6b681532b953722e479a527ebe47de56408ce2e06f93e67a8cb1c3e` |
| macos-f4-r2 / a3 | FAIL | not completed | 423.193024 | `2fcab4ee0a484b86b5b01cc93d790a4e2dd5719a67778c7ebc46e50e4fc5d625` |
| macos-f4-r2 / a4 | PASS | 361.298667 | 415.959199 | `8a6942771cc97e9c914a0c4708eef508c5a9660580a624dfbf5bd5e32dba9682` |
| macos-f4-r3 / a3 | PASS | 244.345375 | 254.868359 | `e7f8c766adf068bec8a361469d8ec0b9bacd6333a16f4ff32b3afc2768321d0b` |

Receipts are `dev-boundary-20261011-v1/attempts/<slot>/<attempt>/receipt.json`.
Each PASS observes a live delayed writer, cancels before the 40-second deadline,
confirms its native group gone, revokes the grant and retains the subsequent
actual read denial. Past the deadline, late.txt is absent and the ledger has one
completed entry. Explicit recovery checks the selected own rollout filename/hash
before `--resume`; the new boot retains history but no old grants. After explicit
reapproval and the frozen follow-up, the model changes only src/lib.rs and runs
only `cargo test --locked --offline --test contract`. Lifecycle file hashes stay
unchanged; start_work does not replay. New-command/effect analysis excludes copied
recovery history at the captured pre-regrant rollout row boundary.

r2 a4 and r3 a3 observe the unique Root child Kernel Process running before cancel,
then exited with exit 130. r1 a5 lacks the individual canceled Tool status and
retains native writer/group termination plus actual initial-instance exit instead.
r3 individually observes initial setup Tool 9 and resumed Tools 9/10/11; initial
completed Tools 10/12 are covered by actual instance exit, without inventing
individual Kernel-status observations.

Separate native checkers use read-only project/protected grants and fresh writable
scratch. Their prospectively frozen offline/locked command verifies protected
behavior and unchanged project/assertion/driver inventories, with zero model
turns. Each PASS captures actual Action a8 original output, 9/9 detail selection
and a changed page, then returns to the middle of abcXYZ and inserts δ to obtain
abcδXYZ. Host scrollback is retained; physical cursor coordinates are not measured.
All initial/resumed/verifier native PIDs exit after `/quit` and the original fish
shell returns to foreground. No operator solution/source edit, corrective model
advice or model-task restart is supplied. Planned grants/setup approval,
cancel/revoke/continue/recovery and model-requested confirmation remain interventions.

r2 a3 remains FAIL: Herdr delivery is mistaken for native input acknowledgment,
so `/continue` appends to the retained forced-Agent draft. Clearing its text keeps
that route, making a subsequent `/discard` literal Agent content too. The planned
follow-up and two unintended paused inputs are retained and all three discarded;
the unintended inputs execute no model turn. Cancellation/Kernel exit 130,
revocation and no late effect pass, but denied-read completion, recovery and
independent checking remain incomplete. Fresh r2 a4 qualifies without unintended
input or collector correction. Existing delayed-ack/opposite-route and forced-Agent
slash regressions both pass; production routing remains unchanged. See
execution-map.md for corrected sequencing, including actual echo/queue/empty draft
before following controls and same-submission reobservation instead of resending.

Read-only collector corrections remain explicit: r1 retains its pre-model socket
path refusal and three observation/schema corrections. r3 retains five corrections
for revocation wording, compact grant JSON, control members in Action listings,
empty-inventory markers and retained context count. r3 has three actual admission
bindings and two available nonduplicated turn contexts, all observed as
gpt-6.1-sol/medium; no missing context is fabricated. Memory remains enabled and
system prompts may expose continuity; empty context_items is not absence of memory
or statistical independence. Usage/cost/time partitions remain unknown; wall times
include observation/operator waits and are not pure product/model latency.

Immutable aggregate v59 SHA-256
`33b0d363962cc3193dc59c25364b198bf3f15a916584d10e6c240f7d073c23fd`
retains 55 attempts, 15 historical slots and 12 applicable final-source slots.
v56/v57/v58 and original failures remain retained. Rehashing all 55 receipts and
3727 retained artifacts passes with all thirty original slots unchanged. Three
macOS F5 refreshes and fifteen Linux model tasks remain; checklist stays 8/19.
This bounded sample does not establish unattended self-bootstrap.

All sixteen checks pass on 19665cd9 including CodeQL. Immutable gates receipt
`gates-196-v1/receipt.json` SHA-256 is
`05a966b914aec35f6526c388a10209f5c5a78b98dce4c853f0ee21de39154212`.
The preceding source/CLI equality proof is
`notice-f2-family-source-applicability-v1.json`, SHA-256
`67775b017e01c28c169ddc8c78908337307de830bf79d12cb4b885b5ae4fc5fe`.
These new evidence-only edits require their own normal gates/source equality and
current-head CI. #1045/#1046/#1047 user merge and canonical closure remain open;
no automatic merge/routing or production Alan self-development occurs.

## Final notice-source long stdout/stderr/diff repetitions — 2026-10-11

All three F5 refreshes qualify on byte-identical documentation head 81b6f361,
using the frozen CLI 950e1ce5, actual chatgpt-main / gpt-6.1-sol / medium,
Rust/Cargo 1.93.0, Apple Git 2.54.0, Seatbelt, 80x24 Herdr and the original
600-second bounds. Fresh v1 fixture hashes/prompts/assertions/allowed effects
match the original matrix. Original failures and the historical separate
unintended verifier model turn remain retained.

| Slot / attempt | Outcome | Independent behavior verification seconds | Submission through verified cleanup seconds | Receipt SHA-256 |
| --- | --- | --- | --- | --- |
| macos-f5-r1 / a4 | PASS | 253.368337 | 299.232744 | `3378b40b735219b32a72004a0e9bdd3f933165a8758a95624cb480e63c4797d8` |
| macos-f5-r2 / a2 | PASS | 221.939835 | 269.991757 | `eebdb1d78a1c5cd953cfe49bc6b767fb806fb9578b32333131f233c0ee85ccd6` |
| macos-f5-r3 / a2 | PASS | 201.037226 | 238.028999 | `700940388bfb5151c4b3af96fe5ea409de5c26cd86bfe214c462638ab727fe72` |

Receipts are `dev-boundary-20261011-v1/attempts/<slot>/<attempt>/receipt.json`.
Each producer executes exactly once in Action a2. The model reads its original
AgentFS output through bounded ReadFile byte ranges, finds the deep value and
changes only src/lib.rs. r1 uses a3/a5 reads and finds QVALUE=42137; r2 uses a3
and finds QVALUE=42274 in stderr; r3 uses a3/a5 and finds +QVALUE=42411 in the
original diff. Raw Action output, successful reader status/result and actual
reader content correlate the values, rather than relying on the model's answer.
The model also inspects fixture/source files, but this does not substitute for
its positively observed retained-result access. r3's original 138113-byte diff
has the frozen SHA-256 before generation, as produced by a2 and after checking.
The pre-existing data/long.txt modification is Host setup, not model authorship.

Actual native details select the original producer: 3/10 in r1 and 3/8 in r2/r3.
Separate stdout/stderr labels and readable content are captured before paging;
changed-page and deep-value frames show the exact QVALUE and adjacent rows.
r3 preserves minus/plus diff markers. Literal server> ready and a > b remain
content, not generated roles. Closing details restores insertion in abcXYZ;
adding δ yields abcδXYZ, and Host scrollback remains available. Physical cursor
coordinates are not measured. Delegated prelaunch rejection is retained once in
history with no duplicate live Error after the parent completes; this proves
direct fallback, not successful child delegation or inherited dynamic authority.

The model's ordinary Cargo tests have zero active behavioral assertions; r1/r3
explicitly report compilation-only checking, and r2 retains its ignored output
test. r2's non-Git inspection diagnostic is also retained in the successful
compound command and its final answer. Qualification requires the separate
protected behavioral check in every case. Native independent checkers use
read-only project/protected grants and fresh writable scratch, run the original
prospectively frozen offline/locked command, and execute zero model turns.
Project, immutable assertions and checker-driver inventories remain unchanged.
All observed model Tool Processes and checker Tools exit; both native instances
exit after `/quit` and the original fish shell is foreground.

No operator source edits, corrective model advice, model task restarts or new
unintended verifier generation occur. Planned project/checker grants, setup
approval and UI controls stay explicit. r1 retains two read-only collection
corrections: Action read output/result shape distinction and a syntax typo in its
diagnostic-record writer, which executed no mutation. r2/r3 have no collector
correction. Memory remains enabled; empty context_items does not exclude prompt
continuity or prove statistical independence. Usage/cost/time partitions and
general macOS Host-read isolation remain unknown or unqualified.

Immutable aggregate v63 SHA-256
`032d906ba07f9bdf18b79b4a261934fd7ab778f8631fc51977a345367deb537b`
retains 58 attempts and 15/30 applicable final-source slots, all macOS. Original
outcomes stay 11 PASS / 4 FAIL / 15 NOT_RUN. All 58 receipts and 4306 retained
artifact hashes verify with the original thirty slots unchanged. v61/v62 and
all earlier attempts remain retained. Checklist stays 8/19; full cross-Host
qualification, merged-main applicability and user delivery remain open.

All sixteen checks pass on 81b6f361, including CodeQL. Gates receipt
`gates-81b-v1/receipt.json` SHA-256 is
`1639efda7c56494be69a97c8312121b43e15cda5bac898b0fdd6cf2ec7066f37`.
The 437 macOS / 440 Linux input and freshly gated Mac CLI equality proof is
`notice-f4-family-source-applicability-v1.json`, SHA-256
`1efc1ca9d8ffd9cab8f3ee14632eaa67a645c4120bc6c621365e691a0ff74795`.
These evidence-only edits need their own normal gates, equality proof and CI.

Fresh Linux CLI reobservation still reports No connection profiles configured
and effective_profile none; its actual native binary hash remains 4fe74466.
Receipt `linux-generation-readiness-recheck-v1.json` SHA-256 is
`7080b32bb647ad5f12c910444f654a98dc24e9de9c02b638ce92e6575677942a`.
All fifteen Linux slots remain NOT_RUN. No private Store/credential is copied,
no substitute model is selected, and no preflight or CI test replaces a slot.
The independent-generation setup question remains pending. #1045/#1046/#1047
user merge/canonical closure stay open; no automatic routing or unattended
self-bootstrap claim follows from the fifteen macOS repetitions.

## Linux readiness and user-authorized auth reuse — 2026-10-12

Current head 092e3278 passes all sixteen distinct checks. Immutable final gates
receipt `gates-092-v1/receipt.json` has SHA-256
`b28280f410ac9ee9d0e13274308890f5b7881bdfe5416551edf0c3d6182b7486`.
The production source and frozen CLI remain unchanged. Actual Linux explicit
commands run through the fresh normal CLI in an ordinary 80x24 PTY, selecting
Git 2.53.0 and Rust/Cargo 1.96.0 under linux_reified_namespace. One planned Bash
approval precedes the compound version/Git command. Cargo actually compiles the
read-only dependency and protected checker in writable scratch; the protected
behavioral test passes and source/checker writes are denied. All fixture hashes
remain unchanged. Action details show readable original output; returning from
details preserves abcδXYZ and insertion produces abcδQXYZ. Both observed Tool
Processes and the native instance exit. Two read-only collector corrections
(Host readiness field and Action owner path) remain retained. This probe does not
measure physical cursor coordinates or qualify Host scrollback. Receipt SHA-256
is `755349a7806693df2d038a7043f8104b6b471542b02fed8147bfe19fa0fff6c4`;
all 51 retained evidence files verify. It adds zero model tasks or slots.

The dedicated qualification-gpt61 profile selects gpt-6.1-sol with medium in its
explicit config. The first-profile automatic default is cleared, restoring the
original unset default. Credential-reference metadata alone is not a login.
Setup receipt SHA-256 is
`bbc94803c212f3354a0bd1d4daf7c8ea34aca75b7573f1db0eba26adb577a3c6`.
The first device-login process times out; a user-requested retry is later stopped
before the user-authorized migration. Neither failure is reclassified as success.

The user then explicitly authorizes copying the existing macOS Alan ChatGPT
managed-auth file to the Ubuntu Host Store. The selected regular auth file uses
the same portable schema on both Hosts; direct process-stdin transfer creates a
new 0600 destination without overwriting existing auth and leaves the macOS
source unchanged. Credential payloads are absent from logs, repository files
and qualification receipts. Private Store trees are not cloned. This is shared
authentication and environment setup, not independent credential acceptance.
The narrow authorization supersedes the earlier no-copy operator constraint;
all project grants, model selection and task success requirements stay fixed.

CLI connection validation passes, followed by one actual redirected-input
generation readiness probe with no project grants or Tool calls. The response
is exactly ALAN_LINUX_AUTH_OK and the instance exits after 13.282594 seconds.
Its sole admitted callable is ChatGPT / qualification-gpt61 / gpt-6.1-sol with
medium request controls; the actual turn context also records gpt-6.1-sol/medium.
The owned rollout remains in the System Store and only its reference/hash are
collected. Readiness receipt SHA-256 is
`5c4f0f98c3bb2f9ee9c25f84eb507a4f9709ee482a7590b96729adf57f779e09`.
This adds one readiness generation turn and zero development tasks or slots.

The unchanged fixture module also prepares native Linux inputs. All fifteen
Linux prompts/follow-ups, project baseline bytes, protected assertions, allowed
changes, original long-diff bytes, 80x24 geometry and 600-second bounds match
the original frozen matrix. Only checker backing paths and local Git setup
identity differ. Input-equality receipt SHA-256 is
`fcd0b5fd8275b3faf76c8b3458b1fdbdc317f42e25920fde24eb9ca768f775bd`.
Preparation writes no solution and launches no Agent. All fifteen Linux slots
remain NOT_RUN; the next task needs its native task-cwd/source/model freeze.
Counts remain 58 development attempts and 15/30 applicable slots, with original
11 PASS / 4 FAIL / 15 NOT_RUN. Usage/cost and statistical independence remain
unqualified. These document edits need their own normal gates and current-head
CI; user review/merge and canonical synchronization remain open.

## First Linux real-model repair — 2026-10-12

Readiness/documentation head 2277b693 passes normal repository quality and
standalone/distribution checks, pinned strict OpenSpec 70/70 and all sixteen
current-head checks. Gates receipt SHA-256 is
`2920fd7af25f583de3198f28b11e9f268736ec8c48998afeb4269d8bd66a6fcf`.
All 437 macOS and 440 Linux committed/working compiled inputs match the frozen
production candidate. The new first-task freeze has SHA-256
`86ddc5a6a8f7a56b7244175f6c95d534075903f82ed31643393f330032c4fc64`.
The actual native catalog includes gpt-6.1-sol with medium support. The selected
task cwd executes Git 2.53.0 and Rust/Cargo 1.96.0 under linux_reified_namespace;
the setup Tool exits before the original Agent input. This closes readiness
task 1.4, not a model-development slot, and moves checklist to 9/19.

Linux F1 r1 a1 then submits the unchanged original prompt in an actual 80x24
ordinary PTY. Its sole force_agent admission preserves the original body and
binds qualification-gpt61 / ChatGPT / gpt-6.1-sol / medium; its single retained
turn context matches. Source and protected checks start from the original
fixture bytes. a2 actually compiles and runs the seeded contract, failing with
exit 101 and left 5/right 9. a3's optional Git status fails because this frozen
fixture is not a Git repository; the failure stays retained. Model inspection
and hashing follow, with one normal r1 hash-command approval. Delegated repo-coding
launch fails before creating a child because the cwd is outside the explicit
child namespace; that refusal stays visible and does not confer parent grants.
The parent directly edits only src/lib.rs from subtraction to addition through
EditFile, then a7 runs the same offline/locked test successfully. No operator
writes a solution, supplies corrective advice or restarts the model task.

Actual detail selects 8/8, Action a7 under /agent/8, displaying readable original
test output. Closing detail restores the Unicode draft; insertion yields
abcδQXYZ. The complete native transcript stream retains earlier failure/history
bytes. Physical cursor coordinates and a physical Host scrollback gesture are
not measured. No successful child delegation or general terminal-host support
is inferred. The seven observed Tool Processes 9 through 15 all exit, including
the real failing commands, before the model invocation ends.

A separate fresh ordinary-PTY invocation grants the model-authored project and
original protected assertions read-only, with only checker scratch writable.
The original checker driver and assertions are unchanged. Its explicit offline/
locked Cargo command actually compiles the dependency and protected library;
the protected behavioral test covers all four original cases and passes.
The checker admits no Agent input and has zero turn contexts/model turns.
Source inventory uses the existing fixture helper's top-level build exclusion,
preserving exact allowed effects rather than hiding nested source paths.

Independent verification completes 530.917682 seconds after submission. Retained
native script completion records exit zero for model/checker at 409.971706 /
530.971706 seconds (second-resolution timestamps); their final log mtimes are
410.917748 / 531.146728 seconds. Both native PIDs and handles are gone. These
actual completion records are within the original 600-second bound; later
read-only audit/sealing time is not substituted for execution completion time.
The collector initially expects intent agent instead of the actual force_agent
and aborts its final audit; the failed assumption and its correction are retained.
No task, assertion, bound, model input or source effect changes to make it pass.
Shared auth and enabled Memory Store remain explicit; empty context_items does
not prove statistical independence. Usage/cost and time partitions remain unknown.

Immutable attempt receipt SHA-256 is
`c4636708bb41e9f6595143fd63fa6c13c57c5ec1ee00e77ee1a491d7a32c951f`;
all 122 retained receipt/artifact files verify. Aggregate v65 SHA-256 is
`26c2b155a93b2df91fbab52eb3ccb9d96afce87553c4d4dbe708569faea7d4c0`.
It retains 59 development attempts and 16/30 applicable slots, with first
outcomes 12 PASS / 4 FAIL / 14 NOT_RUN. Prior aggregates/attempts stay immutable.
The remaining fourteen Linux tasks, new evidence-head gates/CI, formal review,
user merge and canonical synchronization remain open.

## Remaining Linux small-repair repetitions — 2026-10-12

Evidence head f497fbe7 passes all sixteen distinct checks, including CodeQL.
Immutable gates receipt SHA-256 is
`75c494c300925861fbdde88ad8cfff05e6d980e5b84e23df98f6653899a08827`.
The 437 macOS/440 Linux compiled inputs remain unchanged; no production repair
or model substitution is introduced. Linux F1 r2/r3 both use fresh original
fixtures, normal CLI invocations in actual 80x24 PTYs, qualification-gpt61 /
ChatGPT / gpt-6.1-sol / medium and the same selected Git 2.53/Rust-Cargo 1.96.
Each actual task-cwd version command completes before the original Agent input,
with one planned setup approval. Each model rollout contains exactly one original
force_agent admission and one matching turn context.

r2's first Cargo test fails with exit 101 because absolute difference adds one;
the model changes only src/lib.rs to `(x - y).abs()` and the original test passes.
r3's first Cargo test fails with exit 101 because min(-5) does not clamp; the
model changes only src/lib.rs to x.clamp(-5, 5), names the unused argument _y,
and the original test passes. Both retain the optional non-Git inspection failure
and rejected delegated prelaunch before direct parent fallback. Neither failure
is hidden or represented as successful child execution. r2 has one normal model
hash-command approval; r3 needs no model-selected Tool approval. No operator
source edit, corrective advice, task restart or extra Agent input occurs.

Each actual detail modal acquires the final successful test result (a7 in r2,
a6 in r3); a separate Space event and captured changed page precede closure.
Closing restores Unicode draft insertion, producing abcδQXYZ. Complete native
transcript streams retain earlier history. r2's seven observed model Tool
Processes and r3's six all exit. Physical cursor coordinates and a physical
Host scrollback gesture remain unmeasured, rather than inferred from byte-stream
captures. r3's collector initially expects contiguous raw 7/7 text: the native
loading header actually updates through separate cursor-addressed digit patches.
The failed substring assumption and read-only correction are retained; actual
Action/result identity, original output and changed-page evidence are unchanged.
No renderer or task is altered to satisfy the collector.

Fresh independent checker invocations grant the actual project and original
protected assertions read-only with only scratch writable. Each runs the original
unchanged protected library assertions through a fresh offline/locked Cargo build
and passes all four behavioral cases. Driver/assertion hashes and exact allowed
source effects remain unchanged; both checkers have zero Agent admissions and
zero model turn contexts. Both checker Tools and native instances exit normally.
Independent verification completes at 345.676205 seconds for r2 and 294.056641
for r3. Retained native script timestamps put model/checker exits at approximately
227/346 and 164/294 seconds respectively, within the original 600-second bound.
Timestamp precision is one second; later receipt-sealing time is separate.

| Slot / attempt | Receipt SHA-256 | Verified receipt/artifact files |
| --- | --- | --- |
| linux-f1-r2 / a1 | `d8387d601beedab6275d882b7291db852d5b98dcb3a67413a98a4ab6d1440f4a` | 101 |
| linux-f1-r3 / a1 | `725de8137cf1e54b522b99f40afefe6cc044c8ee58bd071b18243815693d9322` | 92 |

Aggregate v68 SHA-256 is
`30a67386bcfd9a223f51ada1c1dcae997f892789f6d2f918f245a8492c6998cf`.
It retains 61 development attempts and 18/30 applicable slots, with original
first outcomes 14 PASS / 4 FAIL / 12 NOT_RUN. All three Linux F1 repetitions now
close task 3.1; checklist is 10/19. Shared authentication and enabled memory
remain explicit; statistical independence, usage/cost and time partitions are
not qualified. The other twelve Linux tasks, new evidence-head gates/CI, final
review, user merge and canonical synchronization remain open. Earlier attempts
and aggregates are immutable and no original failure is reset.

## First Linux cross-file qualification and retained checker failure — 2026-10-12

Evidence head be25825a passes all sixteen distinct current-head checks. Its
immutable gate receipt SHA-256 is
`66a585b05da0b4699361eab43319a3e93e40883ee03a258f35df8dda99854e70`.
This follow-up changes no production source. Both attempts use the same frozen
Linux CLI, 80x24 native PTY, qualification-gpt61 / ChatGPT / gpt-6.1-sol / medium,
selected Git 2.53 and Rust/Cargo 1.96, and linux_reified_namespace. Each has one
original force_agent admission, one matching model turn context and one planned
setup approval. No operator source edits during execution or corrective model
advice occurs. Before the retry, only the previously sealed disposable fixture's
allowed source changes are reset to the exact original baseline; its build cache
is removed. This is fresh-attempt preparation, not model authorship.

Original a1 correctly changes only src/lib.rs, src/main.rs and src/bin/report.rs,
and the model's library test and both binaries pass with outputs 20 and -1.
Its independently frozen compound checker wrongly uses
/mnt/project-request-1/Cargo.toml as a command-text manifest operand. Native
preflight rejects that untranslatable namespace path before checking protected
behavior. The attempt stays FAIL; no separate weaker check replaces it.
Inspection of the existing Bash schema and native checker arrangement confirms
this is an operator command error: shell text is not namespace-translated and
both Hosts' independent drivers already use explicitly granted native backing.
No production parser or authority change is warranted. The setup queue-shape,
Tool-process selection and builtin cd-output collector corrections stay recorded.

Fresh a2 preserves the original prompt, project/checker baseline, protected
assertions, 600-second bound and grant access. Its corrected operator command is
frozen before the model runs, replacing only the manifest operand with the
explicitly granted native backing path. Private backing paths are never injected
into the model prompt. The model updates all three sources, runs library/binary/
doc-test targets, reports the fixture contains zero tests, and obtains 20 / -1.
The optional non-Git inspection and delegated prelaunch rejection are retained
before direct parent fallback; no child execution is claimed.

After model exit, the separate native checker receives project and protected
assertions read-only and checker scratch read-write. The unchanged original
protected API assertions pass, and both binaries independently compile and return
20 / -1 from writable scratch. Source/driver/assertion hashes are unchanged by
verification. The verifier has zero Agent inputs and model turn contexts. The
actual Tool/native Processes exit; no later source effect is observed. Native
detail opens the final result, pages after a separate Space event, and returns
with Unicode draft insertion abcδQXYZ. A read-only collector correction replaces
an invented detail-heading expectation with the actual Action / Original output
heading. Physical cursor coordinates and a Host scrollback gesture remain
unmeasured; full native transcript streams are retained.

| Attempt | Model PID / boot | Checker PID / boot | Independent verification | Receipt SHA-256 / files |
| --- | --- | --- | --- | --- |
| a1 | 481834 / f129a982-e6dc-474d-89b4-ede41eb0c07a | 492625 / e70ec4dc-8ccd-46aa-aa0c-91c3a0d2d1e2 | FAIL / 290.439277 seconds | `58598a50b12f692371a2cc0969dfc8f1210d17580b64f80e59d169fcf36f7ab5` / 110 |
| a2 | 496628 / b4ef4395-9977-4cd1-9668-b66147a3ddab | 509285 / e731cab2-a406-4e99-8b5a-0781ee358b5f | PASS / 185.747696 seconds | `9005e73b9d4e81f999ba8ee53de38f70ff3e1eb4930e4ab3a7c0acb5804d352a` / 101 |

Both model/checker native exits are within the original 600-second bound, with
second-resolution script completion timestamps retained. Later receipt sealing
is separate. Aggregate v70 SHA-256
`c4747089a98ce1baaa7a5515f973bdbf823ceb6aed435949778a46b48de0ed93`
retains the first FAIL; v71 SHA-256
`98ba793942c99331f93e92174b7b663a7e2ec2efaba69d19c6eaaaf11dc5dcd0`
adds its linked PASS. The earlier aggregate's stale final-candidate count/head
metadata is corrected from actual applicable slot flags, with no receipt or first
outcome rewritten. Latest totals are 63 development attempts, 19/30 applicable
slots and first outcomes 14 PASS / 5 FAIL / 11 NOT_RUN. Checklist stays 10/19;
Linux F2 r2/r3, F3/F4/F5, final audit, fresh publication gates/CI and user merge/
canonical closure remain open. Shared auth, enabled memory, historical verifier
intervention and unknown usage/cost/time partitions remain explicit.

## Complete Linux cross-file family — 2026-10-12

Evidence head 94e8940f passes all sixteen distinct current-head checks; gate
receipt SHA-256 is
`6b848a8c0e4ccdb709cf890d889e3631c9115ad07997de624a6a5d1f5a606e04`.
Its five changed documents leave all 437 macOS/440 Linux compiled inputs
unchanged from the proven be25825a parent. Both subsequent Linux F2 repetitions
freeze and execute that same proven native candidate; no production repair,
provider substitution or authority expansion is introduced.

r2/r3 each start from exact original project, prompt, protected assertions and
checker-driver bytes, with no previous model build output. Known actual catalog,
gpt-6.1-sol/medium binding, selected Git 2.53/Rust-Cargo 1.96 and acknowledged
RW project grant are frozen before the one original force_agent input. Each
model updates only src/lib.rs, src/main.rs and src/bin/report.rs, adds adjustment
to calculate_2/calculate_3 and updates both binary callers. Multiplication gives
51 / -20; minimum gives 3 / -5. All library/binary commands actually exit zero.
The delegated prelaunch rejection remains before direct parent fallback in both;
r3 also retains optional Git's non-repository failure. Neither is hidden or
represented as successful child execution. Each has one planned setup approval
and no model-selected Tool approval, operator source edits, corrective advice,
extra Agent inputs or read-only collector corrections during its execution.

After each model exits, a fresh normal CLI checker grants original project and
protected assertions read-only, with only dedicated scratch writable. Its
prospectively frozen compound command uses the explicitly granted native project
manifest; the original protected API assertions pass and both binaries compile
and produce their expected results. Source/driver/assertion inventories remain
unchanged by checking. Both checker rollouts admit commands only and have zero
model contexts. Observed invocation Tools and native model/checker instances
exit; handles are removed. Each actual final Action detail opens, pages after a
separate Space event and restores Unicode draft insertion abcδQXYZ. Full native
transcript streams are retained; physical Host scrollback gesture and cursor
coordinates remain unmeasured.

| Slot / attempt | Model PID / boot | Checker PID / boot | Independent verification | Receipt SHA-256 / files |
| --- | --- | --- | --- | --- |
| linux-f2-r2 / a1 | 515637 / a5f45847-6cf6-402e-b533-6fb85c7ac789 | 526242 / c9c85eee-9feb-49b1-a1e4-8e48e96301b0 | PASS / 192.507015 seconds | `3c89ebcd4ecf1c545c2e4b742bb3843676523fdac68e1597e0fe7014ebd34106` / 91 |
| linux-f2-r3 / a1 | 532580 / 481900fb-9611-4f19-ab1a-02c55ae04d39 | 545239 / 32e00095-2f8f-4ba5-912c-1dcf3f9dedf3 | PASS / 192.429444 seconds | `b60eb51a103974c9bbb0d1a0a7d010e22337a9b3cc7ceedf7dcce48e8070b99f` / 100 |

All native exits are within the original 600-second bound, using retained
second-resolution script timestamps rather than later audit/sealing time.
All 402 receipt/artifact files across the complete F2 family verify.
Aggregate v74 SHA-256 is
`56de122788bf8cee399952515c550282f35ec8144d226500ab13b1161bbde5f2`.
It retains 65 development attempts and 21/30 applicable slots, with first outcomes
16 PASS / 5 FAIL / 9 NOT_RUN. Task 3.2 closes and checklist is 11/19. Linux
F3/F4/F5, complete metrics/finding audit, fresh publication gates/CI, user merge
and canonical closure remain open. Shared auth and enabled memory do not prove
independence; usage/cost and time partitions remain unknown. Historical failures,
retries and the earlier separate unintended verifier turn remain immutable.

## Complete Linux compiler-correction family — 2026-10-12

All three original Linux F3 attempts qualify on unchanged 23e169e6 compiled
production inputs and the same frozen native CLI. All sixteen current-head
checks pass; immutable gate receipt SHA-256 is
`863f030b99fd00d3ee986ee676c5e875acd93bf556217cb9fedc9d20c2837fc9`.
r1 starts while only doc-head CI remains pending, with compiled production
identity unchanged from passing 94e8940f; the last checks complete during its
run. r2/r3 freeze that source's passing sixteen-check receipt before generation.
This follow-up adds evidence only and requires its own normal gates/CI.

Each actual 80x24 ordinary PTY freezes the original prompt/project/checker bytes,
known ChatGPT catalog and gpt-6.1-sol/medium binding, selected Git 2.53/Rust-Cargo
1.96, linux_reified_namespace and the acknowledged RW project grant before its
one force_agent input. Each has one matching model context and one planned
setup approval, with no model-selected Tool approval, operator source edits,
corrective advice or extra Agent input.

The first Cargo test actually fails with exit 101 and E0425, E0583 or E0308.
r1 adds the missing BTreeSet import; r2 changes the module declaration/reference
to the existing operations module; r3 unwraps parse failure to zero. Models change
only src/lib.rs and rerun actual original tests successfully. Original test,
Cargo, fixture and r2's operations.rs bytes remain unchanged. Optional non-Git
inspection failures in r1/r3 stay retained. r1/r2 also retain delegated prelaunch
rejection before direct parent fallback; r3 has no such failed delegation. No
successful child execution is inferred from direct fallback.

The detail test selects the actual original failed Action a2 after parent
completion, rather than the latest successful test. Post-open native segments
contain the original compiler diagnostic; a separate Space event and changed
page precede closing and Unicode draft insertion abcδQXYZ. r3 additionally seals
the diagnostic patch produced after selecting the failed Action. r1's collector
initially expects contiguous Action a2 header text, whereas native incremental
rendering changes only the numeric cell. Its read-only correction is retained;
the original Action/diagnostic, selected navigation, UI and task remain unchanged.
r2/r3 avoid that assumption and have no collector correction during execution.
Full native transcript streams remain retained. Physical cursor coordinates and
a physical Host scrollback gesture remain unmeasured.

After each model exits, a separate normal CLI checker receives original project
and protected assertions read-only, with only dedicated scratch writable. Its
frozen explicit offline/locked Cargo command freshly compiles model-authored
code and unchanged protected assertions, and protected behavior passes. Source,
assertion and driver inventories stay unchanged by checking. All checker inputs
are commands; there are no Agent admissions or model contexts. Observed Tool
Processes and all native model/checker instances exit with removed handles.

| Slot / attempt | Diagnostic | Model PID / boot | Checker PID / boot | Independent verification | Receipt SHA-256 / files |
| --- | --- | --- | --- | --- | --- |
| linux-f3-r1 / a1 | E0425 | 551609 / 65615a7b-cb7a-4fd3-bea5-8946ef4ad804 | 564233 / 68519802-a673-49a2-907e-ab3241fcdcec | PASS / 354.797932 seconds | `4f9147952dea81f44d65d9fdf13612a3ac0cc5fb5ff5759ac322a86e6e526784` / 86 |
| linux-f3-r2 / a1 | E0583 | 570551 / 6ac69b56-f302-48fc-8d04-a36e616c0f39 | 585141 / aeed2b4b-292c-4c8b-b1c2-9eeb0199c035 | PASS / 287.839797 seconds | `6f1a6c22c46a2004465b86876424fbc5dfcbda2fa93e0893c33dd90af03a4d80` / 91 |
| linux-f3-r3 / a1 | E0308 | 591417 / 1d50faee-a94a-4faf-99ab-867d342b00d5 | 604012 / c18fecc4-2e23-4309-b2e8-ffd2382514c8 | PASS / 438.205438 seconds | `13c3a0aee9a67430c9ac83013b8169bc8ce1af3992fbb21b2a3cb578985750ea` / 87 |

All native exits remain within the original 600-second bounds, with script
completion timestamps at one-second precision. Later audits/receipt sealing are
separate. All 264 family receipt/artifact files verify. Aggregate v78 SHA-256 is
`c4b3f6b6607f386882ec0d296a98c911128235893c1771210e35f0282ef3a199`.
It retains 68 development attempts and 24/30 applicable slots, with original
first outcomes 19 PASS / 5 FAIL / 6 NOT_RUN. Task 3.3 closes; checklist is 12/19.
Linux F4/F5, complete metrics/finding audit, fresh publication gates/CI and
user-controlled review/merge/canonical closure remain open. Enabled memory,
shared auth, unknown usage/cost/time partitions and the historical separate
unintended verifier turn remain explicit; no original outcome is reset.

## First Linux lifecycle attempts remain FAIL — 2026-10-12

Current evidence head 3b4adf5d passes all sixteen distinct checks; gate receipt
SHA-256 is
`637a60222fc3dafa1e619108683f8223a4f2964b3cc4f13a2ebb0bb55fb1c9a5`.
The same frozen CLI, 440 Linux inputs, qualification-gpt61 / ChatGPT /
gpt-6.1-sol / medium, selected Git 2.53/Rust-Cargo 1.96 and original F4 r1 prompt,
follow-ups, assertions, allowed effects and 600-second bound remain in use.
No production repair, model substitution or wider grant is introduced.

Original a1 runs start_work and records one completed ledger entry. The collector
mistakes inner writer.pid 2037 for a guest PID; its ps lookup fails. A subsequent
argv-path/NSpid filter also finds no match. Operator cancellation is late: the
forty-second child writes FORBIDDEN_LATE_EFFECT before Ctrl+C. Kernel Tool 10
already exited normally with code zero; no successful live cancellation is
claimed. The source remains defective and revoke/denied-read/recovery/protected
verification are not completed. This is an operator collector failure and stays
first FAIL. The initial native invocation exits normally. Its late-file bytes,
ledger, timestamps and errors are sealed before fresh-attempt preparation.

Fresh a2 restores exact original disposable fixture bytes and clears only the
previous attempt's effect/build data after retaining its receipt and final-effect
inventory. Before generation, the collector changes only native observation:
match final NSpid within the current native invocation's process ancestry,
without assuming a visible argv path. Cycle/unrelated/root-self assertions guard
that mapping. Inner PID 2037 resolves to guest PID 630679, parent 630677, group
628642, all under native Alan PID 620581. Kernel Tool 11 is actually running
before Ctrl+C, then exited with code 130. The actual writer, parent and complete
group disappear. Past the original forty-second deadline, late.txt remains
absent and the ledger has one entry.

At the settled paused boundary, /project revoke acknowledges inactive request-1
and resets cwd. The unchanged denied-read follow-up is admitted alone, with empty
native draft before /continue. Actual ReadFile Action a4 returns no authorized
Host adapter, with no ledger content, new grant request or extra work. Both
original Agent inputs retain gpt-6.1-sol/medium binding but share one actual turn
context; a read-only collector correction retains its failed two-context
assumption. Individual Kernel and native termination evidence stay recorded.

Explicit recovery compares the exact owned rollout before initial exit and
before --resume, without modifying the shared selector. The first resume launcher
exits one before Alan exec because it tries to exclusively recreate the initial
pty-calibration.json. Its failed transcript/script and absent Host handle remain
retained. The corrected launcher uses distinct resumed calibration/transcript
names and the unchanged selector. A fresh native boot retains the original task
and denied follow-up with known empty queue. Same-project RW reapproval and a
settled cwd Action precede the unchanged continuation. The model requests its
planned structured project confirmation; after approval it edits only src/lib.rs
and runs only cargo test --locked --offline --test contract, successfully.
Copied recovery rows are excluded from new-command analysis. The ledger stays
one entry, writer.pid/writer.started hashes remain unchanged and late.txt is
absent; start_work never replays.

That continuation and cleanup occur after the original 600-second bound because
of operator observation/launcher delays. a2 therefore remains FAIL, despite its
retained correct lifecycle/repair evidence. Required protected verification and
full UI acceptance are not completed within the bound. No bound/start time is
relaxed, and no post-bound result is substituted for qualification. All initial
and resumed native invocations exit; no owned native instance remains.

| Attempt | Outcome | Receipt SHA-256 | Verified receipt/artifact files |
| --- | --- | --- | --- |
| linux-f4-r1 / a1 | FAIL: missed cancellation deadline | `056a23011e40ddaea36a858fe2e4ec23bf9f74965af25edde323482c5f1d720f` | 36 |
| linux-f4-r1 / a2 | FAIL: original overall bound exceeded | `deb72654c49622364f5a4b380108559a1c509ab91bd5737095ace95a38a07ec4` | 143 |

Aggregate v81 SHA-256 is
`71ba3bd35cf3ff25c05296097d861a234672b8be73371e3186cfa284ed803345`.
It retains 70 development attempts, 24/30 applicable slots and original first
outcomes 19 PASS / 6 FAIL / 5 NOT_RUN. Task 3.4 remains open and checklist is
12/19. A fresh original-task retry must use the corrected PID mapping, distinct
launcher filenames, actual context counts and prompt/queue control sequencing,
while preserving the complete original lifecycle and verification scope within
600 seconds. Other Linux F4/F5 repetitions, full metrics/audit and user-controlled
review/merge/canonical closure stay open. Shared auth, enabled memory and unknown
usage/cost/time partitions remain explicit.


## Linux lifecycle collection failures and assisted complete family — 2026-10-12

The original thirty slots, model, fixture bytes, allowed changes, assertions,
40-second delayed effect and 600-second execution bound remain unchanged.
Ordinary 80x24 PTYs use the frozen native CLI, gpt-6.1-sol / medium and the
linux_reified_namespace backend on byte-identical source 0f2958d9. Its sixteen
current-head checks now pass, sealed in gates-0f295-v1/receipt.json SHA-256
`894c363e524343a955e36fc653183a31c750483882d3c06d18b526f10b781079`.
The later documentation update needs its own normal gates and current-head CI.

The controller only sends recorded native controls/explicit grants and reads
bounded numeric-owner SDK evidence. It never writes a solution. Two native
preflight failures have zero model input and remain separate from development
attempts: a3 lacks a fresh-PTY cursor-query answer and exits one; a4 misreads the
SDK listing as records rather than names and quits zero. Both actual native PIDs
and handles are gone. Their receipts remain immutable, including a3's misleading
field that conflated a successful zero exit with absence of a live process.

The a5 model attempt proves timely cancellation and revocation, but waits for
revocation's cwd result in the wrong owner. The actual successful result is an
AgentFS Action, not a rollout Tool call; the collector timeout remains FAIL.
The a6 attempt proves cancel/revoke/denial/recovery/no-repeat and contract success,
but its raw ANSI heading assertion fails. The retained screen does contain
Original output and the actual contract result. It lacks the remaining draft and
protected-verifier proof, so it stays FAIL. Its first cleanup writes /quit into
an open modal; a pinned selected-instance Host stop ends only that owned Root.
The full after-cleanup terminal is retained separately. Its original 17,142-byte
prefix is recovered by the receipt's exact SHA-256 and every sealed artifact hash
is verified; neither receipt nor failure classification is rewritten.

The existing repository dependency vt100 0.16.2 reconstructs incremental output,
with a cache-only Rust reader and assert checks for split headings and Unicode
insertion. No production dependency or terminal rendering code changes.
terminal-frame-v1/receipt.json SHA-256 is
`0e35a94bcff5d1b452291ee8673dbfde82582ec303054211fdea1269a24a21c6`.
Parser binary SHA-256 is
`2649a9b40a23eee67063fde7cd7ccbd53dee4b3298660bb9aee956a553cd227c`.

The a7/a8/a9 failures reveal an actual assisted-workflow boundary. A recovered
model may ask to reapprove an already-authorized namespace path. Explicitly
approving that same Host directory replaces its binding; the old cwd authority
then correctly denies read/command execution until explicit directory selection.
This is the accepted stale-cwd rule, not permission to adopt a new grant silently.
a7 cannot complete the requested contract work after that denial. a8 records the
same pending request and an intentional owned-instance stop before adopting a
new assisted protocol. a9 explicitly reselects the directory after settled
failure but repeats the original recovery directive; the model asks for another
approval and exceeds the predeclared one-extra-request variant. None qualifies.
The original two failures also remain recorded.

| r1 record | Classification | Receipt SHA-256 |
| --- | --- | --- |
| a3 | zero-model collector preflight FAIL | `dcb61da341c270c9fb2a50b06ef2429c14634eb4f0c83e240122f39141a86ae1` |
| a4 | zero-model collector preflight FAIL | `6aa844e464af3bbb9095f289987fcd766303035f78dfd01e2827e2d415481903` |
| a5 | development FAIL: wrong cwd-evidence owner | `d0e5705f26cb89f18a9fc21dc73f27bec49fa31438909327d58b2b05fb6c734f` |
| a6 | development FAIL: incremental-label collection and incomplete proof | `31e8ca1633dbba42f529d4d26a9919f189af230820bdc2064077d8195a110c9e` |
| a7 | development FAIL: same-path replacement leaves stale cwd | `da54f161096ef1257cb55c6e7c33f3eaa1fa8958a699e7b4453f93671e13e7a8` |
| a8 | development FAIL: duplicate reapproval beyond fixed controls | `7ed6adff231b3f6ed74e32e4a01c0cd73dcceeb586076f1b30e17365f981a750` |
| a9 | development FAIL: repeated original directive requests approval again | `a995cb30181416e2518759b41dd5fdbbcbf1217d4b2b0597f32fbb379ca19bf7` |

Before new model input, controller v8 freezes an assisted variant. After a single
same-scope Host reapproval and actual stale-cwd denial, it waits for settled input,
explicitly selects the same directory with native !cd and sends one factual
acknowledgement that approval and directory selection are complete. The original
recovery directive remains in the Tape unchanged; the acknowledgement is
additional corrective model advice, not an original input or an autonomy pass.
There is no solution, code edit or additional authority in this acknowledgement.
Its controller SHA-256 is
`cb8bf18838957b4ae1dd85c58133449736b61b39da5d6146f61e06062a624c73`.

All three fresh runs complete the full original behavior within 600 seconds:

- Live writer resolution uses exact owned-native ancestry plus last NSpid. The
  running Kernel Tool exits 130; native writer, parent and group are gone.
- Past the original writer deadline there is no late.txt and exactly one ledger
  entry. Revocation's correlated Action resets cwd to /; the existing grant is
  inactive and an actual read returns the authority error with no ledger data.
- Initial exit precedes explicit --resume. The exact owned selector, copied
  original/denied admission text and known empty queue are checked before regrant.
  Every initial, resumed and verifier boot has distinct identity.
- The recovered model alone edits lib.rs to x + y. Denied read/Bash attempts from
  stale authority stay retained. Only the requested contract command executes
  successfully; start_work is not repeated and lifecycle hashes remain unchanged.
- A separate fresh verifier has read-only project/assertions and read-write
  checker scratch. Original protected behavior passes, guards/driver/source and
  lifecycle effects stay unchanged, all referenced Tools exit and there are zero
  verifier model calls.
- vt100-reconstructed native frames show actual original contract details,
  separate page movement, close/return and abcδQXYZ insertion into the preserved
  draft. Full ANSI is retained. Physical Host cursor geometry and scroll gestures
  remain unmeasured; the parser is logical PTY evidence.
- Every native invocation quits zero within the original bound. All owned native
  handles are absent after the family. Enabled memory, shared authorized auth and
  unknown usage/cost/time partitions remain explicit.

Each successful run has one setup Tool approval, one additional same-scope Host
approval, one corrective directory selection and one corrective model
acknowledgement. Original setup/regrant/checker mount controls also remain
recorded. Each has zero operator source edits and zero unintended model inputs,
but is not zero-unplanned-intervention. r2/r3 prepared a1 never launched; a2 is
therefore their first actual model attempt, classified PASS with assistance.

| Slot / attempt | Initial PID / boot | Resumed PID / boot | Verifier PID / boot | Complete execution | Receipt SHA-256 / files |
| --- | --- | --- | --- | --- | --- |
| F4 r1 / a10 | 774279 / f1ee60e5-d849-4903-9943-016ab21dedbe | 789845 / 5ea8917c-3090-40bc-85a5-bddabd642d2e | 804434 / 7bacaa28-c763-4856-9183-ad5c06dc2e9d | 149.810737 seconds | `7883c9aff68d70548f42d34d95bd0cc5a766875058c9f0fa555ef943db0747d6` / 106 |
| F4 r2 / a2 | 810768 / cad23a55-1e0b-453a-88f5-fd930032f8ad | 823750 / 9fe348a9-1bfd-4713-9060-63237dd88d40 | 837248 / 772b932d-5b62-4343-b85b-7ae50f10e19d | 131.872968 seconds | `e9c899f38a1743a5de4079424e443164861af53a983b43ff7a12619c72ab129d` / 105 |
| F4 r3 / a2 | 843582 / 9b7b275c-6915-4c64-8ee8-856bdc52ee23 | 854347 / c1ff058b-145d-4da9-81b5-5fe92377cfc4 | 872154 / 7401d419-dbcc-4936-8800-a6c209fce0cf | 153.697165 seconds | `60ced0f1651428c3d324c7e9de3bfb89d56b99cfc6f40ef5102a4e515ce1031d` / 105 |

Independent re-reading verifies all 316 successful receipt/artifact files and the
behavior/lifecycle/detail/draft assertions. All 437 macOS and 440 Linux compiled
inputs still match 0f2958d9. Audit linux-f4-family-independent-audit-v1.json SHA-256
is `b218bb5d7ea4670da01c1f5d044906f3f05ffc256ba3023d375a649cbee40b01`.
The local r2 copy's missing-parent collector error is retained separately; the
same immutable receipt is subsequently verified/copied with no model rerun.

Aggregate v85 SHA-256 is
`4d10723274b72978527ccb90a823f95883b388248d5dc71a4d5aa069c275512a`.
It retains 78 development attempts and 27/30 applicable slots, with first outcomes
21 PASS / 6 FAIL / 3 NOT_RUN. F4 contributes ten development attempts, three
assisted completions and seven failures; its two zero-model preflights are
separate. Task 3.4 closes and checklist is 13/19. Three Linux F5 tasks, complete
metrics/review/final audit, current-head gates and user merge/canonical closure
remain open. These controlled assisted completions do not qualify general
unassisted self-bootstrap or automatic routing.

## Linux F5 completes the original long-result family — 2026-10-12

All tasks use the unchanged a9d88a6a runtime, qualification-gpt61,
gpt-6.1-sol/medium, original fixtures/prompt hashes, exact allowed lib.rs effects,
original 600-second limit and enforcing Linux backend. stdout, stderr and the
initial diff producers each run once. The model reads the original Action through
successful retained reader Actions and authors the dependent constant. Native
original details, changed page, deep marker and Unicode draft return are captured;
full ANSI uses no alternate screen. Separate fresh read-only source/protected
checks pass with unchanged guards/driver and zero verifier model calls. All
owned native invocations quit zero and their handles/PIDs are gone.

The original failed qualifications remain FAIL:

| Attempt | Retained failure | Receipt SHA-256 |
| --- | --- | --- |
| stdout r1 a1 | Full reader Action was correct, but collector checked the clipped rollout summary; remaining UI/protected scope incomplete | `078f8d3ac3e36d46499cf7aa0f988758885cd98811bec9d32bab58e31dc68c00` |
| stderr r2 a1 | Normal model verification authorization was incorrectly treated as collector failure; remaining scope incomplete | `678747f7307f0e9f24e4fddfa18d4023bc5988b932ad227893608ee11bb5a813` |
| diff r3 a1 | Collector required server literal in first deleted-line viewport, rather than the actual diff header | `e430450a82133e3f7a2d028c224c83788c40b7548465a275f6443980f27442cb` |
| diff r3 a2 | Delayed scoped approvals and 531 individual page controls exhausted the original 600 seconds before complete marker/protected scope | `9a406eda9e7c8fe6eb4fb8b5d97d262da2116e93312e63074ecc8134dd943e86` |

Fresh retries retain prior inventory/lib bytes before resetting only allowed
lib.rs to the original fixture. Unknown authorization waits for a review tied to
the exact current boot and request fingerprint. Model-authored compiler mistakes
and subsequent model corrections remain visible; the operator does not supply
source solutions or corrective advice. Long diff retry batches ordinary native
Space controls and still requires an actual reconstructed screen at the marker;
it does not infer visibility from content offsets or relax the bound.

| Slot / attempt | Original bytes / marker | Original Action | Wall seconds | Receipt SHA-256 |
| --- | --- | --- | --- | --- |
| linux-f5-r1 / a2 | 165094 / 42137 | a2 | 175.243795 | `ed0a2fbf6ca8c17e9b97b1fc3368171b57d27dbb1e3e199d3c61abf33f826a5a` |
| linux-f5-r2 / a2 | 165163 / 42274 | a2 | 354.337853 | `de882567833a11f455712fb33691ed201b0d135d2d2be15e1f0d59e6aaaaf2c8` |
| linux-f5-r3 / a3 | 138113 / 42411 | a2 | 196.018466 | `4e4c5793123b19cbe4b4213d65f193e34b67b58f8e5c532a4870af8d0d263a60` |

Normal setup approval is recorded in each run; stderr has one additional scoped
compiler approval, diff has two. Every successful F5 run has zero operator source
edits, corrective model advice and verifier generation. Physical Linux cursor
coordinates and Host scroll gestures remain unmeasured. Available Home/10-line
Space detail navigation has no End and remains laborious for long diffs.

## Complete matrix audit and measured limits — 2026-10-12

Aggregate v93 SHA-256 is
`f3b3370b27985e8ea8c0747a435f601033a1855f6ef0bb6e2c9113591edc9092`.
It retains 85 development attempts, 64 PASS / 21 FAIL attempt records, and
30 applicable slots. Original first qualification is 21 PASS / 9 FAIL / 0 NOT_RUN
(70%); retries do not replace first failures. PASS attempt records include earlier
candidates and are not equivalent to current-source slots. Two zero-model
collector preflights and one historical unintended verifier generation remain
separate; neither is erased by later passes.

Independent audit all-thirty-slot-independent-audit-v1.json SHA-256 is
`682b83b23922eb3055cf147af0fd8e323645df4a8a9c3ca22393a5918b137a66`.
It verifies all 85 retained attempt receipt hashes and 4,694 selected artifact
hashes, actual model/effort, protected behavior, permitted source ownership,
recorded lifecycle and original wall limits for all thirty selected completions.
All selected solutions have zero operator source edits. Three Linux F4 selected
completions require corrective model advice and authority reselection; 27 do not
require corrective advice. Zero unplanned intervention is not inferred from that
27: collector corrections remain enumerated per selected receipt, and a strict
cohort count is not established. Enabled memory/shared auth prevent a statistical
independence claim. Provider usage, cost and model/Tool/wait time partitions remain
unknown. This is bounded supervised development evidence, not general autonomous
self-bootstrap qualification.

| Platform / family | Minimum / median / maximum wall seconds (three selected completions) |
| --- | --- |
| macos / F1 | 135.043 / 144.883 / 165.530 |
| macos / F2 | 213.396 / 308.698 / 440.608 |
| macos / F3 | 229.572 / 259.021 / 333.269 |
| macos / F4 | 254.868 / 401.000 / 415.959 |
| macos / F5 | 238.029 / 269.992 / 299.233 |
| linux / F1 | 293.597 / 345.897 / 530.972 |
| linux / F2 | 191.856 / 192.129 / 218.600 |
| linux / F3 | 287.906 / 355.693 / 437.440 |
| linux / F4 | 131.873 / 149.811 / 153.697 |
| linux / F5 | 175.244 / 196.018 / 354.338 |

macOS and modern Linux collectors measure submission through verified native
cleanup. Older Linux F1–F3 use the latest captured actual native exit, whose
terminal timestamp has one-second precision; later audit time is not runtime.
Values include planned controls and operator waits and are not pure model speed
or directly comparable benchmark estimates.

Final review repairs two test-layout violations: public namespace read contracts
move to crates/tools/tests/namespace_read.rs; private SystemProcessRunner scenarios
move to adjacent process_runner_tests.rs. Exact production bodies are unchanged,
435/437 macOS and 438/440 Linux frozen inputs match byte-for-byte, and the two
remaining input changes are cfg(test) declarations/test extraction only. No
production visibility/API, dependency or runtime configuration changes. Six Tool
and two Process focused tests pass; normal quality/distribution pass. A future
production change requires fresh applicability assessment rather than reuse of
this proof. Current committed-head CI and user-controlled canonical closure are
still required.

### Current-source gate closure

Head ceb71ee6b492faac0143329bf45a35b602860bf2 passes all sixteen completed
current-head checks. The sealed gates-ceb71-v1 receipt records check names,
conclusions, timestamps and direct run URLs. Normal commit quality/distribution,
strict OpenSpec 70/70 and both final review axes pass. Task 4.4 closes at 18/19;
this documentation-only recording must pass its own CI. User merge and canonical
closure remain task 4.5, with no autonomous merge or routing enablement.

## Cargo selector/global-option P2 after ready-triggered review — 2026-10-12

Codex review of e8f4f076 finishes after the readiness report and adds
`discussion_r4246031780`. The finding is valid. Portable RED fails the read-only
manifest on cargo +1.97.0 build; actual Linux RED rejects the same manifest before
compiler effects. The shared cargo_subcommand function moves unchanged from the
Rustup inspector to internal command-options and is reused by path_literals.
Selectors and separated/inline global values now reach the real subcommand;
original policy, rights, output/redirection and -- boundaries remain unchanged.

Five manifest tests and two Rustup prefix tests pass. The existing macOS native
Seatbelt program exercises six prefix/parent combinations: source reads and
scratch writes succeed, writes to the explicit read-only source fail. A fresh
owned Linux build passes the full native development fixture in 109.90 seconds,
including three actual +selector/global-prefix compilations from a read-only
manifest, four pre-effect negative controls (read-only output, ungranted manifest,
forwarded operands, unknown subcommand), source/runtime/home/network/cache guards
and descendant lifecycle. These are adapter/native checks, not new model slots.

The newly built normal CLI also admits +1.97.0 --color never test against the
read-only original project manifest while cwd/target remain in writable scratch.
It compiles actual fresh artifacts, exits0 and records no model turn. Its exact
normal scoped approval and all 27 receipt/artifact files are verified. Receipt
SHA-256 is `7ef0a890200fd6c90e63ed66527272d5de376eb2ac36dfc5d1d86e8c51b2f777`.
Physical Host cursor geometry remains unmeasured.

Fresh Mac/Linux CLI SHA-256 are respectively
`e02537a889ba998f653438e2f39eaf635371139ce37e2e823a275bbfa84e1121` and
`cad2b7e6f30f04ac9a60fbfccf8d752e840a147d3ef165e140d3eea0afbecf5d`.
Their 437/440 actual compiled-input hashes match this repaired source. Original
binary hashes do not stand in for these new binaries.

The applicability ledger verifies 66 selected initial/resumed/verifier rollout
references for all thirty slots, covering 193 completed Bash records and a
superset of 450 structured arguments (duplicates, proposed/cancelled requests
included). Source hashes/captured prefixes and exact admission/Action UUIDs are
checked before extraction. A compiled old/new comparison using existing tokenizer
and wrapper inspection verifies 3,415 token roles, zero differences/refusals and
zero nested wrapper scripts in this corpus. Rustup parser and surrounding
selection code are unchanged after removing its move/import. Policy/rights and
other runtime owners are unchanged. This no-impact proof plus targeted new-form
native checks supports inherited applicability of the same thirty slots without
rewriting a receipt or adding a model task. The 85 attempts, first21PASS/9FAIL and
three assisted Linux lifecycle completions retain their original meaning.

source-applicability-ledger.json SHA-256 is
`acc0d1b32c00cedbf052962cc4fc4a34e230236d1f948ccde39ead6f4c9dba8a`.
The complete request corpus SHA-256 is
`9dd406b8f8eba0f1a2ecfef79f8d3ee22e937a1c381e728c5420fff6dc0bd52a`.
It contains only extracted Tool arguments/provenance, no memory or provider bodies.

Retained operational attempts are not promoted to PASS: shared Cargo target
reuse returned the old binary; the owned output helper refused an archive without
Git metadata; the first CLI launch lacked a runtime parent; another requested
literal 1.96.0 that was not installed and was refused before effects. The fresh
owned build and installed 1.97.0 CLI retry supply actual acceptance. Those
preflights have zero model generation and do not alter development attempt counts.
Normal repaired-source quality/distribution pass. Strict OpenSpec, final commit
CI, thread reply/resolution and completed current-head automated review remain
separate closing gates. Task 4.4 reopens; task 4.5 still requires user merge and
canonical synchronization. No automatic merge or autonomous self-bootstrap claim.
