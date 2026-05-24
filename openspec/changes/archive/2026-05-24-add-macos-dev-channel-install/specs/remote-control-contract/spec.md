## ADDED Requirements

### Requirement: Local daemon defaults are channel-scoped
Direct-mode local daemon and client defaults SHALL be scoped by active install
channel so stable Alan and Alan Dev do not bind or connect to each other's
default daemon endpoint.

#### Scenario: Stable daemon defaults are resolved
- **WHEN** stable-channel daemon or client configuration is resolved without explicit overrides
- **THEN** the host config path is `~/.alan/host.toml`
- **AND** the stable default bind address remains `0.0.0.0:8090`
- **AND** the stable default daemon URL remains `http://127.0.0.1:8090`

#### Scenario: Dev daemon defaults are resolved
- **WHEN** dev-channel daemon or client configuration is resolved without explicit overrides
- **THEN** the host config path is `~/.alan-dev/host.toml`
- **AND** the dev default bind address is `127.0.0.1:8091`
- **AND** the dev default daemon URL is `http://127.0.0.1:8091`
- **AND** missing dev host config does not cause the dev client to connect to the stable daemon URL

#### Scenario: Channel endpoint override is used
- **WHEN** an operator explicitly supplies a daemon bind address or daemon URL override for a process
- **THEN** the override applies only to that launched process and active channel
- **AND** it does not mutate the other channel's host config
- **AND** documentation identifies overrides as advanced/debug controls rather than the primary dev-channel selection mechanism
