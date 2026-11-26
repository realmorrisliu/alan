# Proposal: First-Class Decisions API and System 1 Smart Approval

## Why

Alan's current typed evaluation surface (`evaluate_choice` and `choice.v1`) only supports single-question finite-choice classification on TypeSafe Connections. It cannot express boolean predicates, bounded numeric scores, multi-question decision batches, cross-provider decision dispatch, or explicit calibration provenance (`Calibrated` vs `Uncalibrated`).

At the same time, operators working inside an active OS confinement boundary currently face a binary posture choice between `Governed` (`rw·gov`, which pauses for human or System 2 review on boundary-crossing escalations) and `ConfinedTrusted` (`rw·trust`, which bypasses all per-action policy and red lines). A calibrated System 1 smart approval posture (`ConfinedSmart` / `rw·smart`) closes that gap inside OS confinement while keeping evaluation strictly distinct from authority (ADR-0055, ADR-0060, ADR-0061).

## What Changes

### PR 3 — First-Class Decisions API (`alan-llm` and `alan-llmfs`)

- Add provider-neutral typed decision models to `alan-llm`: `DecisionQuestion` (`Predicate`, `Choice`, `Score`), `DecisionRequest`, `DecisionAnswer`, `DecisionResponse`, `CalibrationKind` (`Calibrated` and `Uncalibrated`), and `MalformedDecisionResponse`.
- Extend `LlmProvider` and `SharedLlmProvider` with `decide()`, `supports_decisions()`, and `decision_calibration()`, while preserving existing `evaluate_choice()` and `supports_choice_evaluation()`.
- Implement native calibrated (`CalibrationKind::Calibrated`, zero output tokens) Decisions endpoints across `OpenRouterClient` (`client.decisions().create()` at `POST /api/alpha/decisions`), Google Gemini / Vertex AI Decisions (native `POST /v1beta/decisions` for `gemini-decision-1`), OpenAI (native `POST /v1/decisions`), `TypesafeEvaluationClient` (`POST /v1/systemone`), and `MagpieClient` (`/v1/decisions` passthrough), alongside a reusable `FallbackDecisionEvaluator` / `decide_via_generation()` (`CalibrationKind::Uncalibrated`, never claiming `Calibrated`) for generation-only providers (`anthropic_messages`, `openai_chat_completions`, `chatgpt_responses`, and Magpie chat fallback).
- Extend `alan-llmfs` with `connections/<conn>/decide` (clone-via-open, ReadWrite, versioned `decision.v1` wire document, commit-on-clunk, typed `events`, `ctl` abort, `status`), decision capability fields in `providers/<provider>/capabilities` and `connections/<conn>/capabilities`, and independent `ConnectionLimits::max_decisions` metering (`decision_starts`) in `connections/<conn>/meter`.

### PR 4 — System 1 Smart Approval (`ConfinedSmart` / `rw·smart`)

- Add invocation-only `TrustPosture::ConfinedSmart` strictly between `Governed` and `ConfinedTrusted` (`Governed < ConfinedSmart < ConfinedTrusted < HostAccountTrusted`), never persisted to or parsed from repository files, instruction files, rollout history, or AgentFS control writes.
- Route reviewer-judged escalations inside active OS confinement through `connections/<conn>/decide` using calibration-aware thresholds (`Calibrated` vs `Uncalibrated`), keeping deterministic deny and always-human red lines outside System 1 and failing closed to a correlated human request on timeout, unavailability, malformed evidence, stale authority, or low confidence.
- Preserve ADR-0061's OS-boundary rule: effects already contained by the active OS sandbox run without invoking System 1, and System 1 verdicts never widen Host Mount grants or disable OS confinement.
- Surface `rw·smart` / `ro·smart` (ASCII `rw/smart` / `ro/smart`) in the terminal status bar, allow trusted interactive cycling among confined postures without promoting to `HostAccountTrusted`, and disclose active evaluator profile, calibration kind, and decision rationale in status/receipts.

## Capabilities

### New Capabilities

None. All behavior belongs to existing canonical capability owners.

### Modified Capabilities

- `provider-connection-contract`: First-class typed Decisions API (`Predicate`, `Choice`, `Score`), `CalibrationKind`, `LlmProvider` trait surface, and honest adapter/fallback calibration semantics (PR 3).
- `openrouter-provider-adapter`: OpenRouter native calibrated `POST /api/alpha/decisions` wire mapping (`client.decisions().create()`, zero output tokens, `CalibrationKind::Calibrated`) and strict validation (PR 3).
- `llm-file-server`: `connections/<conn>/decide` endpoint, `decision.v1` wire documents, capability fields, and independent `max_decisions` metering (PR 3).
- `auto-approve-policy`: Invocation-only `ConfinedSmart` posture rank, calibration-aware thresholds, red-line bypasses, and fail-closed human fallback (PR 4).
- `os-boundary-security`: Smart approval strictly inside active OS confinement without reviewing contained actions or treating decision scores as grants (PR 4).
- `autonomous-review-mode`: System 1 smart approval reuse of the no-tools reviewer boundary, untrusted-transcript framing, stale-authority checks, and circuit breaker (PR 4).
- `rust-inline-tui`: `rw·smart` / `ro·smart` status-bar indicator, confined posture switching, and evaluator/calibration status disclosure (PR 4).

## Impact

- **Delivery split**: PR 3 touches `crates/llm` and `crates/llmfs` (plus OpenSpec); PR 4 builds on PR 3 across `crates/agent-engine`, `crates/agent-protocol`, `crates/os-host`, `crates/alan`, and `crates/tui`.
- **Ownership**: Reuses existing Connection Service, Host credential store, `alan-llm`, `alan-llmfs`, `PolicyEngine`, `TrustPosture`, and file-backed TUI owners. Introduces no new Kernel Process type, no global execution manager, and no implicit authority widening.
- **ADR alignment**: Advances ADR-0055 (composable typed evaluation where evaluation is evidence, not authorization), ADR-0060 (separation of scope, approval posture, and isolation), and ADR-0061 (OS boundary as security boundary, four first-party providers, standard-PTY verification).
