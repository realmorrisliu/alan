# Spec Delta

## REMOVED Requirements

### Requirement: macOS platform label is Alan for macOS
**Reason**: The native app is no longer a supported product surface.
**Migration**: Use `Alan` for the product and `Alan Shell`/CLI terminology only where the command surface requires it; refer to retained Apple code as maintenance-only.

### Requirement: Alan Dev is an allowlisted local development channel name
**Reason**: `Alan Dev` was an app bundle/channel identity.
**Migration**: Use the machine-facing `alan-dev` CLI alias only when a channel-specific development executable is required.

## MODIFIED Requirements

### Requirement: Primary public domain is alanworks.app
The product SHALL use `alanworks.app` as the primary public domain for
documentation and public project identity. Short domains such as `alan.now`
MAY be reserved for future action-oriented entry points, but the current
standalone CLI/Host distribution MUST NOT derive an app bundle identifier or
update feed from this requirement.

#### Scenario: macOS app identifier is derived
- **WHEN** retained compatibility metadata or current documentation needs the
  public project domain
- **THEN** it uses `alanworks.app` only as a documentation/project identity
- **AND** it does not create an app identifier, Sparkle feed, or appcast as a
  consequence

#### Scenario: Historical app metadata is inspected
- **WHEN** retained Apple source contains an old bundle identifier
- **THEN** it is treated as maintenance-only compatibility context
- **AND** standalone CLI/Host packaging does not depend on it

### Requirement: Terminal category is separate from shell command syntax
Alan's supported product documentation SHALL describe the terminal-neutral CLI
and Alan OS Host without presenting a native macOS app category as the current
product. The literal `alan shell ...` command namespace remains a command
surface, not a product or app name.

#### Scenario: CLI is described
- **WHEN** current docs explain the supported user-facing product
- **THEN** they describe Alan as a programmable personal computing environment
  with a terminal-native CLI/Host path
- **AND** they do not present a desktop app or `alan shell` as a separate
  product

#### Scenario: macOS app is described
- **WHEN** maintenance-only documentation explains retained native source
- **THEN** it labels that source as legacy maintenance rather than a supported
  desktop product
- **AND** it does not make the app bundle a prerequisite for the CLI/Host path

#### Scenario: CLI syntax is documented
- **WHEN** docs, help text, skills, scripts, or tests refer to the literal
  `alan shell ...` command namespace
- **THEN** that command syntax remains allowed
- **AND** the surrounding copy makes clear it is a command/control namespace,
  not the product name
