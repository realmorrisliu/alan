## ADDED Requirements

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
