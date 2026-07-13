# provider-connection-contract Specification

## Purpose
Defines provider and connection-profile contracts, including provider
capabilities, authentication boundaries, host credential storage, request
controls, model metadata, and explicit degradation.
## Requirements
### Requirement: Provider and connection contracts live in OpenSpec
alan SHALL specify provider capabilities, provider authentication, connection
profiles, request controls, provider-specific degradation, and host credential
boundaries in OpenSpec.

#### Scenario: Provider setup changes
- **WHEN** a change modifies provider descriptors, connection profiles,
  credential storage, managed auth, profile selection, provider request
  shaping, or model metadata
- **THEN** the OpenSpec delta updates this capability,
  `provider-request-controls`, a provider-specific capability such as
  `openrouter-provider-adapter`, or an active provider change
- **AND** no duplicate provider contract is maintained under `docs/spec/`

### Requirement: Host auth and runtime provider state remain separated
alan SHALL keep host credential control, managed login state, connection profile
metadata, runtime request shaping, and provider-native features in their
respective layers.

#### Scenario: Credential material is configured
- **WHEN** a user configures API keys, managed ChatGPT login, or provider
  credential references
- **THEN** secrets are stored through host credential mechanisms rather than in
  agent-facing `agent.toml`

#### Scenario: Provider feature differs by adapter
- **WHEN** a provider supports or rejects a feature such as reasoning effort,
  continuation, rich content, or provider-native metadata
- **THEN** alan projects that capability through the provider contract and
  degrades explicitly when the selected provider cannot support it

### Requirement: Connection metadata and credential material are separated
alan SHALL store non-secret connection metadata separately from secret-bearing
credentials and managed login state.

V1 non-secret metadata lives in the Connection Service subtree of the active
channel System Store as `connections.toml`, with this logical shape. Credentials
and managed login state remain in the matching channel Host Store.

```toml
version = 1
default_profile = "chatgpt-main"

[credentials.chatgpt]
kind = "managed_oauth"
provider_family = "chatgpt"
label = "ChatGPT login"
backend = "host_managed_auth"

[profiles.chatgpt-main]
provider = "chatgpt"
credential_id = "chatgpt"
source = "managed"

[profiles.chatgpt-main.settings]
base_url = "https://chatgpt.com/backend-api/codex"
model = "gpt-5.3-codex"
account_id = ""
```

Rules:

- `connections.toml` stores profile and credential metadata only.
- Secret-bearing credentials live in a host-managed store outside `agent.toml`.
- Managed ChatGPT login state remains outside `connections.toml`.
- Existing ChatGPT managed login uses the managed auth owner in the active
  channel Host Store.
- `secret_string` credentials use a host-managed secret store with file
  permissions equivalent to `0600` unless replaced by a stronger host backend
  such as keychain or keyring.
- Future host credential backends may change without changing the logical
  profile contract.

#### Scenario: Secret credential is configured
- **WHEN** an operator configures an API-key-backed profile
- **THEN** `connections.toml` stores only credential metadata and a credential
  reference
- **AND** the secret value is written through the host credential backend rather
  than `agent.toml` or profile settings

#### Scenario: Managed ChatGPT login is configured
- **WHEN** an operator logs in to the `chatgpt` provider
- **THEN** managed bearer/refresh state is stored in the managed auth store
- **AND** profile metadata only references the managed credential id

### Requirement: Connection events and errors are provider-neutral
alan SHALL expose connection-management events and errors through provider-neutral
envelopes with stable codes and replay cursor metadata.

Stable event types:

- `profile_created`
- `profile_updated`
- `profile_deleted`
- `profile_activated`
- `credential_status_changed`
- `login_started`
- `browser_login_ready`
- `device_code_ready`
- `login_succeeded`
- `login_failed`
- `logout_completed`
- `connection_test_succeeded`
- `connection_test_failed`

Minimum error codes:

- `profile_not_found`
- `credential_not_found`
- `provider_not_supported`
- `unsupported_operation`
- `validation_failed`
- `credential_missing`
- `credential_pending`
- `credential_expired`
- `login_failed`
- `connection_test_failed`
- `session_binding_conflict`

#### Scenario: Connection event is emitted
- **WHEN** profile, credential, login, logout, activation, or test state changes
- **THEN** the event envelope includes profile id, provider, credential id when
  applicable, and replay cursor metadata

#### Scenario: Connection operation is rejected
- **WHEN** a connection operation fails before provider dispatch
- **THEN** alan returns a stable provider-neutral error code rather than a
  provider-specific raw exception shape

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
continuity, and explicit reasoning-effort controls.

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

### Requirement: Provider-specific fidelity remains owned by provider adapters
alan SHALL preserve provider-native semantics at the adapter layer when they
matter to turn semantics, context, streaming, tool orchestration, or provider
state.

Provider-specific requirements:

- OpenAI Responses preserves instructions, itemized tool calls/results,
  provider response id/status, `previous_response_id`, retrieval/cancel,
  background polling, provider compaction where supported, reasoning items,
  encrypted reasoning state, native multimodal/file inputs, cached usage, and
  named reasoning effort when model-supported.
- Managed ChatGPT Responses preserves compatible Responses-shaped semantics
  but defaults to explicit capability limits: stream transport, `store=false`,
  no `temperature`, no `max_output_tokens`, no `previous_response_id`, and no
  background/retrieve/cancel/provider compaction unless revalidated.
- OpenAI Chat Completions preserves official role model including `developer`,
  multimodal content arrays where supported, tool calls and `tool` messages,
  response id, streaming deltas, usage, and reasoning-effort controls where
  supported. It does not pretend to support Responses-style continuation.
- Anthropic Messages preserves `tool_use` and `tool_result` block ordering,
  extended thinking, thinking signatures, redacted thinking, native image and
  document inputs, prompt-caching usage, provider id, `stop_reason`, and
  effort-to-budget mapping where configured.
- Generic Chat Completions-compatible providers remain conservative and support
  only verified text, streaming, tool-call, usage, reasoning-field, and
  reasoning-effort extensions.
- OpenRouter remains a first-class provider id with SDK-backed dispatch, while
  retaining Tier C capability semantics across upstream model/provider routes.

#### Scenario: Provider projection is lossy
- **WHEN** adapter projection would discard provider-native semantics that
  affect runtime behavior
- **THEN** alan preserves the native representation, explicitly emulates,
  rejects, or emits an observable warning according to the selected provider
  tier

#### Scenario: Reasoning effort is configured
- **WHEN** a request carries normalized effective reasoning effort
- **THEN** provider adapters project that value to provider-specific wire fields
- **AND** they do not recompute alan-level precedence, defaults, or validation
  owned by `provider-request-controls`

### Requirement: Cross-channel connection reuse is explicit
Alan SHALL require an explicit user action before copying or importing stable
connection profile metadata or auth material into the dev channel.

#### Scenario: User requests profile import
- **WHEN** a future import command copies a stable profile into the dev channel
- **THEN** the command identifies the source channel and target channel explicitly
- **AND** it writes new metadata to the dev System Store and new credential references to the dev Host Store
- **AND** it does not make the dev profile a live reference to stable credential storage

#### Scenario: Managed auth is reused
- **WHEN** a future command or UI flow allows managed-auth reuse across channels
- **THEN** the user must approve that operation explicitly
- **AND** the resulting dev-channel auth state is stored under the dev-channel managed auth store
- **AND** routine dev startup still does not read stable managed auth as implicit fallback

### Requirement: Provider and connection vocabulary is Process-shaped
Alan SHALL distinguish provider family, provider descriptor, credential reference, connection
profile, default profile, resolved connection, and Process connection binding. A Process
connection binding SHALL associate one Agent Process with one resolved provider/model/credential
reference for its lifetime and SHALL contain no secret material.

#### Scenario: An Agent Process resolves a connection
- **WHEN** Alan spawns an Agent Process whose definition, launch request, or operator default selects a
  connection profile
- **THEN** it resolves one concrete provider/model/credential reference for that Process
- **AND** later default changes do not mutate the running Process binding

### Requirement: Connection management is direct and owner-scoped
Alan SHALL retain direct `alan connection` commands for descriptor discovery, profile mutation,
default selection, secret entry, login, and connection testing. The commands SHALL operate through
the owning Connection Service metadata, Host credential/auth stores, and provider adapters. Any future
file-server management surface requires its own accepted contract.

#### Scenario: Operator lists connection profiles
- **WHEN** an operator runs `alan connection list`
- **THEN** the CLI reads the active channel's connection and credential metadata owners directly
- **AND** the read-only command does not launch or mutate an Agent Process

### Requirement: Host auth remains separate from provider execution
Host auth SHALL own secret storage and managed login state; provider adapters SHALL receive only
resolved credential material needed for a generation. Browser login MAY use a bounded ephemeral
callback owned by the initiating host operation but SHALL NOT require a persistent product API.

#### Scenario: Browser login completes
- **WHEN** the operator initiates managed browser login
- **THEN** the initiating host auth operation receives and validates the callback
- **AND** only the host auth owner may grant or persist credential authority

### Requirement: Connection state remains channel-scoped
Stable and dev channels SHALL keep distinct connection metadata, credential references, and managed
auth roots. Agent Process creation SHALL fail truthfully when its active channel cannot resolve a
profile rather than borrowing another channel's state.

#### Scenario: Dev Process has no dev connection
- **WHEN** a dev-channel Agent Process is spawned without a resolvable dev connection profile
- **THEN** creation reports the missing connection
- **AND** stable connection or credential state is not consumed implicitly

### Requirement: Legacy connection metadata migrates once
Alan SHALL migrate non-secret legacy connection metadata into the channel
System Store, verify the service-readable result, and delete the legacy file.
Credential bytes SHALL remain in the owning Host credential store and no
compatibility reader SHALL remain.

#### Scenario: Legacy profile is valid
- **WHEN** upgrade finds a valid legacy profile and credential reference
- **THEN** the metadata is imported and verified before the old file is deleted
- **AND** secret bytes are never copied into System Store

### Requirement: Child Agent Processes preserve the selected Connection profile
Child Agent Process launch SHALL preserve the effective explicit Connection
profile unless the child definition or launch request selects a different one.
It MUST NOT silently reselect the Connection Service default.

#### Scenario: Parent uses a non-default explicit profile
- **GIVEN** a parent Agent Process uses an explicit profile that is not the service default
- **WHEN** it launches a child without a Connection override
- **THEN** child setup and runtime startup use the same explicit profile
- **AND** absence of a service default does not make child startup fail

#### Scenario: Child definition selects a different profile
- **GIVEN** a child Agent definition selects a profile different from its parent's profile
- **WHEN** the Agent Runtime Service launches the child
- **THEN** it resolves the child-selected profile before constructing the child's LLM client
- **AND** child setup and runtime startup use the same resolved provider settings

### Requirement: Connection profile credential references remain resolvable
Alan SHALL create or validate matching non-secret credential metadata whenever
an operator assigns an explicit credential reference to a Connection profile.
It MUST NOT persist a profile that references unknown or incompatible credential
metadata.

#### Scenario: Operator replaces a profile credential reference
- **GIVEN** an existing secret-backed Connection profile
- **WHEN** the operator edits it to use a new valid credential id
- **THEN** matching credential metadata is registered before the profile is saved
- **AND** setting the secret and testing the edited profile succeed

### Requirement: Connection authority is file-service owned
Connection Service SHALL be the only owner of profile metadata, defaults,
selection, validation status, and callable connection publication. Host
adapters SHALL own only native login and secret storage.

#### Scenario: Host adapter restarts
- **WHEN** Connection Service remains running
- **THEN** profile identity and non-secret settings remain authoritative
- **AND** the adapter can reconnect without reconstructing profiles

### Requirement: macOS is a Connection Service native adapter
Alan for macOS SHALL observe Connection Service native requests and perform
approved browser/device login and Keychain operations. It SHALL return only
opaque credential references and bounded results and MUST NOT maintain a second
profile/default registry.

#### Scenario: App reconnects after login
- **WHEN** the profile already exists in Connection Service
- **THEN** macOS reads its service status
- **AND** it does not recreate metadata from local preferences
