# Evaluation recovery entry — 2026-10-07

This slice adds the Machine-owned reader for `machine_evaluation_v1` rollout
EventRecords and publishes its recovered observation at Agent Runtime Service
startup, before declaring the Process ready. It does not yet create live
evaluations or write those events on the production input path.

The event payload is a typed observation. Immutable identity contains the source
rollout and submission, original-input SHA-256, admission surface, operation ID,
captured Connection identity, `choice.v1` schema, deadline and finite candidates
including their descriptions. Outcome is started, selected, no-match, unavailable,
malformed, timed-out, cancelled or interrupted. Elapsed time and input/output usage
remain optional when unknown; terminal outcomes other than interrupted require
elapsed time. Billing remains unknown rather than inferred from token counts.

The reducer validates the finite candidate set and selection using the existing
LLM boundary validator. It rejects missing start evidence, changed identity,
conflicting terminal outcomes and a restart after a terminal outcome. Exact
repeats of an observation's current acknowledged state are idempotent, including
late repeats after a newer observation; they do not move the latest projection. The latest acknowledged observation is projected;
an unfinished started record is shown as interrupted without inventing terminal
evidence. Repeated explicit recovery retains the originating identity and source
records. No provider, Tool, command, fallback or queue-dispatch API is called by
this reader. Ordinary pending inputs retain their independent paused lifecycle.

Recovery fixtures use real temporary rollout writers and two successive fresh
Machine recoveries. They cover both selected command advice and interrupted
observations, preservation of the original identity, unknown measurements and
cost, no new evaluation records, and no consumption of pending input. A separate
malformed/conflict fixture checks that cancellation cannot be overwritten with
late success and that provider/model binding cannot change under one observation.
These fixtures do not establish live evaluation dispatch, actual provider timing,
or the frozen two-surface qualification. The producer's pre-dispatch/terminal
barriers and its connection to ordinary input remain unfinished.

Validation: all three focused evaluation recovery tests and 55 engine recovery
tests passed. The Service Manager suite passed 142 tests. Independent Spec and
Standards review passed after fixing late duplicate records moving the latest
projection backwards. Full repository checks remain enforced by the commit hook.
