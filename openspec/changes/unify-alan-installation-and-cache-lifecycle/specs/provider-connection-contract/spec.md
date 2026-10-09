## MODIFIED Requirements

### Requirement: Connection metadata and credential material are separated
alan SHALL store non-secret connection metadata separately from secret-bearing
credentials and managed login state.

V1 non-secret metadata lives in the Connection Service subtree of the product
System Store as `connections.toml`, with this logical shape. Credentials
and managed login state remain in the paired Host Store.

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
- Existing ChatGPT managed login uses the managed auth owner in the paired
  Host Store.
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

### Requirement: Connection management is direct and owner-scoped
Alan SHALL retain direct `alan connection` commands for descriptor discovery, profile mutation,
default selection, secret entry, login, and connection testing. The commands SHALL operate through
the owning Connection Service metadata, Host credential/auth stores, and provider adapters. Any future
file-server management surface requires its own accepted contract.

#### Scenario: Operator lists connection profiles
- **WHEN** an operator runs `alan connection list`
- **THEN** the CLI reads the explicitly bound product connection and credential metadata owners directly
- **AND** the read-only command does not launch or mutate an Agent Process

### Requirement: Legacy connection metadata migrates once
Alan SHALL migrate non-secret legacy connection metadata into the product
System Store, verify the service-readable result, and delete the legacy file.
Credential bytes SHALL remain in the owning Host credential store and no
compatibility reader SHALL remain.

#### Scenario: Legacy profile is valid
- **WHEN** upgrade finds a valid legacy profile and credential reference
- **THEN** the metadata is imported and verified before the old file is deleted
- **AND** secret bytes are never copied into System Store

## REMOVED Requirements

### Requirement: Cross-channel connection reuse is explicit
**Reason**: There is no second runtime product channel.
**Migration**: Adopt a selected legacy store pair through the explicit installation migration contract; do not leave live credential links into the source.

### Requirement: Connection state remains channel-scoped
**Reason**: Store bindings, not executable names or install channels, determine metadata and credential ownership.
**Migration**: Use the canonical product pair or explicitly injected isolated test stores, with no legacy fallback.

## ADDED Requirements

### Requirement: Connection resolution stays within its explicit store pair
Connection and auth operations SHALL resolve metadata and credentials only through their supplied product or isolated test bindings. A missing profile SHALL fail without consulting legacy stores. Legacy adoption SHALL preserve valid credential references without writing secret bytes into System Store.

#### Scenario: Selected product store lacks a connection
- **WHEN** Agent creation cannot resolve a profile in its bound product store
- **THEN** it reports the missing connection
- **AND** it does not borrow a profile or auth state from a historical stable/dev store

#### Scenario: Legacy managed auth is adopted
- **WHEN** the operator explicitly adopts a verified legacy store pair
- **THEN** credential ownership is transferred into the canonical Host store through its owner
- **AND** profile references resolve without a live fallback to the old source
- **AND** the original source pair remains intact by default

#### Scenario: Retired runtime channel override is supplied
- **WHEN** a data-using command receives `ALAN_INSTALL_CHANNEL` or a retired `alan-dev` invocation name
- **THEN** it reports the obsolete selection before opening writable stores
- **AND** it does not reinterpret the request as authority to use personal product data
