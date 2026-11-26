# Spec Delta

## ADDED Requirements

### Requirement: OpenRouter native calibrated Decisions endpoint
`OpenRouterClient` SHALL implement `LlmProvider::decide()`,
`supports_decisions()`, and `decision_calibration()` via OpenRouter's native
calibrated Decisions API (`client.decisions().create()` at
`POST /api/alpha/decisions`) with zero output tokens over untrusted `state`.
Validated native responses SHALL be stamped `CalibrationKind::Calibrated`;
malformed payloads SHALL fail with `MalformedDecisionResponse` without falling
back to chat generation.

#### Scenario: OpenRouter executes a native calibrated decision
- **WHEN** `OpenRouterClient::decide()` evaluates `Predicate`, `Choice`, and
  `Score` questions via `client.decisions().create()` at
  `POST /api/alpha/decisions`
- **THEN** the adapter sends the typed decision payload with zero tool
  definitions and zero generated output tokens
- **AND** validates every probability distribution, expected score, and
  confidence bound before returning a `DecisionResponse` stamped
  `CalibrationKind::Calibrated`

#### Scenario: OpenRouter returns malformed native decision payload
- **WHEN** `POST /api/alpha/decisions` returns invalid JSON, missing or extra
  question keys, an unknown option or level ID, non-finite probabilities, or a
  broken probability distribution
- **THEN** `OpenRouterClient::decide()` fails with `MalformedDecisionResponse`
- **AND** does not silently coerce the answer or fall back to chat completions
  generation

#### Scenario: OpenRouter explicit uncalibrated fallback remains distinct
- **WHEN** a caller explicitly evaluates a `DecisionRequest` through
  `FallbackDecisionEvaluator` / `decide_via_generation()` over a generation-only
  chat route instead of native `POST /api/alpha/decisions`
- **THEN** the resulting `DecisionResponse` is stamped
  `CalibrationKind::Uncalibrated` and never claims `Calibrated`
