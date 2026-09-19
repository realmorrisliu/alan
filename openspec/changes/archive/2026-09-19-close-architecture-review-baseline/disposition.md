# Baseline disposition — 2026-09-19

This change closes documentation and product decisions, not runtime delivery.
The review report remains the detailed evidence snapshot under
`add-cognitive-model-routing/architecture-review.md`.

## Existing change inventory

| Change | Disposition | Next entry gate |
| --- | --- | --- |
| add-cognitive-model-routing | Rewritten direction; parked | Select bounded slice and complete llmfs/Machine deltas on merged main |
| expose-agent-rollout-history | Parked/redesign | Remove renderer launch exception; separate history from authority |
| define-alan-interaction-model | Parked/redesign | Terminal product, no native client or renderer execution owner |
| define-alan-programmable-client-surface | Parked/split | Real Shell evaluator and incremental Process IO first |
| add-proactive-memory-v2 | Parked/rebase | Machine evidence and System Store commit ownership |
| add-alan-anywhere-mvp | Parked/reassess | Demonstrate unmet need beyond existing terminal access |
| define-updf-product-umbrella | Parked | Select actual surviving consumer |
| define-groove-master-alan-app | Parked | Select actual surviving consumer |
| spike-macos-matter-controller | Parked | Separate platform owner and concrete hardware request |
| add-macos-shell-component-system | Cancelled archive | No desktop feature continuation |
| add-macos-app-auto-update | Cancelled archive | No remaining App release delivery claim |
| verify-macos-managed-user-pty | Cancelled archive | Security validation, if needed, moves to surviving sandbox owner |
| add-alan-voice-mvp | Cancelled archive | Any future voice input requires a new consumer-based proposal |

Cancelled archives use `archive/2026-09-19-<change>/` and retain incomplete
task states. No deltas are synced from those cancellations. Some updater
requirements already existed in canonical specs before this closure; they are
maintenance-only and were not newly delivered here.

## Outstanding implementation boundaries

- Desktop source/build/release removal needs consumer and security-adapter
  inventory plus explicit requirement-removal deltas. This closure retains code.
- Bare alan currently evaluates locally; real Shell Process evaluator, correct
  runner wiring, incremental IO and bounded Local Entry retention remain gaps.
- Canonical generation contracts remain current; mixed operations need owning
  runtime/llmfs deltas and recoverable state before implementation.
- q v0 installs Skills; binfs/executable/system-package delivery is not shipped.
- Herdr 0.9.1 did not advertise an Alan kind in the inspected CLI. Standard
  terminal compatibility and optional status integration need separate evidence.
- Model judgments never grant authority; namespace checks do not replace
  syscall sandboxing; Unknown external effects cannot be blindly replayed.

## Next planning order after merge

1. Scoped desktop source retirement and standalone CLI/Host distribution.
2. Machine/evidence and typed operation contracts, with one bounded use case.
3. Shared Shell evaluator/Process IO and terminal lifecycle acceptance.
4. Measured Jev vertical slice and optional Herdr integration.

Do not infer implementation approval for all parked initiatives from this list.
