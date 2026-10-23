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
  Host/System Store bindings, bare foreground invocations through Herdr.
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
   schema. A source regression executes the quoted filter under a read-only mount
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
   [budget delivery](../delegate-project-autonomy/budget-delivery.md).

## Evidence

Private evidence is retained under
`~/Library/Caches/Alan/daily-use-pilot/20261023/evidence/`: frozen plan/config and
candidate bindings, original rollouts, task receipts, retained answer, terminal
reads, ledger audit and clean exit records. Earlier native cancellation/recovery
evidence remains under the `20261014` fixture, separated from these pilot tasks.
Observer failures are retained; correcting an observer did not retry a task.
