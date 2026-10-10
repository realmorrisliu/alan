# Native execution and evidence map — 2026-10-10

This is operator preparation for this change, not a qualification report. The
matrix still has thirty NOT_RUN slots. Fixture self-checks and provider preflight
are separate from model-authored task outcomes.

## Candidate and prerequisite evidence

- UI implementation: main `faf747c2`, merged #1044; closure #1045 remains open.
- Toolchain candidate: #1046 `cace913c3cbe76a63fc3f7b6dfb9bcb5468891b8`.
  Its current-head sixteen CI checks pass. Direct caller/boundary review and
  native positive/negative toolchain evidence precede these disposable tests.
  The proposed ADR-0058 grant extension, formal review, user merge and canonical
  synchronization remain final-delivery gates.
- macOS native CLI: the candidate's owned
  `target/quality-gate/aarch64-apple-darwin/debug/alan`; SHA-256
  `b66362691d83e8f63867058b3a15afd9ddbbbc721ef151a23b93b2ef53c8d6fd`.
- Actual macOS generation preflight succeeded in 14.23 seconds. Its own rollout
  `32a46d54-3cc1-412c-b43c-b6d9d27f2876` records model `gpt-6.1-sol`, reasoning
  effort `medium`, and Process path `/proc/8`. This is not a matrix slot and has
  no retained live boot-status receipt. An earlier setup failure remains retained.
- Linux's candidate CLI builds natively, but no Connection metadata or Host
  credential store was present at inventory. Linux generation readiness remains
  unverified; no macOS credential or private store has been copied.

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
records. If an explicit shell read is required, record it as an observer input;
it changes the timeline and must not become invisible model steering.

## Host terminal and fixture preparation

Herdr is an ordinary terminal host here; Alan-specific detection is unnecessary.
Use a task-owned sibling pane with `--no-focus`, returned opaque IDs, raw pane
input/read APIs and a terminal capture. Do not use Herdr Agent idle/done as Alan
completion. Record the actual PTY winsize and visible Host geometry before 80x24
qualification; current caller layout and viewport disagree, so neither alone
proves the test dimensions. A task-owned calibration pane (`w58:p18`) reported
120x40 from its actual stdin PTY while the Host layout reported 60x40 for that
pane. This is not an 80x24 acceptance; the pane was closed after confirming its
foreground was only the owned shell, leaving the caller focused. Linux uses the
native CLI in an ordinary PTY with
the independently qualified enforcing backend and exact tool environment.

Prepare all thirty inputs in a new cache directory:

```sh
python3 scripts/harness/repeated_development_fixtures.py --output <new-absolute-owned-cache>
python3 -m unittest discover -s scripts/harness -p test_repeated_development_fixtures.py
```

The manifest hashes each prompt, follow-up, baseline file and protected verifier.
Only `projects/<slot>` is granted; never grant the parent cache, protected verifier,
operator metadata or Alan production checkout. The long-diff fixtures contain a
local Git baseline and a separately frozen 3000-line working diff. Git setup uses
no global config, hooks, templates or commit signing. The original diff is Host
fixture setup, not model authorship.

The self-check authors disposable corrected examples only to prove that the
protected assertions distinguish failure from correct behavior. These examples
never enter the qualification manifest or model project. Independent runtime
verification must keep these protected assertions outside model authority and
execute model-authored code through the qualified isolation boundary. A model's
own test claim or this offline self-check cannot fill that requirement.

F4's ignored Rust lifecycle test appends its completed entry, spawns an observable
child and schedules a write after forty seconds. Observe `writer.started` and the
actual child before cancel/revoke; wait past that deadline, explicitly recover,
reapprove and run only the contract test. Missing/duplicate ledger entries or any
`late.txt` fail. F5 runs long stdout, long stderr and long initial diff respectively;
QVALUE is beyond inline bounds. Tool/action correlation, retained detail, dependent
edit, draft/detail return and scrollback remain live assertions, not fixture checks.

Before the first task, freeze runtime/tool/backend identities, actual terminal
calibration, evidence collection and per-Host model readiness alongside the thirty
inputs. The fixture manifest explicitly says `runtime_readiness_frozen: false`.
Missing usage/cost stays unknown. Capture first outcomes and linked retries without
turning a final answer, command exit or preflight into a qualification pass.
