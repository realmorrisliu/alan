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

### Requirement: Provider and connection vocabulary is stable
alan SHALL use stable provider/connection vocabulary across CLI, daemon, TUI,
runtime metadata, docs, and OpenSpec deltas.

Stable terms:

- **Provider descriptor**: static metadata for a provider family, including
  supported credential kinds, editable settings, login capabilities, default
  settings, and validation behavior.
- **Credential**: secret or managed-auth material used to authenticate provider
  requests, such as an API key, managed OAuth login state, or ambient cloud
  auth.
- **Credential status**: host-managed availability state such as `missing`,
  `available`, `pending`, `expired`, or `error`.
- **Connection profile**: a named operator-selectable provider configuration
  that references a credential and carries non-secret settings such as base URL,
  model, region, workspace binding, or client name.
- **Default profile**: the operator default used for new sessions when no
  explicit profile or pin is supplied.
- **Pinned profile**: an optional `connection_profile` in an agent config that
  overrides the default profile for the matching global or workspace scope.
- **Effective profile**: the profile alan will use for the next session after
  resolving explicit session input, pin state, and default-profile state.
- **Session binding**: the immutable association between a session and the
  connection profile used to create it.

#### Scenario: Provider setup surface names a concept
- **WHEN** CLI output, daemon payloads, docs, or runtime metadata refer to
  provider setup or session binding concepts
- **THEN** they use this vocabulary and preserve the distinctions defined by
  this capability

### Requirement: Provider descriptors define provider setup capabilities
alan SHALL expose provider setup through static provider descriptors rather than
ad hoc provider-specific UI or runtime branches.

Stable provider descriptor fields:

```text
provider_id
display_name
credential_kind
supports_browser_login
supports_device_login
supports_secret_entry
supports_logout
supports_test
required_settings[]
optional_settings[]
default_settings{}
```

V1 provider catalog:

- `chatgpt`: `managed_oauth`; browser and device login; no secret entry; uses
  managed ChatGPT/Codex login.
- `openai_responses`: `secret_string`; secret entry; API key.
- `openai_chat_completions`: `secret_string`; secret entry; API key.
- `openai_chat_completions_compatible`: `secret_string`; secret entry; API key
  or token-like secret.
- `openrouter`: `secret_string`; secret entry; OpenRouter API key; default
  model `moonshotai/kimi-k2.6`.
- `anthropic_messages`: `secret_string`; secret entry; API key or compatible
  secret.
- `google_gemini_generate_content`: `ambient_cloud_auth`; no secret entry;
  requires project, location, model, and valid local Google auth.

Credential kinds are logical classes: `managed_oauth`, `secret_string`, and
`ambient_cloud_auth`.

#### Scenario: Provider catalog is requested
- **WHEN** the CLI or daemon lists available connection providers
- **THEN** alan returns provider descriptors with credential support, login
  support, validation settings, and default settings
- **AND** descriptors remain static metadata rather than operator state

#### Scenario: New provider family is added
- **WHEN** alan adds a provider family or credential kind
- **THEN** the OpenSpec delta updates this descriptor contract and any
  provider-specific capability owner before clients depend on it

### Requirement: Connection metadata and credential material are separated
alan SHALL store non-secret connection metadata separately from secret-bearing
credentials and managed login state.

V1 non-secret metadata lives in the active install channel's `connections.toml`
with this logical shape. The stable channel stores it at
`~/.alan/connections.toml`; the dev channel stores it at
`~/.alan-dev/connections.toml`.

```toml
version = 1
default_profile = "chatgpt-main"

[credentials.chatgpt]
kind = "managed_oauth"
provider_family = "chatgpt"
label = "ChatGPT login"
backend = "alan_home_auth_json"

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
- Existing ChatGPT managed login uses the managed auth store under the active
  alan home.
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

### Requirement: Connection profiles are the operator-facing provider object
alan SHALL use connection profiles as the primary operator-facing object for
provider setup and session binding.

Stable profile fields:

```text
profile_id
label?
provider
credential_id
settings{}
created_at
updated_at
source
```

Rules:

- `profile_id` is stable and operator-visible.
- `label` is display metadata and is not a stable key.
- `provider` chooses the provider request/auth surface.
- `settings` contains only non-secret provider configuration.
- `credential_id` is required unless the provider descriptor explicitly allows
  `ambient_cloud_auth` without an explicit secret-bearing credential.
- Profile settings are validated against the provider descriptor.
- Changing `provider` requires creating a new profile; patching an existing
  profile may mutate `label`, `settings`, and `credential_id` only.
- Multiple profiles may reference the same credential.
- A credential may outlive any one profile.

#### Scenario: Profile is created
- **WHEN** a profile is created through CLI, daemon, or onboarding
- **THEN** alan validates provider-specific settings against the descriptor
- **AND** stores a non-secret profile that references a credential id

#### Scenario: Provider field would change
- **WHEN** an update attempts to change an existing profile's `provider`
- **THEN** alan rejects the update and requires a new profile so session
  history and operator intent remain explicit

### Requirement: Profile resolution and session binding are deterministic
alan SHALL resolve one concrete profile for each new session and freeze that
binding for the lifetime of the session.

Profile resolution precedence for new sessions:

1. explicit session-create `profile_id`
2. workspace `agent.toml` `connection_profile`
3. global `agent.toml` `connection_profile`
4. `connections.toml` `default_profile`
5. configuration-required failure

Session binding snapshot:

```text
session_id
profile_id
provider
settings_snapshot{}
credential_snapshot_kind
resolved_model
```

Rules:

- `login` only changes credential state.
- `default set` only changes the default profile for future session creation.
- `pin` only changes `agent.toml.connection_profile` for the chosen scope.
- Existing sessions do not switch providers implicitly when credential,
  profile, pin, or default-profile state changes.
- Mutating a profile affects future sessions only.
- Session metadata shows the bound `profile_id`, `provider`, and
  `resolved_model`.
- `current` inspection surfaces report global pin, workspace pin, default
  profile, and effective profile separately.
- Onboarding sets `default_profile` but does not write a pin unless the operator
  explicitly asks to pin.

#### Scenario: Session starts with explicit profile
- **WHEN** session creation supplies `profile_id`
- **THEN** alan uses that profile over workspace pins, global pins, and default
  profile state
- **AND** persists the resolved binding metadata on the session

#### Scenario: Default profile changes after session creation
- **WHEN** an operator changes the default profile after a session starts
- **THEN** the existing session keeps its original provider binding
- **AND** future sessions use the new resolution result

#### Scenario: No profile can be resolved
- **WHEN** session creation has no explicit profile, no applicable pin, and no
  default profile
- **THEN** alan fails with a configuration-required error rather than
  synthesizing a fallback profile from legacy inline provider fields

### Requirement: Agent config pins profiles without carrying credentials
alan SHALL keep `agent.toml` as the agent-definition file and use
`connection_profile` only as an optional profile pin.

Rules:

- `connection_profile` means "pin this agent or workspace to a specific
  profile".
- Target user-facing `agent.toml` examples do not include provider-specific
  settings such as `base_url`, `model`, `account_id`, `project_id`, or
  `api_key`.
- Current code may accept legacy inline provider fields for compatibility, but
  new examples and OpenSpec changes must not extend that legacy shape.
- Runtime-only knobs such as timeouts, compaction thresholds, durability,
  memory, and skill overrides remain in `agent.toml`.

#### Scenario: Agent root pins a profile
- **WHEN** global or workspace agent config sets `connection_profile`
- **THEN** profile resolution treats it as a pin according to the deterministic
  precedence order
- **AND** provider credentials remain outside the agent-definition file

### Requirement: Connection management has a stable CLI and daemon surface
alan SHALL expose connection setup, selection, credential status, login,
secret entry, testing, and event observation through stable local-first CLI and
daemon control-plane surfaces.

Canonical daemon routes:

- `GET /api/v1/connections/catalog`
- `GET /api/v1/connections`
- `GET /api/v1/connections/current`
- `POST /api/v1/connections/default/set`
- `POST /api/v1/connections/default/clear`
- `POST /api/v1/connections/pin`
- `POST /api/v1/connections/unpin`
- `POST /api/v1/connections`
- `GET /api/v1/connections/{profile_id}`
- `PATCH /api/v1/connections/{profile_id}`
- `DELETE /api/v1/connections/{profile_id}`
- `POST /api/v1/connections/{profile_id}/activate`
- `GET /api/v1/connections/{profile_id}/credential/status`
- `POST /api/v1/connections/{profile_id}/credential/login/browser/start`
- `POST /api/v1/connections/{profile_id}/credential/login/device/start`
- `POST /api/v1/connections/{profile_id}/credential/login/device/complete`
- `POST /api/v1/connections/{profile_id}/credential/logout`
- `POST /api/v1/connections/{profile_id}/credential/secret`
- `POST /api/v1/connections/{profile_id}/test`
- `GET /api/v1/connections/events`
- `GET /api/v1/connections/events/read`

Canonical CLI namespace:

```text
alan connection list
alan connection show <profile-id>
alan connection current [--workspace <path>]
alan connection add <provider> [--profile <profile-id>]
alan connection edit <profile-id>
alan connection default set <profile-id>
alan connection default clear
alan connection pin <profile-id> [--scope global|workspace]
alan connection unpin [--scope global|workspace]
alan connection login <profile-id> browser|device
alan connection logout <profile-id>
alan connection set-secret <profile-id>
alan connection test <profile-id>
```

Required semantics:

- `catalog` returns provider descriptors only.
- `connections` returns profile summaries plus default-profile metadata.
- `default set` changes future-session defaults but never mutates existing
  sessions.
- Login routes are available only when the provider descriptor supports the
  selected credential kind.
- `credential/secret` is available only for `secret_string` credentials.
- `test` performs provider-specific dry-run or minimal validation and returns
  errors in a provider-neutral host envelope.
- `pin` and `unpin` modify `agent.toml.connection_profile` only and do not
  mutate `default_profile`.

#### Scenario: Credential login route is requested
- **WHEN** a client starts browser or device login for a profile
- **THEN** alan checks the provider descriptor for supported login modes
- **AND** unsupported login modes fail with a provider-neutral host error

#### Scenario: Connection test fails
- **WHEN** a provider-specific connection test fails
- **THEN** alan returns a provider-neutral host envelope with stable code,
  profile id, provider, retryability, and human-readable message

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

### Requirement: Provider and auth layers remain explicitly separated
alan SHALL keep provider selection, authentication, request projection, and
kernel execution in separate layers.

Layer boundaries:

- Kernel must not own browser login, device-code login, refresh-token
  persistence, account selection UX, provider billing semantics, or provider
  selection policy.
- Runtime/provider layer owns provider-specific request projection, request
  authentication headers, auth refresh and failure classification, and mapping
  provider/auth state into transport requests.
- Host/CLI layer owns login/logout commands, auth-state inspection, and secure
  persistence of managed auth state outside `agent.toml`.

Provider surfaces:

- `openai_responses`: API Platform only.
- `openai_chat_completions`: API Platform only.
- `openai_chat_completions_compatible`: generic compatible endpoints only.
- `openrouter`: OpenRouter API-key auth and SDK-backed chat surface.
- `chatgpt`: ChatGPT/Codex subscription auth surface, separate from API-key
  OpenAI providers.
- `anthropic_messages`: Anthropic Messages API-key or compatible secret.
- `google_gemini_generate_content`: Google GenerateContent with ambient cloud
  auth where configured.

Normative boundaries:

- `openai_*` providers do not depend on ChatGPT login state.
- `chatgpt` does not read API keys from OpenAI Platform config fields.
- `openai_chat_completions_compatible` remains a generic endpoint family and
  does not imply ChatGPT semantics.
- `openrouter` is not an alias for the generic compatible provider family.

#### Scenario: Provider config resolves to ChatGPT
- **WHEN** a session resolves a profile with provider `chatgpt`
- **THEN** alan uses managed ChatGPT/Codex auth state and ChatGPT-specific
  request semantics
- **AND** it does not read OpenAI API Platform secret fields

#### Scenario: Provider config resolves to OpenAI API Platform
- **WHEN** a session resolves an `openai_*` provider profile
- **THEN** alan authenticates through the profile credential secret
- **AND** it does not depend on managed ChatGPT login state

### Requirement: ChatGPT managed auth has first-class boundaries
alan SHALL treat ChatGPT/Codex subscription auth as a first-class provider/auth
surface with managed login, account/workspace context, refresh, and typed auth
errors.

Rules:

- The `chatgpt` provider may reuse Responses wire shape where compatible but
  remains distinct from `openai_responses`.
- Managed ChatGPT auth state lives outside `agent.toml` and outside Tape.
- Account/workspace identity is auth metadata, not prompt metadata.
- If a provider request requires account identity and none is available, alan
  fails with a first-class auth error before model execution.
- If launch or host policy constrains allowed account/workspace identity and
  resolved login state does not match, alan fails before model execution.
- Runtime/provider code may perform proactive refresh before dispatch.
- On an auth failure that indicates expired or invalid bearer state, alan may
  perform one managed refresh-and-retry cycle.
- Repeated auth failure surfaces as a first-class auth error, not a generic
  transport error.
- Browser login starts through the connection control plane and may complete
  through a host-owned callback bound to a pending login attempt and validated
  with OAuth state.
- Host auth observation and mutation are independently scope-gated from session
  I/O through host auth read/write scopes.

Minimum auth error family:

- not logged in
- token expired / refresh required
- refresh failed
- workspace/account mismatch
- unauthorized after refresh

#### Scenario: ChatGPT token is expired
- **WHEN** a ChatGPT request fails due to expired bearer state
- **THEN** the provider path may perform one managed refresh-and-retry cycle
- **AND** repeated failure returns a typed auth error

#### Scenario: Browser login completes through daemon callback
- **WHEN** the browser redirects to the daemon-owned callback path
- **THEN** alan validates the callback against a pending login attempt and OAuth
  state before mutating managed auth state

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

### Requirement: Connection and credential stores are channel-scoped
Alan SHALL store connection metadata, credential references, secret-bearing
credentials, managed auth state, default profile state, and provider model
overlays under the active install channel's alan home.

#### Scenario: Stable connection store is used
- **WHEN** stable-channel `alan` or `Alan.app` reads connection metadata
- **THEN** it uses `~/.alan/connections.toml`
- **AND** it uses stable-channel credential and managed-auth stores under `~/.alan`

#### Scenario: Dev connection store is used
- **WHEN** dev-channel `alan-dev` or `Alan Dev.app` reads connection metadata
- **THEN** it uses `~/.alan-dev/connections.toml`
- **AND** it uses dev-channel credential and managed-auth stores under `~/.alan-dev`
- **AND** it does not read `~/.alan/connections.toml`, stable credentials, or stable managed auth as fallback

#### Scenario: Dev channel has no configured profile
- **WHEN** a dev-channel session starts without a resolvable dev connection profile
- **THEN** Alan reports a configuration-required or onboarding-required condition
- **AND** it does not silently reuse the stable default profile

### Requirement: Cross-channel connection reuse is explicit
Alan SHALL require an explicit user action before copying or importing stable
connection profile metadata or auth material into the dev channel.

#### Scenario: User requests profile import
- **WHEN** a future import command copies a stable profile into the dev channel
- **THEN** the command identifies the source channel and target channel explicitly
- **AND** it writes new dev-channel metadata and credential references under `~/.alan-dev`
- **AND** it does not make the dev profile a live reference to stable credential storage

#### Scenario: Managed auth is reused
- **WHEN** a future command or UI flow allows managed-auth reuse across channels
- **THEN** the user must approve that operation explicitly
- **AND** the resulting dev-channel auth state is stored under the dev-channel managed auth store
- **AND** routine dev startup still does not read stable managed auth as implicit fallback
