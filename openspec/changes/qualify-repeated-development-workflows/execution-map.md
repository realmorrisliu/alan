# Native execution and evidence map — 2026-10-10

This records supported operator controls. Current actual task outcomes are in
`native-runs.md`: eleven first PASS, four first FAIL, fifteen NOT_RUN slots and
fifteen qualified slots through thirty-four development task attempts. All
fifteen macOS slots now match the final candidate; Linux generation is open. Fixture
self-checks and provider/backend preflights remain separate from model-authored
task outcomes.

## Candidate and prerequisite evidence

- UI implementation: main `faf747c2`, merged #1044; closure #1045 remains open.
- Toolchain candidate: #1046 `cace913c3cbe76a63fc3f7b6dfb9bcb5468891b8`.
  Its current-head sixteen CI checks pass. Direct caller/boundary review and
  native positive/negative toolchain evidence precede these disposable tests.
  The proposed ADR-0058 grant extension, formal review, user merge and canonical
  synchronization remain final-delivery gates.
- macOS native CLI: cace913c plus the exact production repairs and 35-file
  inventory recorded in `native-runs.md`, built in the owned
  `responses-native-verify-20261011` worktree at
  `target/quality-gate/aarch64-apple-darwin/debug/alan`; SHA-256
  `1b609034eea7c8c2a7f7f5a1725d14fb6ed5165ec4776a196bd7237162a2495e`;
  tracked source diff `60fbb03ab93f2d1a6d94cc924a064fd12dae38052cd25c9615608e3da8085608`.
  The separate source-file inventory also covers new files; the diff hash alone
  is insufficient. This is qualification evidence, not an installed deployment.
- Actual macOS generation preflight succeeded in 14.23 seconds. Its own rollout
  `32a46d54-3cc1-412c-b43c-b6d9d27f2876` records model `gpt-6.1-sol`, reasoning
  effort `medium`, and Process path `/proc/8`. This is not a matrix slot and has
  no retained live boot-status receipt. An earlier setup failure remains retained.
- The same public candidate passes the complete normal Linux `just quality` in
  its own real Git checkout, including standalone/distribution checks. The gate
  uses the cace913c candidate's quality script; phase-specific fixture checks and
  final merged-source applicability remain separate requirements. The unchanged
  35-file inventory and all 2283 tracked source files match the public archive.
  The actual quality-built CLI SHA-256 is
  `6b7565827d0b8479d2a6d6315deb51e71a1c9c2eaa8c8982171fd9f4f95ae125`.
  Its fresh 80x24 ordinary-PTY explicit-command preflight passes Git
  version/status/diff and protected read-only Cargo compilation/write denial;
  both Tool Processes exit, the queue empties and the owning invocation exits.
  The compound Git/version command requires one human approval under existing
  safe degradation; Cargo is classified write and allowed. No blanket autonomous
  Bash or full protected-subpath confinement is implied.
  Actual Tool stdout selects Git 2.53.0 and Rust/Cargo 1.96.0 in checker scratch,
  while the repository quality gate selects Rust/Cargo 1.97.0. Freeze actual
  task-cwd selections before Linux generation rather than inheriting the gate's
  versions. Its latest native `connection current` reports `effective_profile
  none`; Linux generation readiness and all fifteen slots remain open. No macOS
  credential or private store has been copied. See the latest `native-runs.md`
  receipts; neither preflight nor quality closes a real-model slot.

## Supported product controls

The authority is `crates/alan/src/main.rs`, `foreground.rs`, and the file-backed
TUI handlers. Use the normal product CLI, not `shadow_client_fixture` or an
ephemeral/mock boot. Do not enable `--shadow-evaluator` or automatic routing.

| Operation | Supported surface | Required observation |
| --- | --- | --- |
| Fresh invocation | Bare `alan` on terminal stdin/stdout | Own Host PID/boot ID and Root Process; no borrowed Root |
| Explicit input | `: ` for Agent work, `!` for a deterministic command | Original bytes, admitted submission ID and dispatched/completed events |
| Project authorization | `/project`; read-only default, Tab selects read-write, Enter approves, Esc cancels | Request ID, granted Process, namespace path/access and native project identity |
| Project revoke | `/project revoke` | Machine cwd reset and Host grant revocation acknowledged before later-effect probe |
| External grant inspection | `alan host mount list`; `approve <request-id> <native-directory>`; `revoke <grant-id>` | Select the exact instance and retain logical IDs/access; no raw Host path as agent authority |
| Interrupt | Ctrl+C during active work | Correlated submitted Process/submission; descendants gone before scheduled late effect |
| Exit | `/quit`, or Ctrl+D with an empty draft and no pending form | Native invocation terminal and its instance ended; a Herdr detach alone is not exit |
| Explicit durable recovery | Bare `alan --resume` | Current selected rollout belongs to the intended interrupted attempt; recovered segments retain model/effort and grant observations |
| Queue continuation | `/continue`, `/discard` | Queue control acknowledgement, distinct from a fresh invocation's durable recovery |
| Retained details | Ctrl+O; `p` toggles Actions/plans; arrows and PgUp/PgDn; Esc returns | Same Action owner/ID, original output/result, stable draft/cursor and scrollback |

`ALAN_CONFIG_PATH` selects a task-only absolute configuration containing
`connection_profile = "chatgpt-main"` and `model_reasoning_effort = "medium"`.
It contains no inline credentials and changes no default profile.
`ALAN_INSTANCE_RUNTIME_DIR` selects an existing instance boundary for Host controls.
Create its parent first and use short per-attempt names: its appended
`Alan OS/namespace.ap.sock` must be shorter than 104 bytes on macOS, 108 on Linux.

Recovery currently selects the Agent Runtime Service's `metadata/root-rollout`
filename (`crates/service-manager/src/agent_runtime/root_recovery.rs`). Before
`--resume`, compare that selection and its rollout ID to the interrupted attempt.
Do not silently resume a newer unrelated invocation, rewrite the selection file,
or invent a CLI flag for arbitrary rollout IDs. Interleaving another Root can
change this shared selector; serialize recovery qualification and retain mismatch
as a real limitation rather than forcing a pass.

## Store and observation ownership

Use current `crates/os-host/src/paths.rs` and `boot.rs`; there is no channel suffix
in these paths. macOS data root is `~/Library/Application Support/Alan`; Linux is
the actual XDG data root, ordinarily `~/.local/share/Alan`.

| Evidence | Owner/location | Collection rule |
| --- | --- | --- |
| Process lifecycle | `/proc/<pid>` | Record boot ID as well as PID; reused numeric PIDs are not cross-invocation identity |
| Agent request/Action/Machine view | `/agent/<pid>` | Pin owner Process and Action/request IDs; `/agent/root` is only an instance-local alias |
| Tool retained detail | `/agent/<pid>/actions/<id>/{name,status,output,result}` | Correlate original output and result metadata; visible preview is insufficient |
| Durable execution | System Store `services/agent-runtime/{rollouts,checkpoints,metadata}` | Reference and hash only the owned run's files; Tape projection is not a recovery substitute |
| Root definition | System Store `services/agent-runtime/definitions/root` | Record identity and exposed definition; no project-directory overlay |
| Memory exposure | System Store `services/memory/stores/default` | Record exposure/config identity without clearing personal memory or claiming independent samples |
| Composer history | System Store `services/shell-ui/composer-history` | Existing service-owned state, not qualification output |
| Connection metadata | System Store `services/connections/connections.toml` | Collect profile/provider/model metadata, not private settings bodies |
| Credentials/auth | Host Store `credentials/` and `auth.json` | Never collect bodies or copy them between Hosts |
| Qualification captures/receipts | Unique owned Host cache | Retain failures, exact sources/effects, interventions and references; never overwrite `latest` |

There is no generic `alan host fs cat` command. Use supported TUI details and
instance status/mount commands, plus read-only observation of the owned durable
records. The qualification-only `development_observer` example connects through
the existing public `LocalAttachment` SDK to the already-running native instance;
it never boots an Agent or runs a model. Its `read` command requires a numeric
Process path and verifies the expected Host boot before and after connection.
`read <path> --list` lists actual Action/request names through the existing bounded
Shell API (1,024 entries / 1 MiB), without allocating clone endpoints. Ordinary
directory or Action publication can lag admission; reobserve the same owned
instance after a not-found response, never repeat an input to manufacture an ID.
Its explicit `mount` command uses existing `HostCommandPlane::mount_project`,
with a frozen operation UUID, expected boot, absolute owned directory and access.
Retain intent before effects; reconcile uncertain transport with that same UUID,
never allocate a replacement operation blindly. Record the observer source/binary
separately from the production CLI. Any explicit shell observer input remains a
timeline event and must not become invisible model steering.

## Host terminal and fixture preparation

Herdr is an ordinary terminal host here; Alan-specific detection is unnecessary.
Use a task-owned sibling pane with `--no-focus`, returned opaque IDs, raw pane
input/read APIs and a terminal capture. Do not use Herdr Agent idle/done as Alan
completion. Record the actual PTY winsize and visible Host geometry before 80x24
qualification; current caller layout and viewport disagree, so neither alone
proves the test dimensions. A task-owned calibration pane (`w58:p18`) reported
120x40 from its actual stdin PTY while the Host layout reported 60x40 for that
pane. This is not an 80x24 acceptance; the pane was closed after confirming its
foreground was only the owned shell, leaving the caller focused. The mismatch
was resolved in isolated named session `alan-development-10332030`: task-only
headless geometry was set to 80x24 with pane scrollbars disabled, then a new
owned pane `w4:p1` reported actual stdin PTY 80x24 and Host geometry 80x24. No user
session configuration or focus changed. The earlier mismatch remains retained;
the final calibration is not a task outcome. Linux uses the native CLI in an ordinary PTY with
the independently qualified enforcing backend and exact tool environment.

Prepare all thirty inputs in a new cache directory:

```sh
python3 scripts/harness/repeated_development_fixtures.py --output <new-absolute-owned-cache>
python3 -m unittest discover -s scripts/harness -p test_repeated_development_fixtures.py
```

The manifest hashes each prompt, follow-up, baseline file, protected verifier and
independent checker driver. The model's Root is granted only `projects/<slot>`;
never grant it the parent cache, protected verifier, checker scratch, operator
metadata or Alan production checkout. The long-diff fixtures contain a
local Git baseline and a separately frozen 3000-line working diff. Git setup uses
no global config, hooks, templates or commit signing. The original diff is Host
fixture setup, not model authorship.

The self-check authors disposable corrected examples only to prove that the
protected assertions distinguish failure from correct behavior. These examples
never enter the qualification manifest or model project. Independent runtime
verification must keep these protected assertions outside model authority and
execute model-authored code through the qualified isolation boundary. After the
model invocation exits, a separate owned native verifier invocation receives only
the original project read-only, `protected/<slot>` read-only and its dedicated
`checker-scratch/<slot>` read-write. The driver manifest in that scratch directory
references the protected assertion source and project dependency; run explicit
`cargo test --offline --locked --target-dir target` there. Expected assertions
remain outside both model write authority and verifier write authority. Keep
source/driver hashes and compiler/test output before and after checking, not just
exit zero. These operator-owned driver manifests use native backing paths on both
Hosts: `Sandbox::reified_mount_declarations` preserves granted Host paths in the
Linux runner too. Alan namespace paths remain the Agent's command/file surface,
not implicit replacements inside arbitrary file contents. Freeze the exact driver,
its acknowledged grants and backend before execution; never inject private backing
paths into model prompts or grant manifests to the model. Do not alter assertions,
copy dependency contents or broaden grants to make a run pass.

A macOS native preflight exercised this driver arrangement with separate live
read-only project/checker grants and writable scratch, actual fresh compilation
and assertions that attempts to open both source and checker for writing fail.
All source/checker/driver hashes remained unchanged. Wrong-boot mounting failed
before intent; repeated same-operation mounting returned the original grant.
Earlier direct `cargo test` from read-only cwd and an explicit read-only
`--manifest-path` were refused before execution and remain retained boundaries.
The writable driver uses ordinary granted dependency reads, without weakening
those guards. This is candidate preflight, not a matrix slot or macOS general
read-isolation claim. Linux's separate normal-CLI preflight also compiled both
packages under the audited `linux_reified_namespace` backend and passed protected
behavior/read/write-denial checks. Its initial incorrect operator fixture paths
remain retained; provider was unconfigured and no generation occurred. See
`preflight-evidence.md`. A model's own test claim or offline self-check cannot
fill the independent runtime verification requirement.

Build output ownership is Host-specific even when source worktrees are shared.
Use the Linux toolchain checkout as explicit build owner with this qualification
checkout as workspace and a new native output path for future Linux SDK builds;
keep macOS quality output registered only to its macOS owner. An initial Linux
SDK build registered its guest-only output in this shared qualification checkout,
so the next Mac commit correctly refused it. After verified native build/CLI exit,
read-only Linux process/handle inspection and admission/output locks, only that
foreign registry entry was archived in the owned Linux cache. Its output tree,
sidecar and qualification receipts were preserved; no output was cleaned or
reassigned. The next normal commit must rerun the gate.

F4's ignored Rust lifecycle test appends its completed entry, spawns an observable
child and schedules a write after forty seconds. Observe `writer.started` and the
actual child before cancel/revoke; wait past that deadline, explicitly recover,
reapprove and run only the contract test. Missing/duplicate ledger entries or any
`late.txt` fail. F5 runs long stdout, long stderr and long initial diff respectively;
QVALUE is beyond inline bounds. Tool/action correlation, retained detail, dependent
edit, draft/detail return and scrollback remain live assertions, not fixture checks.

Before the first task, freeze all thirty inputs, bounds and evidence collection;
freeze runtime/tool/backend identities, actual terminal calibration and model
readiness before execution on each Host. An unavailable Host remains explicit and
does not prevent ready-Host runs; all thirty slots and full completion criteria
remain required. The preparation fixture manifest explicitly says
`runtime_readiness_frozen: false` and cannot substitute for a Host readiness receipt.
Missing usage/cost stays unknown. Capture first outcomes and linked retries without
turning a final answer, command exit or preflight into a qualification pass.


The receiving Agent's concrete Action output can now be acquired locally through
ReadFile byte_offset/byte_limit (maximum 4096), using the actual invocation's
namespace/parent; Host files keep line ranges and mixed modes remain invalid.
Native reader candidate F5 r1 a2 fails through four mixed-mode calls; aggregate
v16 retains eighteen attempts and twelve qualifications. This does not yet qualify
model use of the reader. A fresh candidate must include the Responses optionality
and AgentFS guard-error publication repairs, freeze all identities again, then
retry the unchanged original task without operator advice or source edits.


Fresh unchanged macOS F5 r1 a3 now qualifies the actual own-Action byte reader:
original a2 runs once, a5 acquires QVALUE from its retained bytes, and independent
read-only native verification/draft/details/scrollback/exit proof passes. Aggregate
v17 is nineteen attempts/thirteen qualified slots, retaining both failed retries.
The observer reads actual Tool Process status separately from virtual cd's Root;
observe asynchronous Action/detail publication before dependent gestures. Send
Escape separately from Unicode to avoid terminal Alt-key ambiguity, and move to
the end before Ctrl-U when clearing the complete draft. These collection controls
are not model advice or substitute Tool execution. Remaining frozen tasks and
final-candidate/delivery gates are unchanged.
