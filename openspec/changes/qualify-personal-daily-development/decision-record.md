# Personal daily development: design interview

Status: interview confirmed by the user at Q16, 2026-10-14. This record is not
implementation authorization or a claim of delivered behavior. Canonical specs
continue to describe the current contract until reviewed implementation and sync.

## Confirmed scope

- Q1: Prioritize the user's personal macOS + Herdr development, using Alan and
  one real second project. Retain Linux regressions; qualify additional Linux
  toolchains only when actual project work needs them.
- Q2: Include an explicit fully trusted option in the design, with its own
  delivery and acceptance. Its precise approval and isolation semantics remain
  pending; this does not activate such a mode.
- Q3: Prioritize project continuity, history selection and explicit recovery
  across invocations. Root Agent Process remains instance-local. Background
  execution, scheduling and multi-device entry are later work.
- Q4: Delegate explicit project capabilities: project read/write, local
  build/test/commit and constrained task-relevant dependency downloads. Publishing,
  messaging and additional private-directory access require separate delegation.
  A command named test does not grant its executable effects additional authority.
- Q5: Approval posture and execution isolation are independent selections.
  Disabling per-action approvals preserves granted scope and available sandbox
  confinement by default; Host-account execution requires an additional explicit
  user selection. Agents cannot change these controls themselves.
- Q6: A user may explicitly save project delegation. Reuse requires project
  identity, scope and revocation revalidation. One-operation approval never
  silently becomes standing delegation or permission to replay prior work.
- Q7: Qualify over two consecutive weeks and at least 50 real tasks across at
  least two repositories, with at least 90% completion without unplanned rescue.
  Initial authorization and ordinary clarification are not rescue; operator
  environment/runtime repair or finishing implementation is rescue. Median
  post-authorization permission interruptions for ordinary tasks must be zero;
  record P95 interruptions, elapsed time and available usage. Cancellation,
  revocation and no-repeat controls must pass. Any observed unauthorized write,
  repeated effect or lost work fails acceptance. Dates are estimates, not gates.
- Q8: Fully trusted posture is an explicit trusted-user-interface choice for
  this invocation only. It bypasses Guardian and per-action approval red lines,
  including otherwise always-human force-push review. Host-account execution
  remains a separate explicit choice. Children inherit no broader selection;
  revocation, cancellation, audit and OS authority remain. Project files and
  Agents cannot enable the mode or modify control policy.
- Q9: Deterministically delegated routine actions need no reviewer call.
  Reviewer-dependent actions use bounded evaluation and optional main-model
  fallback; unresolved actions stay pending and ask once, while independent
  work may proceed. Timeout never authorizes an action. Qualification precedes
  enabling a new reviewer configuration.
- Q10: Retain task evidence and form project-scoped summaries of goals,
  completed/pending work and verification. Summaries are inspectable, correctable
  and deletable. Explicit history selection restores context, not old effects or
  revoked authority. No automatic cross-project memory propagation or policy
  learning.
- Q11: The second repository is `realmorrisliu.com`. Read-only inventory found
  `/Users/morris/Developer/github.com/realmorrisliu/realmorrisliu.com` with active
  dirty work. Use a separately frozen worktree; do not switch, stash or clean the
  user's checkout. It uses Astro/TypeScript/React/pnpm; main pushes deploy the
  website, so initial qualification excludes publication/deployment.
- Q12: Each task requires configurable elapsed-time, Tool-call and model-call
  budgets. Exhaustion pauses, never silently resets. Freeze numeric limits after
  a bounded pilot and before formal qualification. Report monetary cost only
  with trustworthy usage and pricing provenance.
- Q13: Host-account execution does not guarantee that arbitrary same-account
  code cannot tamper with writable control or audit files. Trusted control APIs
  restrict changes to user-origin operations, but this is not an OS isolation
  claim. Describe this distinction explicitly.
- Q14: Permit scoped advance delegation of publication. Ordinary push does not
  imply force-push or remote deletion; these are distinct capabilities. Private
  resource expansion, privilege changes and control-policy changes require the
  trusted user entry; the reviewer cannot grant them.
- Q15: Codex is the primary comparison, matched on model/input/repository
  snapshot when possible. Otherwise label a whole-product comparison. Pi is a
  supplementary experience reference. Report comparative latency/cost without
  inventing relative pass thresholds.

## Final confirmation and measurement gates

Q16 confirmed the consolidated scope, four independent changes, one ADR and a
documentation-only delivery. The decision frontier is closed. No new runtime
behavior, website edits or model task execution is authorized by this document.

Numeric task/reviewer budgets and reviewer false-denial, latency and cost gates
are explicitly deferred to a bounded pilot, then frozen before formal evaluation.
High-impact unauthorized allows must be zero in the measured corpus. A finite
pilot plan is a prerequisite; these gates do not authorize unlimited measurement.

## Repository audit at main ea929481

- `auto-approve-policy` currently requires a single Autonomous posture and
  prohibits persistent grants derived from operation approvals. New selectable
  postures and explicit saved delegation require deliberate normative deltas.
- Guardian review exists, but production uses the main generation connection
  and fixed default reviewer policy. Dedicated reviewer configuration and policy
  overrides promised by `autonomous-review-mode` are implementation gaps.
- System 1/System 2 remain work modes, not Process types or permission levels
  (ADR-0055). Evaluator output is evidence, not independent grant authority.
- `expose-agent-rollout-history` and `add-proactive-memory-v2` are parked. Their
  retained tasks do not authorize background launch or permission learning.
- Historical development qualification retains 21/30 first passes and 91 model
  attempts. Linux recovery remains assisted and delegation unqualified. A new
  sample must not overwrite those outcomes.
- PR #1057 (`df961b0c`) already synchronized repeated-development canonical
  requirements; its active change still has pre-merge closure text. Linux and
  installation canonical adoption need their own exact delta audits.
- PR #1059 (`f2a98f4b`) shipped composer refinements; canonical synchronization
  and archive follow-up remain. Existing ADR-0059 covers presentation direction.

## Confirmed document ownership

- This change owns the personal daily-use roadmap, frozen acceptance sample,
  intervention taxonomy and measured outcomes under `runtime-harness-contract`.
- A separate autonomy change and one new ADR own explicit delegation, approval
  posture, execution isolation, Guardian qualification and grant lifetime.
- A bounded continuity change owns project context/history selection and
  explicit recovery; it must replace any applicable parked history design
  explicitly, without reviving obsolete background launch paths.
- `CONTEXT.md` remains the sole glossary. Existing ADRs receive narrow refinement
  and current-delivery notes; no new global Project execution identity is implied.
- Update current explanatory docs when decisions settle. Do not document
  proposed commands as available. Automatic input routing stays separate and off.

## Completion condition

The interview and consolidated confirmation are complete. Finalize the
ADR/OpenSpec artifacts and validate them. Product implementation is a
separate step; this interview does not silently change the running installation.
