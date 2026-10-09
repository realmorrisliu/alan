# Native routing qualification v4 — 2026-10-08

## Decision and delivery boundary

**Qualification failed; automatic routing remains OFF.** This delivers improved
shadow criteria, matched native baselines and a Connection identity fix, not an
automatic dispatcher, activation control or qualification approval. PR #1040 is
the delivery vehicle; merge and current-head CI remain separate gates.

Implementation `07b8c351e54e6b41a6e39aa38ee2d6bd8b390db2` was independently
reviewed on both Spec and Standards axes. Freeze
`dac7802f` commits [candidate v4](shadow-candidate.v4.json) before measurement.
Fixture SHA-256 is `be8d92c262b244d56fc6fca5fbf389ec8db13db89f70b630c5d9148af500e65f`.
All 13 source hashes, three binary hashes, original corpus/budgets, profile
settings and medium generation effort match the frozen inputs. The build receipt
links the source to actual binary bytes; the collector's
`runtime_build_source_verified=false` remains unchanged and is not advertised as
an automatic source-verification feature.

## Complete attempted matrices

The audit verifies each Cartesian matrix of cases × two surfaces × three repeats,
without duplicate or missing attempted slots:

| Baseline | Ordinary attempts | Interactive responses observed | Actual unsupported redirected responses | Total retained |
| --- | ---: | ---: | ---: | ---: |
| TypeSafe jev-1.13.0 | 300 | 12 | 12 | 324 |
| ChatGPT gpt-6.1-sol medium | 300 | 12 | 12 | 324 |
| Native prefix, no evaluator | 300 | Not included | Not included | 300 |

All **948 planned attempts** are retained: 924 ordinary/response observations and
24 unsupported attempts. Each model baseline has 264 evaluated ordinary slots,
36 explicit overrides and 12 observed interactive response bypasses. Bypasses
have zero evaluator operations; pending response text, including `!` and `:`, is
preserved literally in its answered request. Unsupported redirected clients
actually report `needs interactive input` with a pending request: the response
was not admitted, and is not replaced by synthetic aP writes or a fabricated
successful route. Prefix's matrix excludes response slots.

No collection failed in v4 and no attempt was retried. The v3 generation failure
remains intact. Every completed receipt has an empty Tool registry, no Host
project mount and correlated completion. Classification enables no Tool effect
or Host-project mutation; ordinary completion uses the same fixed mock across
baselines. The generation facade exists only in the harness, with one nested
captured generation operation per evaluated input. Its terminal done event,
closed tail, terminal cleanup status, actual profile/model, input and usage were
checked. This does not add a shipped ChatGPT choice capability.

## Matched measured results

All rows below use the same **264 non-bypass ordinary slots**. Full-window timings
include client input, admission, Machine processing, durability and fixed mock
completion. Ready/completion handshakes exclude Host boot and Process exit;
model-component timing remains a separate diagnostic.

| Metric | TypeSafe jev-1.13.0 | gpt-6.1-sol medium | Native prefix |
| --- | ---: | ---: | ---: |
| Correct evaluated classifications | 230/264 (87.12%) | 249/264 (94.32%) | 162/264 (61.36%) |
| False command classifications | 8 | 0 | 0 |
| Command recall | 100% | 100% | 0% |
| Agent recall | 86.42% | 94.44% | 100% |
| Ambiguous recall | 77.78% | 88.89% | 0% |
| Valid model selected/NoMatch results | 264/264 | 264/264 | Not a model evaluator |
| Full-window p50 / p95 | 1154.36 / 1296.10 ms | 3604.94 / 5967.82 ms | 30.27 / 61.63 ms |
| Model-component p50 / p95 | 1118 / 1258 ms | 3572 / 5945 ms | No evaluator |
| Mean routing model cost | 26.12782 microUSD list-price estimate | Unknown subscription per-call billing | Zero evaluator calls |

The TypeSafe estimate uses actual recorded input tokens and the current
[official Jev 1.13 pricing](https://docs.typesafe.ai/models): $0.042 per million
input tokens, free output, rechecked 2026-10-08. It is not an invoice. Generation
billing stays null; no OpenAI alias price, zero-cost assumption or token-price
invention is substituted. Typed full p95 is 78.28% lower than generation in the
matched window, but unknown relative mean cost blocks efficiency qualification.

## Gates and remaining work

The frozen zero false-command, 95% accuracy, 90% each-class recall, 300/1000 ms
full-window p50/p95, real evidence, parity and relative cost gates are unchanged.
The 264-slot comparison is descriptive; it is not a passing full 324-slot score.

- Typed has eight false command classifications, inadequate recall and excessive
  full-window latency. Compared with v2, its evaluated accuracy rises from 75.76%
  to 87.12%, and false command choices fall from 34 to eight. Those choices remain
  blocking evidence, not executed commands.
- Generation has zero false command choices in this frozen sample, but ambiguous
  recall is 88.89% and full latency exceeds both absolute limits. This sample is
  not a general guarantee of safe execution classification.
- Twelve unsupported response slots per model prevent interactive/redirected
  response parity and a truthful complete passing numeric score.
- Unknown generation billing prevents the required relative cost comparison.
  No activation authorization was requested because qualification failed.

Further work stays within this active change: refine mutually exclusive intent
and uncertainty criteria without case-ID allowlists; address model/transport
latency rather than renderer overhead (measured model p50 already exceeds one
second); define an actual redirected response path and obtain verifiable cost
provenance for the chosen generation binding. Freeze a new candidate and repeat
all gates before considering activation, visible command presentation or owning
spec synchronization. Preserve the explicit `!`/`:` and unprefixed Agent baseline.

## Evidence receipts

Cache: `~/Library/Caches/Alan/shadow-candidate-v4-20261008/`, containing frozen
source/build receipt, copied binaries, all reports/terminals/timings/handshakes,
profile and nested generation receipts, read-only summaries and matrix audit.

| Receipt | SHA-256 |
| --- | --- |
| Build receipt | `fcd4f6c83ad2424111ec80bda68258c71e98b75a1833083aab942d6526f4d17a` |
| Matrix audit | `1fa648cb2705b42535f8323aaf2ebd24b8aa5592c2f804c0923a831a7d883750` |
| Typed ordinary manifest | `cd1d9b10fd465692c841b02ca4971c06d987def5f301769f5df473292211e99d` |
| Typed pending manifest | `4d5e4a1c19bd67c21e7764a8619cdf5950382fb44ef6584fa7557581f84304ca` |
| Generation ordinary manifest | `e7fb303152bfb07e29565554886ea6900cc7b2aa85bb39f13661100eb55ba67a` |
| Generation pending manifest | `2b38a37178a7481c069aca028c098a240a0282b4ff791507820854ce625d1c23` |
| Prefix manifest | `35645a584937e06bfff456c1ff22d026accc073ae632012361c9482bac708ca0` |

Local runtime validation: workspace 2862 passed / zero failed / 14 existing
credential/platform-gated ignored; final eight-scenario public-path regression
passed; five example tests, four collector tests and scorer self-check passed.
The missing-model and default-model defects each failed before the fix, then
passed with the shared publication change. Mandatory quality, architecture,
standalone CLI/distribution and current-surface checks passed on source/freeze
commits. Exact final delivery head, independent evidence review and required CI
must be recorded separately before merge; this receipt claims no merge or activation.

## Post-measurement reader correction

Final automated review identified a terminal-event/status publication race in the
shared generation baseline reader. It now waits for terminal status within the
existing caller deadline instead of classifying the first snapshot. A real llmfs
protocol fixture forces the first snapshot to remain `running` (red before the
fix, green after); persistent nonterminal status still times out without a second
generation, and the event tail is closed. Both baseline callers share this fix.
All v4 generation receipts already contain terminal `done` snapshots, so the
frozen measurements above remain historical evidence for `07b8c351`, unchanged.
They do not qualify the modified reader: future measurement requires a new freeze.
Shared result validation also receives each caller's allowed labels: the
three-label component baseline rejects `none`, while the native choice facade
allows NoMatch. The contract regression fails before this correction and passes
afterward; no frozen v4 result or classification criterion is changed.
