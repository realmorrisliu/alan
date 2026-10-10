# Initial native development runs — 2026-10-10

All three frozen macOS F1 inputs qualify through four normal-CLI real-model
executions in Herdr, without corrective model advice or operator source edits.
The original r1 attempt omitted prospective exact PATH/executable identity and
remains FAIL; its fresh linked retry passes without replacing that first outcome.
Including macOS F2/F3/F4 and retained failed attempts below, seventeen real-model
task attempts qualify twelve slots: nine first PASS, four first FAIL and seventeen
NOT_RUN. Task attempts are not API-call counts; F4 includes explicit continuation. Tasks 2.1, 2.2,
2.3 and 2.4 are complete. Linux generation readiness, task 1.4, F5 and final
candidate applicability remain open.

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
