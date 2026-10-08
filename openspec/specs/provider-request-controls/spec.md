# provider-request-controls Specification

## Purpose
Define alan's canonical provider request-control contract. This capability owns
named reasoning effort as the only public control, strict input validation,
request-control resolution, provider projection, metadata mirroring, and
guardrails that prevent request-control truth from spreading across
configuration, Agent Process state, rollout evidence, and provider adapters.
## Requirements
### Requirement: Canonical reasoning effort type
alan SHALL define a shared typed reasoning effort model with lowercase
serialization values `none`, `minimal`, `low`, `medium`, `high`, and `xhigh`.

#### Scenario: Parsing valid effort values
- **WHEN** config, protocol, or API payloads contain `none`, `minimal`, `low`, `medium`, `high`, or `xhigh`
- **THEN** alan parses the value into the canonical reasoning effort enum

#### Scenario: Rejecting invalid effort values
- **WHEN** config, protocol, or API payloads contain an unknown reasoning effort string
- **THEN** alan rejects the value with an error that names the supported values

#### Scenario: Distinguishing unset from none
- **WHEN** reasoning effort is omitted
- **THEN** alan treats the effort as unset rather than as `none`
- **AND** `none` remains an explicit request to disable reasoning where the model supports it

### Requirement: Public reasoning configuration has one canonical form
Alan SHALL expose `model_reasoning_effort` as the only agent-facing reasoning
configuration control. Configuration, protocol, API, and client documents SHALL
reject `thinking_budget_tokens` as an unknown or unsupported field and SHALL NOT
migrate it to a named effort.

#### Scenario: Canonical reasoning effort is configured
- **WHEN** a valid `model_reasoning_effort` is present in agent configuration
- **THEN** Alan resolves and validates that effort through the canonical request
  control path

#### Scenario: Retired thinking budget is configured
- **WHEN** agent configuration contains `thinking_budget_tokens`
- **THEN** configuration loading fails and identifies the retired field
- **AND** Alan does not translate the numeric value into a reasoning effort

#### Scenario: Provider needs a numeric wire budget
- **WHEN** a provider adapter must send a numeric budget for a validated named
  reasoning effort
- **THEN** the adapter derives that provider-native value internally
- **AND** the derived wire field does not create a public budget-token input

### Requirement: Reasoning-capable model metadata
Alan SHALL declare model-level supported and default reasoning efforts in the
model catalog. The Connection-owned catalog projection used by model selection
SHALL expose those values from the canonical model owner, without a second
renderer-owned catalog. Unavailable metadata SHALL remain explicitly unavailable.

#### Scenario: Model catalog entry declares efforts
- **WHEN** a bundled or overlay model entry supports reasoning
- **THEN** the entry can declare `supported_reasoning_efforts` and `default_reasoning_effort`

#### Scenario: Default effort must be supported
- **WHEN** a model entry declares `default_reasoning_effort`
- **THEN** Alan validates that the default appears in `supported_reasoning_efforts`

#### Scenario: Existing supports_reasoning compatibility
- **WHEN** an existing catalog entry only declares `supports_reasoning = true`
- **THEN** Alan derives a conservative supported/default effort set or requires the entry to be migrated before validation passes

#### Scenario: Owner-visible model metadata
- **WHEN** the model catalog exposes model metadata to an authorized consumer
- **THEN** it includes supported reasoning efforts and the default reasoning effort for each listed model
- **AND** the Connection projection preserves those owner-provided values

#### Scenario: Default managed configuration uses the canonical bundled catalog
- **WHEN** a published managed connection uses a model recognized by the canonical bundled catalog without an explicit catalog override
- **THEN** its model list and reasoning metadata come from the same resolved catalog used to validate model selection
- **AND** absence of an optional override does not make that known catalog unavailable
- **AND** a provider without canonical model information remains explicitly unavailable

#### Scenario: Frequent model observation uses published Connection state
- **WHEN** the runtime observes model state without an explicit Connection operation
- **THEN** its catalog projection reads the Connection Service's already-published authority and canonical metadata
- **AND** observation does not reload persisted connection metadata or construct a new callable
- **AND** Connection-owned publication changes remain visible on the next observation
- **AND** external metadata changes are refreshed at existing explicit Connection read, capture, selection and recovery boundaries, without claiming an instantaneous cross-process disk watch
- **AND** observation never bypasses refreshed validation of an explicit capture or selection

### Requirement: Explicit request controls are validated before dispatch
alan SHALL validate explicit request controls against provider capability and
resolved model metadata before making a provider request. Explicit unsupported
controls SHALL fail before dispatch instead of being silently dropped.

#### Scenario: Provider does not support effort control
- **WHEN** a turn explicitly requests reasoning effort and the selected provider declares no effort-control support
- **THEN** alan rejects the turn before provider dispatch
- **AND** the error identifies the unsupported request control

#### Scenario: Model rejects unsupported effort
- **WHEN** the resolved model catalog entry supports only `low` and `high`
- **AND** an Agent Process or turn explicitly requests `xhigh`
- **THEN** alan rejects the request before provider dispatch
- **AND** the error lists the supported efforts from the model metadata

#### Scenario: Legacy budget request is rejected
- **WHEN** config, protocol, API, or client payloads contain `thinking_budget_tokens`
- **THEN** alan rejects the request before provider dispatch
- **AND** the error identifies `model_reasoning_effort` as the supported reasoning control

### Requirement: Generation requests carry normalized controls
alan SHALL carry canonical reasoning controls on `GenerationRequest` rather than
requiring provider adapters to infer them from ad hoc `extra_params` or legacy
budget fields.

#### Scenario: Turn request includes resolved effort
- **WHEN** runtime constructs a generation request for a reasoning-capable model
- **THEN** the request includes the validated effective reasoning effort

#### Scenario: No reasoning control
- **WHEN** neither explicit effort nor model default applies
- **THEN** the request omits reasoning controls and lets the provider use its default behavior

#### Scenario: Legacy public budget field is unavailable
- **WHEN** a caller tries to construct or mutate a generation request with `thinking_budget_tokens`
- **THEN** alan provides no supported public request-control path for that field
- **AND** provider projection cannot use a legacy budget fallback

#### Scenario: Canonical controls reject raw budget-only input
- **WHEN** protocol or generation-request code attempts to set `reasoning.budget_tokens`
- **THEN** alan rejects or cannot represent that raw budget-only control
- **AND** callers must use named reasoning effort instead

### Requirement: Provider adapters only project normalized controls
Provider adapters SHALL consume normalized request controls from
`GenerationRequest` and project them to provider-specific payload fields.
Provider adapters SHALL NOT own alan-level override precedence, model default
selection, config conflict resolution, or legacy budget compatibility.

#### Scenario: Canonical effort overrides provider extra params
- **WHEN** a generation request contains normalized reasoning effort `low` and provider-specific extra params include `reasoning_effort = "high"`
- **THEN** the provider adapter sends `low`
- **AND** the provider-specific extra param does not create a competing effective value

#### Scenario: Provider-native budget is derived internally
- **WHEN** a provider requires a budget-shaped wire field for the effective reasoning effort
- **THEN** the provider adapter derives that budget from normalized effort, model metadata, and provider rules
- **AND** the adapter does not accept public `thinking_budget_tokens` as an alternate effective value

### Requirement: OpenAI provider mapping
alan SHALL map canonical reasoning effort to OpenAI-native request fields for
OpenAI Responses and OpenAI Chat Completions providers.

#### Scenario: OpenAI Responses effort
- **WHEN** `openai_responses` receives a generation request with effective effort
- **THEN** alan sends it as `reasoning.effort`

#### Scenario: OpenAI Chat Completions effort
- **WHEN** `openai_chat_completions` receives a generation request with effective effort
- **THEN** alan sends it as `reasoning_effort`

#### Scenario: OpenAI unsupported effort
- **WHEN** a selected OpenAI model does not support the effective effort
- **THEN** alan rejects the request before provider dispatch

### Requirement: Anthropic provider mapping
alan SHALL map canonical reasoning effort to Anthropic extended-thinking budget
configuration when the selected Anthropic model supports extended thinking.

#### Scenario: Anthropic effort maps to budget
- **WHEN** `anthropic_messages` receives a generation request with effective effort
- **THEN** alan maps the effort to the configured Anthropic `thinking.budget_tokens` preset for the selected model

#### Scenario: Anthropic minimum budget
- **WHEN** the mapped Anthropic budget is below the provider minimum
- **THEN** alan rejects the request before dispatch

#### Scenario: Anthropic max tokens relationship
- **WHEN** Anthropic thinking is enabled
- **THEN** alan ensures `max_tokens` is greater than `budget_tokens` or rejects/adjusts according to the provider adapter contract

### Requirement: Gemini provider mapping
alan SHALL map canonical reasoning effort to Gemini thinking controls according
to model family.

#### Scenario: Gemini 3 thinking level
- **WHEN** `google_gemini_generate_content` uses a Gemini 3 model and receives effective effort
- **THEN** alan maps supported efforts to `thinkingConfig.thinkingLevel`

#### Scenario: Gemini 2.5 thinking budget
- **WHEN** `google_gemini_generate_content` uses a Gemini 2.5 model and receives effective effort
- **THEN** alan maps the effort to a catalog-defined `thinkingBudget`

#### Scenario: Gemini disable thinking
- **WHEN** a Gemini model does not support disabling thinking and the effective effort is `none`
- **THEN** alan rejects the request before dispatch

### Requirement: Compatible-provider and OpenRouter mapping
alan SHALL only send reasoning-effort extension fields to compatibility
providers when the provider/model explicitly declares support.

#### Scenario: Compatible provider supports effort extension
- **WHEN** `openai_chat_completions_compatible` receives effective effort for a model that declares `reasoning_effort` support
- **THEN** alan sends the compatible extension field

#### Scenario: Compatible provider does not support effort extension
- **WHEN** a compatibility provider/model lacks declared effort support
- **THEN** alan rejects explicit reasoning effort rather than silently dropping it

#### Scenario: OpenRouter SDK-backed provider
- **WHEN** the SDK-backed `openrouter` provider receives effective effort
- **THEN** alan maps the effort to the OpenRouter SDK/provider-native reasoning field supported by the selected endpoint and model

### Requirement: Request control intent separates Process and turn ownership
Alan SHALL represent Agent Process request-control intent separately from per-turn intent and from
the normalized controls passed to a provider. Process intent SHALL be resolved from the explicit
Agent Definition descriptor or installed reference, connection/model catalog, and spawn inputs;
turn intent MAY override only the current transition.

#### Scenario: A turn overrides Process reasoning effort
- **WHEN** an Agent Process resolves medium reasoning effort and one turn explicitly requests low
- **THEN** that generation uses low
- **AND** later turns retain the Process-level medium intent

### Requirement: Effective request controls are file and rollout observable
Agent Runtime Service SHALL project effective Process and current-turn request
controls through Agent Machine state and rollout/checkpoint evidence. Inspection
SHALL distinguish the confirmed binding for subsequently admitted input from the
captured binding and resolved controls of already-admitted work. Unknown values
SHALL be explicit; no secret material SHALL enter these projections.

#### Scenario: Renderer or auditor inspects effective controls
- **WHEN** effective reasoning controls are needed for inspection
- **THEN** the client reads the owning Agent Machine or durable evidence surface
- **AND** the projected values come from the canonical runtime resolver

#### Scenario: Model selection occurs while older work remains admitted
- **WHEN** a client inspects status after a confirmed model selection
- **THEN** it can distinguish the confirmed next-input binding from older admitted work's captured binding
- **AND** the renderer does not relabel that older work with the newly selected model

#### Scenario: Model selection receipt belongs to one Process input
- **WHEN** the renderer requests a model selection
- **THEN** it correlates completion with the pinned Process and the exact selection input ID
- **AND** successful control-file transport or an unrelated completion does not confirm selection
- **AND** a response from an earlier Root cannot settle the current request

#### Scenario: Model observation and selection completion arrive separately
- **WHEN** a model projection update and a selection completion arrive in either order
- **THEN** the renderer distinguishes the observed selected-next binding from the request's pending or completed disposition
- **AND** it does not fabricate model controls when the projection cannot be read
- **AND** losing the owning Process or completion stream leaves the request uncertain without automatic retry

#### Scenario: A completed model selection is retried after a later selection
- **WHEN** a client repeats a model selection with the same input ID after a different model has been confirmed
- **THEN** the original terminal outcome is returned without installing or persisting that selection again
- **AND** subsequent ordinary input captures the later confirmed binding
- **AND** restoring durable selection outcomes preserves the same identity fence without replaying model selection

#### Scenario: A failed model selection is repeated
- **WHEN** a selection has a retained terminal failure and the same input ID is delivered again
- **THEN** it returns the same failed outcome without trying a new selection
- **AND** a durable failure outcome does not replace the last successfully confirmed binding

### Requirement: Request control tests guard durable owners
Tests SHALL cover Agent Process intent, per-turn override, Agent Definition configuration, model
catalog default, provider projection, and Agent Machine/rollout observability. They SHALL fail if a renderer,
transport adapter, or provider adapter independently recomputes resolver-owned defaults.

#### Scenario: Resolver ownership drifts
- **WHEN** request-control resolution is duplicated outside the canonical runtime resolver
- **THEN** focused boundary tests fail

### Requirement: TypeSafe finite-choice adapter preserves bounded evaluation authority
The TypeSafe evaluation adapter SHALL map the caller's finite candidate set to a typed Choice with a distinct abstention option. It SHALL validate returned model identity, selection, distribution and usage, enforce bounded transport without retries or redirects, and keep credentials and response bodies out of errors. This adapter SHALL NOT implement generation by reinterpreting an evaluation request.

#### Scenario: Valid finite-choice result
- **WHEN** the pinned model returns a valid choice from the submitted wire criteria
- **THEN** the adapter returns the original caller candidate ID or NoMatch for abstention
- **AND** original input is preserved and no Tool or command is dispatched

#### Scenario: Untrusted or unavailable provider result
- **WHEN** response provenance, type, candidate identity, distribution, usage or size is invalid, or transport fails
- **THEN** evaluation fails without retry, fallback generation or exposed credential/body diagnostics

#### Scenario: Live qualification is incomplete
- **WHEN** qualification evidence consists only of local HTTP fixtures
- **THEN** that evidence does not establish authenticated live-provider support
- **AND** successful Connection publication alone does not establish routing qualification
