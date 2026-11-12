# Document ownership and closure map

Interview Q1–Q16 is complete. ADR-0060 is accepted direction; the four new changes
are future implementation. This branch changes documentation only. Canonical
updates below reflect previously merged behavior, not the proposed trust modes.

## Architectural decisions

| Document | Treatment |
| --- | --- |
| ADR-0060 | New decision: explicit delegation; scope/approval/isolation; Guardian; invocation-only trust; continuity without permanent Root |
| ADR-0003/0011/0022 | Add refinement pointer, preserve original reasoning and service ownership |
| ADR-0050 | Retain Host Mount Service grant ownership unchanged |
| ADR-0055 | Correct partial delivery: finite-choice evaluation and one mixed read-only program shipped; general mixed transitions unfinished |
| ADR-0056/0058 | Correct shipped foreground/recovery and product-store status; retain dated historical decisions |
| ADR-0059 | Retain presentation direction; no additional UI ADR needed |

## New changes

| Change | Owns | Dependency |
| --- | --- | --- |
| [qualify-personal-daily-development](./) | Unified roadmap, bounded pilot, sample/metrics, actual daily-use verdict | Measured final candidate consumes autonomy and continuity |
| [delegate-project-autonomy](../delegate-project-autonomy/) | Saved scope, qualified Guardian, budgets, fewer repeated prompts | Existing Host/Process/Connection owners |
| [add-explicit-trust-posture](../add-explicit-trust-posture/) | Invocation-only bypass and distinct Host-account execution | Delegation change lands and syncs first; cumulative overlapping deltas must not be applied in reverse |
| [restore-project-continuity](../restore-project-continuity/) | History selection, controlled project summaries and explicit recovery | Saved-authority and budget semantics from delegation |

Only project-autonomous tasks count toward the default daily-use score. Fully
trusted tasks have an independent cohort. Prototype/pilot thresholds are frozen
before formal qualification, never adjusted retroactively to pass observed data.

## Previously merged deliveries closed in this patch

| Archived change | Implementation evidence | Canonical closure |
| --- | --- | --- |
| [Repeated development](../archive/2026-10-14-qualify-repeated-development-workflows/) | #1047/#1056; canonical #1057 df961b0c | Verify thirteen already adopted blocks; preserve all original outcomes |
| [Linux tools](../archive/2026-10-14-support-linux-development-toolchains/) | #1046 465e3e47 | Add six missing requirements; preserve later query-provider repairs |
| [Installation/cache](../archive/2026-10-14-unify-alan-installation-and-cache-lifecycle/) | #1042 997ade6a, install case fix #1058 | Sync product-store, lifecycle/distribution/cache deltas; no new migration |
| [Composer](../archive/2026-10-14-simplify-shell-composer-status/) | #1059 f2a98f4b | Sync contextual-prompt block, preserve other requirements and407-test evidence |

These archive/sync edits were adopted by planning PR #1060 at `a4ad5992` and
rechecked against fetched main `9313b5a0`; see [the closure receipt](planning-delivery.md#post-merge-verification--2026-10-23).
They do not rerun historical model qualification. Pre-existing historical archives
are not edited.

## Existing active plans

- `qualify-agent-input-routing`: still unqualified and off; move later in the order.
- `add-cognitive-model-routing`: reuse delivered capability without reopening general mixed-Machine work.
- `unify-agent-command-input`: keep broader unchecked boundaries distinct from shipped recovery evidence.
- `expose-agent-rollout-history`: remains parked; bounded history need goes to the new continuity change, never its rejected clone/background launch path.
- `add-proactive-memory-v2`: remains parked; no automatic policy/permission learning.

## Current guides and vocabulary

Update CONTEXT.md, AGENTS.md, docs/architecture.md, docs/governance_current_contract.md
and docs/README.md for ownership, precise current-versus-target statements and
entry links. Reuse the existing glossary; do not add a second planning surface.
README.md, standalone installation instructions and harness command examples only
need new operational syntax when their corresponding implementation exists. This
planning patch intentionally advertises no invented trust-mode flags or profiles.

## External comparison context

These product references motivate the design, not claims about their internal
Process implementation. Alan retains its independent foreground invocation model.

- [Dot official guide](https://help.openai.com/en/articles/20001530-getting-started-with-your-dot): ongoing Agent work with user-defined controls.
- [Claude Projects](https://support.claude.com/en/articles/14116274-organize-your-tasks-with-projects-in-claude-cowork): project-scoped tasks, instructions and memory.
- [Claude Code Auto mode](https://claude.com/resources/articles/auto-mode): classifier-mediated action review.
- [Pi security model](https://pi.dev/docs/latest/security): local-account execution without per-tool prompts; external isolation remains relevant.

## Verification record

Use the repository's CI-pinned OpenSpec1.4.1 validator, the current-surface guard,
local-link checks and exact-delta audits. A newer local CLI may add historical
long-requirement warnings; preserve the distinction from the pinned merge gate.
Do not report runtime tests or two-week qualification as run by this doc patch.
