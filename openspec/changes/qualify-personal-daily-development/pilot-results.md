# Bounded read-only pilot — 2026-10-23

This is preflight measurement, not the formal daily-use cohort. Both original
tasks were admitted once; the failed task remains in the denominator. Formal
progress is **0 tasks / 0 days**. No readiness or bootstrap claim follows.

## Frozen conditions

- Private clean candidate `ff1e39e0699c238ed64c1c2c27645c9a21dd3590`, tree
  `62872d6ca604d219e6d8419e99fa38bd1d07e9fd`; binary SHA-256
  `2893a1917d2adca3ce5571526767d09bc719bd228e3763e0a37ba5e73ce1e39f`.
  Its sixteen CI checks passed; the installed product was not changed.
- Model `gpt-6.1-sol`, reasoning `medium`, memory disabled, explicit isolated
  Host/System Store bindings, bare foreground invocations through an ordinary terminal (the pilot pane ran in a terminal multiplexer host; no host-specific behavior was relied on).
- Each task: 600 execution seconds, 30 Tool attempts, 10 model admissions;
  zero task retries and no extensions. A separate Host supervisor allowed at most
  660 seconds per invocation. It is additional containment, not proof that every
  in-runtime IO/deadline boundary is qualified.
- Alan snapshot `ea92948141003cbe43bb76a44cb2aaccaad04c3a`; website snapshot
  `6aa44ab92f9aa5098c42eab61aad8d8ed69d472f`. The original Alan fixture was missing;
  new detached worktrees used those same commits. Each project was explicitly
  selected read-only through `/project`. No build, install, publishing or deployment
  task was authorized. The dirty primary website checkout was left in place.

## Observed outcomes

| Task | Outcome | Models | Tools | Charged execution |
| --- | --- | --- | --- | --- |
| P1: Alan authorization/Guardian call-chain review | Budget exhausted before a final answer; extension rejected with empty Enter | 10/10 | 9/30 | 72.745s |
| P2: website checks/build/download/publication review | Completed, core findings independently checked against the frozen source | 6/10 | 6/30 | 123.726s |

P1 is unsuccessful. P2 completed without operator correction or another scope
decision after initial authorization. Both encountered one rejected shell glob;
the Agent changed subsequent commands without operator help. These tool failures
remain part of the original outcomes; a successful final answer does not erase them.
P1's budget rejection retained all spending and made no further model admission.

P2 correctly identified the CCI checks omitted from `AGENTS.md`, conditional
Wasm/font downloads, local generation versus remote R2 upload, and manual deployment
versus the repository's claim about external Cloudflare automation. Root checked
these core findings against the actual scripts. This does not verify every line
citation, run `pnpm check`/`build`, or inspect external deployment configuration.

Both worktrees remained clean; primary checkout status digests matched before and
after. All executed task commands were local source inspection. Both native
invocations exited normally without the supervisor firing. No push or deployment
command was executed; external live deployment state was not queried.

Ledger audit matched recorded Tool results (including failed attempts and P1's
plan Tool) to charged attempts, checked unit model-admission increments, monotonic
time/call consumption, unchanged limits and retained terminal spending. Totals are
16 model admissions and 15 Tool attempts, excluding project-selection controls.
Actual usage-derived monetary cost is unknown; admission counts are not a price.

## Next actions from this evidence

1. Keep these two original outcomes; do not increase their limits retrospectively.
   Ten model admissions were insufficient for the broad Alan audit. A later
   separately frozen calibration needs representative implementation/check tasks
   before daily-use numeric limits, task mix and comparison subset can be frozen.
2. Explain unsupported shell expansion at the existing bash Tool boundary. Both
   tasks used ordinary source globs rejected by path validation. Prefer explicit
   operands or quoted `rg --glob` filters; do not loosen resource enforcement to
   hide a rejected command. Verify advice on the actual active backend.
   The follow-up now adds this advice to the existing Tool description and command
   schema. A source regression executes a portable `find` filename filter under a read-only mount
   with the detected backend and the path-guard fallback, while an unquoted glob
   stays rejected. This is source verification only: a fresh native candidate must
   rerun the unchanged frozen task before claiming improved model behavior. Neither
   original pilot outcome is replaced or qualified by the guidance edit.
3. Improve source-inspection Action summaries: long `Bash` commands and an
   `exit 0` plus source-header preview obscure the useful file/range/result.
   Keep the command, full output and failures available in details/scrollback.
   Separately assess first-response latency: the current Root buffers model text
   before publishing successful-response chunks. Preserve failed-attempt semantics.
4. Continue the owning delegation/budget controls, continuity and real Node/pnpm
   acceptance. No saved-delegation, qualified reviewer or full-trust capability was
   established by this pilot. The budget qualification gaps remain open in
   [budget delivery](../archive/2026-11-21-delegate-project-autonomy/budget-delivery.md).

## Evidence

Private evidence is retained under
`~/Library/Caches/Alan/daily-use-pilot/20261023/evidence/`: frozen plan/config and
candidate bindings, original rollouts, task receipts, retained answer, terminal
reads, ledger audit and clean exit records. Earlier native cancellation/recovery
evidence remains under the `20261014` fixture, separated from these pilot tasks.
Observer failures are retained; correcting an observer did not retry a task.

## Fresh guidance candidate follow-up

Clean candidate `bec2cb85`, built after the dependency fixes, served the new Bash
description and command schema in its live namespace manifest. G1 reran P1's exact
prompt on the same source snapshot, model/effort and 600s/30-Tool/10-model limits.
It again exhausted ten model admissions with nine Tool attempts and no final
answer, after 99.193s. One unquoted source glob was still attempted and rejected.
The guidance correction therefore has no qualified model-improvement result.

The operator did not close this invocation before the predeclared 660-second
supervisor deadline. It ended with exit 143 while its task budget remained paused.
An Enter sent afterward reached the idle shell, not the Runtime; the subsequent
completed-budget assertion failed. This is retained as a harness/operator failure,
not a successful default-rejection check. No budget was extended. At this
checkpoint G2 had not started; neither original pilot outcome is replaced by this separate cohort.

CI also reproduced an undeclared test-tool prerequisite: the first quoted-filter
regression required `rg`, absent in the test/coverage jobs, and exited 127. A
restricted-PATH local run reproduced it. The corrected regression uses portable
`find` to exercise the same real quoted-filter and unquoted-expansion boundary;
it does not install another tool or relax the sandbox. Fresh CI remains required.

## Live beta dogfooding pilot — 2026-11-25 (`alan 0.1.11-beta.20261125.27`)

This is preflight live-binary dogfooding on the installed beta release (`alan 0.1.11-beta.20261125.27`, commit `9519ac4`), **not** the formal two-week daily-use cohort. Formal qualification progress remains **0 / 50 tasks** across **0 / 14 days**.

### Environment & conditions

- Installed binary: `/Users/morris/.local/bin/alan` (`0.1.11-beta.20261125.27 (beta, 9519ac4)`), `managed` install ownership, `seatbelt` sandbox backend.
- Configured profiles: `vertex-main` (`google_gemini_generate_content`, `gemini-4-neon`, default, ADC present) and `magpie-main` (`magpie`, `auto/deepseek-v4.1-flash`).
- Posture: default `Governed` mode (zero `--trust` or `--unsafe-host-account` runs).
- Isolation & cleanliness: operated against a temporary detached worktree (`wt-dogfood-qualify`), temporary saved grant (`dogfood-qualify-wt`), temporary `alan project bind` (`proj-170ba7eca820416ea65f4d3f3fbeab7a`), and `--no-resume-update` / `ALAN_NO_RESUME=1`; all temporary grants, bindings, and `alan-arena` package installation were cleaned up after testing so the operator's stores stayed pristine.

### Pilot battery outcomes

| Probe | Outcome | Observed details & telemetry |
| --- | --- | --- |
| CLI discovery (`--version`, `doctor --json`, `--help`, `q list`, `skills list`, `project --help`, `workflow draft --help`) | Passed | `doctor` reported `healthy: true`, `seatbelt`, writable stores, `2` profiles (`0` missing auth), `2024` rollouts; `6` preinstalled packages listed (`alan-arena` confirmed as installable package under `crates/service-manager/packages/alan-arena`). |
| Explicit grant & project bind (`host grant save`, `project bind --grant`, `project status`) | Passed | Bound temporary worktree to `proj-170ba7eca820416ea65f4d3f3fbeab7a` (`active_non_archived: 0`, `unread_inbox_count: 0`). |
| OneShot (`--grant dogfood-qualify-wt --no-resume-update -p ... --json`) | Passed with reproduced bug | `bash` executed `ls` inside `/mnt/grants/dogfood-qualify-wt` (`exit 0`, `30585` input / `644` output / `385` reasoning tokens, `1` tool call), and `metadata/root-rollout` stayed untouched. **Bug reproduced:** initial `SelectProjectDirectory` (`cd`) failed with `invalid absolute namespace directory` because `resolve_host_against_grants` joined an empty suffix onto `/mnt/grants/dogfood-qualify-wt`, producing a trailing `/`. |
| Interactive PTY (`openpty` 32x110, `\x1b[6n` DSR -> `\x1b[20;1R`, `--grant`, `--no-resume-update`) | Passed | Booted TUI in `0.42s`; exercised `/help`, `/streams` (workstream view), `/summary` (derived continuity summary), and a live `read_file` turn on `tasks.md`; exited cleanly via `/quit` (`exit_code=0`, `has_panic=False`, `\x1b[?2004l` bracketed paste disabled, `metadata/root-rollout` untouched). |
| Workflow drafting dry-run (`alan workflow draft --target launchd` & `--target cron`, without `--confirm`) | Passed | Rendered valid `launchd` `.plist` (with `--project-key`, `--mount`, `--stream`) and POSIX `crontab` previews plus SHA-256 confirmation digests; `alan workflow list` remained empty and zero persistent scheduler jobs were created. |
| Package lifecycle & `$alan-arena` (`q install`, `q inspect`, `q enable`, OneShot bakeoff) | Partial / exposed live bugs | `q install` + `q inspect` + `q enable` succeeded (`f5890f95`). During the live `$alan-arena` OneShot run on `gemini-4-neon`, Root hit `Invalid spawn_subagent payload.` whenever passing `host_mounts` because Gemini's `partialArgs` stream emitted `"{}": null` inside `host_mounts[0]`. Root fell back to omitting `host_mounts`; one contender (`arena-contender-consumer-contract`) completed cleanly, while `arena-contender-producer-boundary` hit a transient `alan-agent: bad request` and its retry remained stalled on Vertex past 15m before cancellation. |

### Shared-owner fixes landed from this pilot

1. **OneShot grant-root trailing slash (`crates/alan/src/foreground/oneshot_cwd.rs`)**:
   - **Root cause:** `Path::new(&grant.namespace_path).join(suffix)` appends a trailing `/` when `suffix` is empty (running from or targeting the grant root itself). Existing unit tests masked this because Rust's `PathBuf::eq` normalizes trailing slashes, while `serde_json` serialized the raw trailing `/` into `project-cwd-v1` and `valid_project_directory` rejected it.
   - **Fix & verification:** Added `join_namespace_suffix` to return `PathBuf::from(namespace_root)` unchanged when `suffix` is empty, strengthened `foreground_tests.rs` to assert exact string equality (`to_str()`), and added grant-root `current_dir` coverage in `crates/alan/tests/oneshot_grant_cwd_integration_test.rs` asserting no `cd` failure occurs.
2. **Gemini `partialArgs` empty-object / valueless markers (`crates/llm/src/google_gemini_generate_content/streaming.rs`)**:
   - **Root cause:** When Vertex AI streams `streamFunctionCallArguments` (`partialArgs`), object-container markers such as `$.host_mounts[0]{}` or `$.host_mounts[0].{}` and value-less termination markers were parsed by `parse_json_path` as `PathSegment::Key("{}")` and materialized as `"{}": null`, which `SpawnHostMount` (`#[serde(deny_unknown_fields)]`) rejected with `Invalid spawn_subagent payload.`.
   - **Fix & verification:** Updated `parse_json_path` and `apply_partial_arg` to treat unquoted `{}` / `[]` as `ObjectMarker` / `ArrayMarker` and avoid inserting spurious `null` entries for value-less `PartialArg` markers, preserving strict `deny_unknown_fields` on `SpawnHostMount`. Added unit regression `partial_args_ignore_empty_object_and_valueless_markers` in `streaming.rs`.
