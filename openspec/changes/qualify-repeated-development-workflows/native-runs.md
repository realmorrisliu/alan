# Initial native development runs — 2026-10-10

All three frozen macOS F1 inputs qualify through four normal-CLI real-model
executions in Herdr, without corrective model advice or operator source edits.
The original r1 attempt omitted prospective exact PATH/executable identity and
remains FAIL; its fresh linked retry passes without replacing that first outcome.
Including the subsequent F2 attempt below, first outcomes are two PASS, two FAIL
and twenty-six NOT_RUN. Qualified slots
are three out of thirty, closing task 2.1 only. Linux generation readiness,
task 1.4 and the remaining families stay open.

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
