# Spec Delta

## MODIFIED Requirements

### Requirement: Provider capabilities and degradation are explicit
alan SHALL document and expose provider capabilities so product/runtime code can
branch on capability metadata instead of ad hoc provider-name checks.

Provider support tiers:

- Tier A full-fidelity stateful providers: `openai_responses`, `chatgpt` where
  live validation confirms support.
- Tier B full-fidelity stateless providers: `openai_chat_completions`,
  `anthropic_messages`.
- Tier C best-effort compatibility providers:
  `openai_chat_completions_compatible`, `openrouter`.

Minimum capability matrix fields:

```text
supports_streaming_text
supports_streaming_tool_calls
supports_provider_response_id
supports_provider_response_status
supports_reasoning_text
supports_reasoning_signature
supports_reasoning_effort_control
supports_redacted_thinking
supports_multimodal_input
supports_document_input
supports_cached_token_usage
supports_server_managed_continuation
supports_background_execution
supports_retrieve_cancel
supports_provider_compaction
supports_provider_managed_tools
supports_decisions
decision_calibration
compatibility_tier
instruction_role
```

Every capability mismatch uses one of four strategies:

1. preserve with the provider-native representation
2. emulate intentionally in alan
3. reject with a first-class error
4. drop with warning, only for Tier C compatibility providers or clearly
   non-critical metadata

Silent degradation is forbidden for tool semantics, continuation semantics,
multimodal or document inputs on official providers, reasoning-signature
continuity, explicit reasoning-effort controls, and decision calibration
provenance.

#### Scenario: Provider lacks a requested capability
- **WHEN** a request depends on a provider capability that the selected profile
  does not support
- **THEN** alan preserves, emulates, rejects, or warns according to this
  degradation contract
- **AND** it does not silently drop machine-relevant behavior

#### Scenario: Product code needs provider behavior
- **WHEN** product or runtime code needs to know whether a provider supports a
  feature
- **THEN** it reads the capability matrix or provider-specific OpenSpec owner
  rather than spreading ad hoc provider string checks

#### Scenario: Decision calibration is queried
- **WHEN** runtime code inspects `supports_decisions()` and
  `decision_calibration()` on an `LlmProvider` or `SharedLlmProvider`
- **THEN** the adapter reports whether `decide()` is supported and whether its
  baseline probabilities are `Calibrated` or `Uncalibrated`
- **AND** an uncalibrated structured-output evaluator never advertises
  `Calibrated`

## ADDED Requirements

### Requirement: Providers expose typed decision evaluation
`alan-llm` SHALL expose `decide()`, `supports_decisions()`, and
`decision_calibration()` on `LlmProvider` and `SharedLlmProvider` for
`Predicate`, `Choice`, and `Score` questions over untrusted JSON `state`
without prose or Tool calls. Existing `evaluate_choice()` and
`supports_choice_evaluation()` SHALL remain supported (delegating via
`DecisionQuestion::Choice` when appropriate). Invalid answers SHALL fail with
`MalformedDecisionResponse`.

#### Scenario: Native calibrated Decisions endpoint answers multi-question decision
- **WHEN** a caller submits a `DecisionRequest` containing `Predicate`,
  `Choice`, and `Score` questions to a native calibrated Decisions endpoint
  (OpenRouter `POST /api/alpha/decisions`, Google Gemini / Vertex AI Decisions
  `POST /v1beta/decisions` for `gemini-decision-1`, OpenAI `POST /v1/decisions`,
  TypeSafe `POST /v1/systemone`, or Magpie `/v1/decisions` passthrough)
- **THEN** the adapter dispatches one bounded no-tools native Decisions request
  with zero output tokens and Host-managed credentials
- **AND** returns validated 1:1 typed answers stamped
  `CalibrationKind::Calibrated`

#### Scenario: Provider returns out-of-set choice or invalid distribution
- **WHEN** an upstream response selects an option ID outside the request, omits
  a question key, returns a `NaN`/infinite number, or returns probabilities
  outside `[0.0, 1.0]` or violating simplex/expected-score consistency
- **THEN** `decide()` fails with `MalformedDecisionResponse` without implicit
  retry or prose fallback
- **AND** no malformed answer or option expansion is returned as evidence

### Requirement: Decision calibration provenance is explicit and honest
Every `DecisionResponse` SHALL carry `CalibrationKind::Calibrated` or
`CalibrationKind::Uncalibrated`. `Calibrated` SHALL be stamped only by native
calibrated Decisions endpoints with zero output tokens (OpenRouter, Gemini,
OpenAI, TypeSafe, Magpie passthrough). `FallbackDecisionEvaluator` and
`decide_via_generation()` over generation-only providers SHALL ALWAYS report
`Uncalibrated` and NEVER claim `Calibrated`.

#### Scenario: FallbackDecisionEvaluator wraps a generation-only provider
- **WHEN** `FallbackDecisionEvaluator` or `decide_via_generation()` evaluates a
  `DecisionRequest` over a generation-only `LlmProvider` (`anthropic_messages`,
  `openai_chat_completions`, `chatgpt_responses`, or Magpie chat fallback)
- **THEN** it frames `state` as untrusted data, disables tools, enforces a
  strict JSON Schema matching the submitted questions, and validates all
  probability and score invariants
- **AND** stamps `CalibrationKind::Uncalibrated` on the response and never
  claims `Calibrated`

#### Scenario: Generation-based fallback never promotes self-reported numbers to Calibrated
- **WHEN** `decide_via_generation()` receives valid structured JSON containing
  model-generated `yes_probability`, `probabilities`, or `confidence` values
- **THEN** the returned `DecisionResponse` reports
  `CalibrationKind::Uncalibrated`
- **AND** self-reported JSON probabilities or confidence numbers are never
  labeled `Calibrated`
