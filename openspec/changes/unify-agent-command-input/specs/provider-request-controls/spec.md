## MODIFIED Requirements

### Requirement: Canonical reasoning effort type
alan SHALL define a shared typed reasoning effort model with lowercase
serialization values `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, and `ultra`.

#### Scenario: Parsing valid effort values
- **WHEN** config, protocol, or API payloads contain `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, or `ultra`
- **THEN** alan parses the value into the canonical reasoning effort enum

#### Scenario: Rejecting invalid effort values
- **WHEN** config, protocol, or API payloads contain an unknown reasoning effort string
- **THEN** alan rejects the value with an error that names the supported values

#### Scenario: Distinguishing unset from none
- **WHEN** reasoning effort is omitted
- **THEN** alan treats the effort as unset rather than as `none`
- **AND** `none` remains an explicit request to disable reasoning where the model supports it

#### Scenario: Provider does not support an extended effort
- **WHEN** a request uses `max` or `ultra` with an adapter that has no qualified projection for that effort
- **THEN** Alan rejects the request before provider dispatch instead of silently mapping it to another effort

## ADDED Requirements

### Requirement: Managed ChatGPT model discovery preserves account authority
Connection Service SHALL publish ChatGPT model metadata obtained through the managed provider adapter with its bound account identity. Model selection and request-control resolution SHALL use the same resolved catalog. Renderer observation SHALL NOT fetch metadata. Discovery failure SHALL preserve the original callable and SHALL NOT trigger repeated startup or observation retries.

#### Scenario: Managed catalog protocol compatibility
- **WHEN** the provider adapter requests the managed model catalog
- **THEN** it supplies an explicitly qualified catalog protocol version, independent of Alan's release version
- **AND** it preserves declared supported and default reasoning efforts for selectable models
- **AND** request time and response size are bounded, with at most one authentication refresh after a 401

#### Scenario: Catalog failure and explicit retry
- **WHEN** discovery is unavailable during Connection publication
- **THEN** the original callable remains usable and the catalog remains explicitly unavailable
- **AND** a later explicit model selection may retry discovery
- **AND** repeated Connection reads and Process status observations do not retry it

#### Scenario: Replacing the owning profile
- **WHEN** a Connection publication is replaced or revoked
- **THEN** its earlier catalog is no longer authority for new selection
- **AND** already captured input bindings retain their own configuration without silent rebinding

#### Scenario: Implicit account changes after discovery
- **WHEN** a profile omits `account_id` and its managed auth store changes from account A to B after publication
- **THEN** its published catalog and captured callables remain bound to A
- **AND** a new selection cannot install a B callable using A's catalog
- **AND** durable recovery cannot silently reinterpret an A binding as B

#### Scenario: Slow or superseded discovery
- **WHEN** catalog discovery is awaiting a provider response
- **THEN** already-published catalog observations and unrelated captures remain available
- **AND** replacing the owning publication prevents the old result from entering the new catalog
