# Initial native development runs — 2026-10-10

Two frozen macOS F1 inputs have executed through the normal CLI in Herdr. Both
models repaired their project without corrective advice or operator source edits.
Only the second attempt qualifies: the first omitted prospective exact PATH and
executable identity. Keep that failure; successful coding alone cannot repair a
missing precondition. The matrix has one PASS, one FAIL and twenty-eight NOT_RUN
slots, with one qualified completion out of thirty. Task 2.1 remains open until
all three F1 repetitions have qualified evidence, including a fresh linked retry
for r1. Linux generation readiness and task 1.4 remain open.

## Candidate and receipts

Production source is unchanged #1046 `cace913c`; the clean native checkout's CLI
SHA-256 is `b66362691d83e8f63867058b3a15afd9ddbbbc721ef151a23b93b2ef53c8d6fd`.
Both actual admitted callable bindings and generation contexts confirm
`chatgpt-main`, provider `chatgpt`, model `gpt-6.1-sol`, reasoning effort `medium`.
Both instances expose the same default Memory Store; no personal memory bodies
were collected or cleared, and statistical independence is not claimed.

Owned evidence root:
`~/Library/Caches/Alan/qualification/dev-20261010-10332030`.
The immutable thirty-input fixture manifest is `fixtures-v3/manifest.json`,
SHA-256 `b64ad0126be948a69d46f4936cf25caacc6ff8d69b61bcda6e2b37229c32b25e`.
`first-outcomes-v1.json` preserves all thirty slots, SHA-256
`d8689edaed159ed8777bc5fe3f3e2d35c1189ce65d6f00ce07d7195da387c81b`.

| Slot / attempt | First outcome | Model task time | Submission through verified cleanup | Receipt SHA-256 |
| --- | --- | --- | --- | --- |
| macos-f1-r1 / a1 | FAIL: incomplete prospective tool identity | 31.88 s | 227.34 s | `343e968cf9d5f89eba55d1051974fd807262b2ccc0f02239d05cf440f3db2e2d` |
| macos-f1-r2 / a1 | PASS | 31.50 s | 228.62 s | `02cf4ac6e0c66483e771591db2f4882a3bb4d9742fbc7931e22adbb6d5f37841` |

Receipts are `attempts/<slot>/a1/receipt.json`. Model task time ends at the
retained final assistant message. Total time includes observation, planned detail
checks, instance exit and independent native verification. Model/Tool/wait time
partition, usage and cost remain unknown. Both attempts have zero unplanned model
steering and zero operator source edits; planned grants, the setup Tool approval,
detail observations and verifier lifecycle are recorded, not approval-free autonomy.

## Authorship, independent checks and lifecycle

Each model first ran `cargo test --locked --offline`, retained a genuine contract
failure, changed only `src/lib.rs` with EditFile, then reran Cargo successfully.
The original tests, Cargo files and fixture data matched their frozen hashes.
The first restored addition; the second removed the extra one from absolute
difference. Both also encountered and independently recovered from a Git command
in a non-Git fixture. Those Tool failures remain in the receipts.

After each model invocation actually exited, a separate normal native invocation
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

r2 opened the final Cargo Action details with a nonempty draft and a cursor in
its middle. After returning, inserting a character produced `abcδXYZ`, proving
the same draft and logical insertion point survived. Raw detail and terminal
captures remain retained. This is bounded input/detail evidence; it does not
replace the later long-output family or broad UI qualification.

## Collector correction and observed product limitation

r1 did not freeze exact selected PATH/executable digests before generation. The
next run captured selected paths, rustup proxy and real compiler/Cargo digests,
Git digest, versions and platform/kernel, then explicitly supplied that captured
PATH to the native invocation. Tool versions were independently checked through
the actual product before submitting generation. A future r1 retry needs a fresh
fixture; neither its original source nor its first outcome may be overwritten.

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

r1 Action a4 rejected an absolute `/mnt/project-request-1/src/lib.rs` operand in
a read-only `sed` command as outside host_mount; relative operands subsequently
succeeded, and EditFile addressed the same absolute namespace path successfully.
Tracing shows Bash passes command text directly to native Sandbox validation,
while file Tools use the Host adapter's namespace path resolution. Retain this as
an observed shell projection limitation to reproduce and review in the Host/Tool
owner before a repair. Do not replace strings indiscriminately, weaken containment
or grant native backing paths to the model. No production fix is claimed here.
