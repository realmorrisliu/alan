# Remaining checklist audit — 2026-10-07

Baseline `59855ebe`; source inspection plus the local/product checks indexed in
[next-delivery.md](next-delivery.md). An unchecked parent is not evidence of
missing implementation. This inventory distinguishes observed implementation
from unqualified combinations; it does not claim all listed matrices passed.

| Task | Current finding | Remaining work / classification |
| --- | --- | --- |
| 2.1 | Versioned input and correlated Tape/terminal outcomes exist; `stdio_tests` checks identical-prompt and unrelated-result exclusion | Acceptance gap: simultaneous authorized clients of the same Agent, including independent result consumption |
| 2.2 | Shared intent framing and TUI/stdio prefix handling are implemented; compact prompt shipped | Closure audit: map all empty/nested/slash/pending/EOF/multiline clauses to owning checks; no new parser justified |
| 2.3 | `explicit_command` Host integration executes pipelines, multiline scripts, redirection, relative cwd, partial failure and bounded cd | Closure audit: selected shell/environment/PATH cases and unsupported standalone forms; reuse adapter |
| 2.4 | Same governed Tool Process boundary; current native exit and cancelled descendant evidence exists | Acceptance gap: complete cross-grant scope and explicit-versus-generated authority matrix |
| 2.5 | `engine_input_order_tests` covers FIFO/control boundaries; Process cwd and cross-invocation isolation shipped | Acceptance gap: two clients sharing one Agent and ordered cross-grant cwd changes |
| 2.6 | Targeted cancellation, pause, continue/discard and pending-response handling exist; current native E/Q evidence verifies pause and continuation | Closure audit: pre-start/active/unsettled/pending-response clauses, including durable removals |
| 2.7 | Existing Host test verifies retained stdout/stderr/exit and recovered Action; bounded context/output projection exists | Acceptance gap: large-output truncation, retained-reference loss and later Agent question in one trace |
| 2.8 | Recovery owner and Engine tests exist; actual resumed Q stayed paused with fresh authority required and no replay | Implemented, bounded product acceptance complete; retain missing/invalid/unknown-case test references in delivery record |
| 2.9 | Foreground exit, redirected result correlation and no-response failures exist; actual Ctrl-D terminated owned descendants | Closure audit: pending confirmation/structured-input Ctrl-D and redirected missing-channel cases; view-client closure now verified |
| 2.10 | Focused recovery/directory/Host checks and real Herdr lifecycle/recovery passed | Delivery gate: full quality, exact review and CI; unprefixed input remains Agent-routed |
| 2.11 | Private adapter path resolution and output projection already implemented | Acceptance gap: complete generated-metadata versus ordinary-content disclosure matrix; path text never grants authority |
| 2.12 | macOS/native and Linux reified/path-guard adapters and tests exist | Platform qualification gap: Linux namespace/mount/network smoke requires capable Linux host; macOS tests cannot certify it |
| 2.13 / 2.13.2 | `runtime/tests/agent_work.rs` discovers Tool from namespace manifest and invokes it through a Process; `agent_work/commit_tests.rs` covers commit before/after error without retry | Implementation exists, checklist stale; rerun owning checks and map invocation/fault outcomes before closing parent |
| 2.14 | Host integration verifies Agent write/edit → native read/diff, native write → Agent read/search and stale-edit rejection | Acceptance gap: full noncurrent-grant, pending-save, RO/symlink/revoke combination matrix |
| 2.15 | `agent_work::HELP` and manifest explicitly explain local root/self-scheduling, no external notification, acceptance versus completion; regression exists | Implementation complete for requested help correction; normal-flow terminology audit and delivery references remain |
| 2.19 | Full clause matrix now includes real SIGKILL recovery/unknown warning, no repeated U/Q effects, missing/invalid boot rejection and real one-shot shutdown | Implementation and bounded acceptance closed; current-head delivery and canonical sync remain 4.1/4.2. Harness cleanup is not an Alan cancellation receipt |
| 2.19.2 | Native chooser controls recovered cwd without dispatching Q; existing directory tests cover invalid/revoked/running controls and lost replies | Closed implementation/acceptance in tasks; final slice review/CI still tracked by 4.1 |
| 2.20 | Captured callable admission and real ChatGPT catalog shipped in merged PR #1033; native success/failure/queued-original-model acceptance passed | Closed by the clause-by-clause audit in chatgpt-catalog-delivery: exact recovery, unavailable binding, resolved controls and pause boundaries passed fresh owning checks; canonical sync remains 4.2 |
| 4.1 | Lifecycle #1032 and model #1033 merged with successful checks and local independent review; exact identities are in next-delivery | Broader slices retain their own review/CI/merge gates; UI #1034 and shadow #1035 are still open |
| 4.2 | Earlier shipped startup/isolation/UI contracts were synchronized | Sync only newly qualified implemented scope after delivery; no automatic-routing guarantees |
| 4.3 | Broad contract still has outstanding qualification | Keep change active; do not archive or silently transfer unchecked guarantees |

## Delivery order

Finish the phase-1 lifecycle slice and its exact evidence review first. Remaining
cross-client, cross-grant, retention and Linux matrices stay explicitly owned
here; absence of a new failure is not their qualification. Phase 2 addresses the
concrete live-catalog gap, phase 3 the requested bounded rendering improvements,
and phase 4 shadow evaluation under its existing owners. No stage enables
unqualified automatic command execution.

Current delivery refresh: see [exact merged heads and remaining boundaries](next-delivery.md#current-delivery-reconciliation--2026-10-07). The original inventory baseline remains unchanged.
