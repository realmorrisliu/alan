# Design: First-Class Decisions API and System 1 Smart Approval

## Context

`alan9` models each Agent Process as an AI Turing Machine composing deterministic code, typed evaluation, generation, governed effects, and waiting (ADR-0055). Prior delivery shipped a narrow finite-choice operation (`evaluate_choice` / `choice.v1`) backed solely by `TypesafeEvaluationClient`. Real decision consumers — starting with System 1 smart approval (`ConfinedSmart`), and later bounded routing, classification, and risk scoring — need three typed question primitives (`Predicate`, `Choice`, `Score`), honest probability calibration metadata (`Calibrated` vs `Uncalibrated`), multi-provider support with an explicit fallback evaluator, and an independent `alan-llmfs` file surface (`connections/<conn>/decide`).

Separately, ADR-0060 and ADR-0061 establish three orthogonal axes — delegated scope, approval posture, and OS execution isolation — and require that inside active OS confinement the OS sandbox and explicit Host Mount grants enforce the security boundary. Today's postures (`Governed`, `ConfinedTrusted`, `HostAccountTrusted`) leave no middle ground for operators who want fast System 1 triage of uncontained/reviewer-judged actions inside the OS sandbox without disabling red lines or approval gates altogether.

## Goals / Non-Goals

### Goals
- Provide a provider-neutral typed Decisions API in `alan-llm` covering `Predicate`, `Choice`, and `Score` questions with explicit `CalibrationKind` (`Calibrated` | `Uncalibrated`).
- Support native calibrated Decisions endpoints (`OpenRouterClient` via `client.decisions().create()` at `POST /api/alpha/decisions`, Google Gemini native `POST /v1beta/decisions` for `gemini-decision-1` / Vertex AI Decisions, OpenAI native `POST /v1/decisions`, TypeSafe `/v1/systemone`, and Magpie `/v1/decisions` passthrough — all `CalibrationKind::Calibrated` with zero output tokens) alongside explicit `FallbackDecisionEvaluator` / `decide_via_generation()` (`CalibrationKind::Uncalibrated` structured fallback for generation-only providers such as `anthropic_messages`, `openai_chat_completions`, `chatgpt_responses`, and Magpie chat fallback) without ever mislabeling uncalibrated output as `Calibrated`.
- Expose decisions through `alan-llmfs` at `connections/<conn>/decide` using clone-via-open, commit-on-clunk `decision.v1` wire documents, capability reporting, and independent `max_decisions` quota/metering.
- Deliver invocation-only `TrustPosture::ConfinedSmart` (`rw·smart`) in PR 4 on top of `connections/<conn>/decide`, enforcing calibration-aware thresholds, deterministic deny and always-human red lines, OS-boundary containment skip, and fail-closed human escalation.
- Keep PR 3 and PR 4 cleanly separable so PR 3 lands and verifies independently before PR 4 starts.

### Non-Goals
- No automatic input routing (`qualify-agent-input-routing` remains deferred research).
- No model verdict, probability, or score may widen Host Mount grants, disable OS confinement, or mutate `TrustPosture` or control policy.
- No implicit persistence of `ConfinedSmart` in `agent.toml`, `AGENTS.md`/`CLAUDE.md`, rollout files, or AgentFS control writes.
- No removal of existing `evaluate_choice()` / `connections/<conn>/evaluate` (`choice.v1`) during PR 3/PR 4; existing callers remain functional.
- No new Kernel Process type, background daemon, or global approval cache.

## Part I (PR 3): First-Class Decisions API (`alan-llm` and `alan-llmfs`)

### 1. Core Domain Types (`crates/llm`)

`crates/llm` exposes typed decision primitives (defined in `crates/llm/src/decision.rs` and re-exported from `crates/llm/src/lib.rs`) decoupled from generation prose and tool calls:

```rust
/// Whether reported probabilities/confidences come from a dedicated native
/// calibrated Decisions endpoint (zero output tokens) or an uncalibrated
/// structured-generation fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationKind {
    Calibrated,
    Uncalibrated,
}

/// One finite option in a `DecisionQuestion::Choice`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionOption {
    pub id: String,
    pub description: String,
}

/// One discrete scored level in a `DecisionQuestion::Score`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionLevel {
    pub id: String,
    pub description: String,
    pub score: f64,
}

/// Typed question kinds accepted by `LlmProvider::decide`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionQuestion {
    /// Boolean yes/no judgment over untrusted state.
    Predicate {
        prompt: String,
    },
    /// Selection from unique bounded option IDs (ASCII `[A-Za-z0-9._-]`, len 1..=128).
    Choice {
        prompt: String,
        options: Vec<DecisionOption>,
    },
    /// Discrete expected-value rating over unique finite scored levels.
    Score {
        prompt: String,
        levels: Vec<DecisionLevel>,
    },
}

/// Bounded multi-question decision request; `state` is untrusted JSON data, never instructions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionRequest {
    pub state: serde_json::Value,
    pub questions: BTreeMap<String, DecisionQuestion>,
    pub instructions: Option<String>,
    pub model_override: Option<String>,
}

/// Typed answer corresponding 1:1 to a submitted `DecisionQuestion`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionAnswer {
    Predicate {
        yes_probability: f64,
        confidence: f64,
    },
    Choice {
        choice: String,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
    Score {
        expected_score: f64,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
}

/// Validated provider decision output; missing usage stays `None`, never invented zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionResponse {
    pub model: String,
    pub provider: String,
    pub calibration: CalibrationKind,
    pub answers: BTreeMap<String, DecisionAnswer>,
    pub usage: Option<TokenUsage>,
}
```

#### Validation Invariants
- `DecisionRequest::validate()` enforces:
  - `1..=64` questions; every question key is non-empty, `<= 128` bytes, ASCII `[A-Za-z0-9._-]`.
  - Total serialized request payload (`state`, `questions`, `instructions`, `model_override`) `<= 1 MiB`.
  - `Predicate`: non-empty `prompt`.
  - `Choice`: non-empty `prompt`, non-empty bounded `options` (`1..=128`), unique non-empty bounded option IDs (`1..=128` bytes, ASCII `[A-Za-z0-9._-]`).
  - `Score`: non-empty `prompt`, non-empty bounded `levels` (`1..=128`), unique non-empty bounded level IDs (`1..=128` bytes, ASCII `[A-Za-z0-9._-]`), finite `score` on every level (no `NaN` or infinity).
  - Optional `instructions` and `model_override`, when present, are non-empty; `state` is preserved verbatim without rewriting or shell/template interpolation.
- `DecisionRequest::validate_response(&self, response: &DecisionResponse)` enforces:
  - Non-empty `model` and `provider`, and exact 1:1 key set match between `request.questions` and `response.answers` (no missing or extra answers).
  - Exact variant match (`Predicate` -> `Predicate`, `Choice` -> `Choice`, `Score` -> `Score`).
  - Finite `confidence` in `[0.0, 1.0]` (non-NaN) for every answer.
  - `Predicate`: finite `yes_probability` in `[0.0, 1.0]`.
  - `Choice`: `choice` must name a submitted option ID; `probabilities` keys match the exact submitted option ID set, every probability is finite in `[0.0, 1.0]`, total mass sums to `1.0` within rounding tolerance, and `choice` achieves maximal probability within floating-point epsilon.
  - `Score`: `probabilities` keys match the exact submitted level ID set, every probability is finite in `[0.0, 1.0]`, total mass sums to `1.0` within rounding tolerance, and finite `expected_score` lies within `[min_level_score, max_level_score]` and agrees with `sum(p_i * level_i.score)` within rounding tolerance.
- `MalformedDecisionResponse` is a distinct error type so callers and `alan-llmfs` differentiate corrupted or schema-violating evidence from transport/HTTP unavailability without implicit retries or prose fallback.

### 2. `LlmProvider` Trait and `SharedLlmProvider` (`crates/llm/src/provider.rs`)

```rust
pub trait LlmProvider: Send + Sync {
    // ... existing generate / chat / generate_stream ...

    /// Whether this callable supports typed `decide()` requests.
    fn supports_decisions(&self) -> bool {
        false
    }

    /// Baseline calibration advertised by this callable when `supports_decisions()` is true.
    fn decision_calibration(&self) -> Option<CalibrationKind> {
        None
    }

    /// Evaluate one or more typed decision questions without assistant prose or tool calls.
    async fn decide(&mut self, _request: DecisionRequest) -> Result<DecisionResponse> {
        anyhow::bail!("decision evaluation unavailable")
    }

    /// Defaults to `self.supports_decisions()` and delegates `evaluate_choice()`
    /// cleanly via `DecisionQuestion::Choice` when `supports_decisions()` is true.
    fn supports_choice_evaluation(&self) -> bool {
        self.supports_decisions()
    }
}
```

`SharedLlmProvider` caches `supports_decisions: bool` and `decision_calibration: Option<CalibrationKind>` at construction time and forwards `decide()` through its inner async mutex, matching `supports_choice_evaluation()` / `evaluate_choice()`.

### 3. Native Calibrated Decisions Endpoints vs `FallbackDecisionEvaluator`

#### Native Calibrated Decisions Endpoints (`CalibrationKind::Calibrated`, zero output tokens)

| Provider | `supports_decisions()` | `decision_calibration()` | Native Wire Endpoint & Provenance |
| --- | --- | --- | --- |
| `OpenRouterClient` | `true` | `Some(Calibrated)` | Native OpenRouter Decisions API via `client.decisions().create()` at `POST /api/alpha/decisions`; zero output tokens, strict distribution validation, always `Calibrated` on verified native response. |
| `GoogleGeminiGenerateContentClient` | `true` | `Some(Calibrated)` | Native Google Gemini / Vertex AI Decisions at `POST /v1beta/decisions` (e.g. `gemini-decision-1` / Vertex AI Decisions); zero output tokens, strict distribution validation, `Calibrated`. |
| `OpenAiResponsesClient` (OpenAI native) | `true` | `Some(Calibrated)` | Native OpenAI Decisions endpoint at `POST /v1/decisions`; zero output tokens, strict distribution validation, `Calibrated`. |
| `TypesafeEvaluationClient` | `true` | `Some(Calibrated)` | Native TypeSafe SystemOne endpoint at `POST /v1/systemone` (`predicate` / `choice` / `score`), pinned `jev-X.Y.Z` provenance check, unique JSON keys, bounded `<= 1 MiB` response, Host credential per request; zero output tokens, `Calibrated`. |
| `MagpieClient` (native decisions route) | `true` | `Some(Calibrated)` | Native Magpie `/v1/decisions` passthrough to a calibrated Decisions backend with session-affinity headers and zero output tokens (`Calibrated`). |

#### Structured Generation Fallback (`CalibrationKind::Uncalibrated` — NEVER `Calibrated`)

| Provider / Wrapper | `supports_decisions()` | `decision_calibration()` | Wire Mechanism & Calibration Rule |
| --- | --- | --- | --- |
| `FallbackDecisionEvaluator` / `decide_via_generation()` | `true` (when inner supports generation) | `Some(Uncalibrated)` | Reusable no-tools strict JSON Schema evaluator over any generation-capable `LlmProvider` (used for generation-only providers such as `anthropic_messages`, `openai_chat_completions`, `chatgpt_responses`, and Magpie chat fallback). Frames `state` explicitly as untrusted JSON data, disables tools, sets `temperature: 0.0`, parses with duplicate-key rejection, validates via `DecisionRequest::validate_response()`, and **ALWAYS** stamps `CalibrationKind::Uncalibrated`. |

**Calibration Honesty Invariant**: `CalibrationKind::Calibrated` is reserved exclusively for native calibrated Decisions endpoints returning calibrated probability distributions with zero output tokens (OpenRouter `POST /api/alpha/decisions`, Google Gemini `POST /v1beta/decisions` / Vertex AI Decisions, OpenAI `POST /v1/decisions`, TypeSafe `/v1/systemone`, and Magpie `/v1/decisions` passthrough). Generation-based fallbacks (`FallbackDecisionEvaluator` / `decide_via_generation()` over `anthropic_messages`, `openai_chat_completions`, `chatgpt_responses`, or Magpie chat fallback) MUST NEVER claim `CalibrationKind::Calibrated`.

### 4. `alan-llmfs` File Surface (`crates/llmfs`)

```text
/mnt/llm/
├── providers/<provider>/
│   ├── capabilities        # adds "decisions": { "v1": bool, "calibration": "calibrated"|"uncalibrated"|null }
│   ├── models
│   └── status
└── connections/<conn>/
    ├── clone               # existing Generation allocation (version 2 request)
    ├── evaluate            # existing choice.v1 allocation (preserved)
    ├── decide              # NEW: decision.v1 allocation (clone-via-open, ReadWrite)
    ├── capabilities        # adds "decisions": { "v1": bool, "calibration": ... }
    ├── meter               # adds limits.max_decisions & meter.decision_starts
    ├── models
    ├── profile
    ├── provider
    └── <n>/                # allocated operation directory: data, events, ctl, status
```

#### Lifecycle & Limits
- Replace `evaluation: bool` on `Generation` / `try_reserve` with `OperationKind { Generation, Evaluation, Decision }`.
- `ConnectionLimits` adds `pub max_decisions: Option<u64>`, metered by `decision_starts: AtomicU64` independently of `max_generations` and `max_evaluations`.
- Opening `connections/<conn>/decide`:
  - Requires `OpenMode::ReadWrite` (rejects read-only/write-only with `ErrorCode::InvalidMode`).
  - Rejects with `ErrorCode::Unsupported` before allocating an operation if `!connection.supports_decisions`.
  - Rejects with `ErrorCode::NoAccess` at dial time if `max_decisions` is exhausted.
- Committing `data` (on clunk):
  - Parses strict `decision.v1` request document (`#[serde(deny_unknown_fields)]`, `version: 1`, `schema: "decision.v1"`, `state`, `questions`, `deadline_ms` in `1..=30_000`, total `<= 1 MiB`).
  - Malformed documents transition `Open -> Rejected`, append a rejected record to `events`, and return `ErrorCode::BadRequest` without calling the provider.
  - Valid documents transition `Open -> Running`, run `provider.decide(request)` under `tokio::time::timeout_at(expires, ...)` raced with `ctl` abort, validate the returned `DecisionResponse` against the request, record `TokenUsage` if present (leaving missing usage as `null`, never `0`), append one terminal `decision.v1` JSON line to `events`, and advance to `Done`.
  - Failure reasons map deterministically: `malformed_decision_response`, `invalid_decision_answer`, `decision_timeout`, or `alan_llm::safe_failure_reason(&error)`.

#### Wire Document Shapes (`decision.v1`)
Request written to `<n>/data`:
```json
{
  "version": 1,
  "schema": "decision.v1",
  "state": { "tool": "bash", "command": "cargo test" },
  "deadline_ms": 5000,
  "instructions": "Evaluate strictly against policy.",
  "questions": {
    "safe": { "type": "predicate", "prompt": "Does the action stay within policy?" },
    "verdict": {
      "type": "choice",
      "prompt": "Select policy action.",
      "options": [
        { "id": "allow", "description": "Action satisfies policy." },
        { "id": "deny", "description": "Action violates policy." },
        { "id": "escalate", "description": "Ambiguous; ask human." }
      ]
    },
    "risk": {
      "type": "score",
      "prompt": "Rate risk level.",
      "levels": [
        { "id": "low", "description": "Read-only or contained.", "score": 0.0 },
        { "id": "medium", "description": "Reversible external effect.", "score": 0.5 },
        { "id": "high", "description": "Destructive or irreversible.", "score": 1.0 }
      ]
    }
  }
}
```

Terminal record appended to `<n>/events`:
```json
{
  "version": 1,
  "done": true,
  "decision": {
    "schema": "decision.v1",
    "calibration": "calibrated",
    "provider": "typesafe",
    "model": "jev-1.13.0",
    "answers": {
      "safe": { "type": "predicate", "yes_probability": 0.96, "confidence": 0.96 },
      "verdict": {
        "type": "choice",
        "choice": "allow",
        "probabilities": { "allow": 0.95, "deny": 0.02, "escalate": 0.03 },
        "confidence": 0.95
      },
      "risk": {
        "type": "score",
        "expected_score": 0.08,
        "probabilities": { "low": 0.88, "medium": 0.08, "high": 0.04 },
        "confidence": 0.91
      }
    },
    "usage": { "prompt_tokens": 120, "completion_tokens": 0, "total_tokens": 120 },
    "cost_microusd": null
  }
}
```

## Part II (PR 4): System 1 Smart Approval (`ConfinedSmart` / `rw·smart`)

### 1. Trust Posture Spectrum (`crates/agent-engine/src/policy/trust.rs`)

Extend `TrustPosture` with `ConfinedSmart` ranked strictly between `Governed` and `ConfinedTrusted`:

| Posture | Rank | OS Sandbox | Host Mount Bounds | Per-Action Governance | Status Badge (Row 1) |
| --- | --- | --- | --- | --- | --- |
| `Governed` | 0 | Active | Enforced | Full policy + red lines + Guardian / human | `rw·gov` / `ro·gov` |
| `ConfinedSmart` | 1 | Active | Enforced | Contained actions auto-run; red lines bypass System 1; reviewer-judged actions triaged via `decide()` with calibration gates + fail-closed human fallback | `rw·smart` / `ro·smart` |
| `ConfinedTrusted` | 2 | Active | Enforced | Bypasses per-action policy, red lines, and Guardian | `rw·trust` / `ro·trust` |
| `HostAccountTrusted` | 3 | Disabled | Disabled | Bypasses per-action policy and OS sandbox (explicit opt-in) | `! unsandboxed` (bold red) |

#### Authority & Lifecycle Invariants
- **Invocation-only**: Activated only by an explicit trusted-user CLI flag (e.g. `--smart-approval`, incompatible with `--unsafe-host-account` unless `--trust` semantics explicitly supersede) or trusted interactive toggle (`Shift+Tab` cycles confined postures `Governed -> ConfinedSmart -> ConfinedTrusted -> Governed`, never jumping to `HostAccountTrusted`).
- **Untrusted sources rejected**: `agent.toml`, mounted `AGENTS.md`/`CLAUDE.md`, saved rollout/summary history, and AgentFS control writes cannot activate or widen to `ConfinedSmart`.
- **Child narrowing**: `TrustPosture::resolve_child()` allows a `ConfinedSmart` parent to spawn `ConfinedSmart` or `Governed` children, and rejects `ConfinedTrusted` or `HostAccountTrusted`.
- **Downgrade safety**: Downgrading from `ConfinedSmart` to `Governed` invalidates any in-flight System 1 assessment so a stale `allow` cannot authorize execution.

### 2. Decision Flow inside `ConfinedSmart`

```text
Proposed Tool Action (under ConfinedSmart)
  │
  ├─► 1. OS Boundary Check (ADR-0061)
  │       └─ Contained by active OS sandbox + explicit Host Mounts? ──► Execute directly (no decide() call)
  │
  ├─► 2. Deterministic Red Lines & Grant Check
  │       ├─ Catastrophic deny red line? ────────────────────────────► Deny immediately
  │       ├─ Always-human red line (secrets/exfil, policy weakening,
  │       │   undelegated force-push/remote delete, new private
  │       │   resource / out-of-grant authority)? ───────────────────► Escalate straight to human
  │       └─ Missing OS sandbox backend? ────────────────────────────► Conservative pre-sandbox gate (no confined claim)
  │
  └─► 3. Reviewer-Judged Escalation -> System 1 `connections/<conn>/decide`
          ├─ Build no-tools `DecisionRequest` (`Predicate` + `Choice{allow,deny,escalate}` + `Score[0,1]`)
          │   with untrusted-transcript framing, bounded deadline, and captured policy/grant revision
          ├─ Dispatch to configured System 1 evaluator Connection (dedicated profile or main fallback)
          ├─ Validate calibration-aware gate:
          │   ├─ Calibrated: allow iff verdict == allow && safe == true && confidence >= cal_min && risk <= cal_max_risk
          │   ├─ Uncalibrated: allow iff policy permits uncalibrated fast-path && verdict == allow && safe == true
          │   │                && confidence >= uncal_min && risk <= uncal_max_risk (stricter thresholds)
          │   ├─ Deny verdict: return rationale to main agent for self-correction + increment denial circuit breaker
          │   └─ Uncertain / below threshold / timeout / error / malformed / stale revision:
          │       Fail closed ──► emit one correlated pending human approval request (NEVER auto-allow)
```

### 3. Terminal UI Presentation (`crates/tui`)

- Row 1 status bar displays `rw·smart` / `ro·smart` (or pure-ASCII `rw/smart` / `ro/smart` when `theme.is_dumb` is set), preserving the narrow-terminal priority hierarchy (never dropping urgent bold red `! unsandboxed`).
- `^T` status panel discloses the active System 1 evaluator profile/model, its `CalibrationKind` (`calibrated` or `uncalibrated`), and smart-approval counts/decisions without leaking credentials or internal `request_id`s.
- Inline decision receipts surface System 1 allows, denials (with rationale), and fail-closed escalations to the user so autonomous triage remains observable and auditable.
- Verified via real Unix PTY (`openpty` + `vt100`) tests covering `rw·smart` badge rendering, dumb-terminal ASCII fallback, `Shift+Tab` posture cycling, narrow-pane degradation, and fail-closed approval rendering.

## Risks / Trade-offs

| Risk | Mitigation |
| --- | --- |
| Uncalibrated generation models produce overconfident self-reported scores | Enforce `CalibrationKind` on every response; never label schema-extracted output `Calibrated`; apply stricter thresholds or human confirmation for `Uncalibrated` evaluators in PR 4. |
| Prompt injection inside tool outputs or transcript tries to coerce System 1 into `allow` | Frame `state` strictly as untrusted data, prohibit tools on decision requests, enforce deterministic deny and always-human red lines before System 1, and enforce the 3-consecutive / 10-of-50 denial circuit breaker. |
| Provider latency or runaway decision calls slow down turns or inflate cost | Enforce bounded per-request `deadline_ms` (`<= 30_000`), independent `ConnectionLimits::max_decisions` metering in `llmfs`, and ADR-0061's rule skipping System 1 entirely for sandbox-contained actions. |
| Breaking existing `evaluate_choice` / `choice.v1` tests and callers | Keep `evaluate_choice()`, `supports_choice_evaluation()`, `connections/<conn>/evaluate`, and `max_evaluations` intact alongside `decide()` and `connections/<conn>/decide`. |
| Users confuse `rw·smart` with unrestricted execution or believe it weakens OS confinement | Distinguish `rw·gov`, `rw·smart`, `rw·trust`, and `! unsandboxed` clearly in Row 1 and `^T` status; reject `ConfinedSmart` from untrusted files and never allow `Shift+Tab` to promote to `HostAccountTrusted`. |
