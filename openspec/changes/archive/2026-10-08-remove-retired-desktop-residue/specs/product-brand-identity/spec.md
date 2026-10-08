## MODIFIED Requirements

### Requirement: Terminal category is separate from shell command syntax
Alan's supported product documentation SHALL describe the terminal-neutral CLI
and alan9 Host without presenting a native macOS app category as the current
product. The former `alan shell ...` desktop IPC namespace SHALL be removed from the
CLI. The file-native Alan Shell remains supported and is not that desktop
control protocol.

#### Scenario: CLI is described
- **WHEN** current docs explain the supported user-facing product
- **THEN** they describe Alan as a programmable personal computing environment
  with a terminal-native CLI/Host path
- **AND** they do not present a desktop app or `alan shell` as a separate
  product

#### Scenario: macOS app is described
- **WHEN** documentation references the former native app or source
- **THEN** it labels them as retired and removed rather than a supported
  desktop product or retained maintenance surface
- **AND** it does not make the app bundle a prerequisite for the CLI/Host path

#### Scenario: CLI syntax is documented
- **WHEN** docs, help text, skills, scripts, or tests refer to the literal
  `alan shell ...` command namespace
- **THEN** they label it as retired desktop syntax or a rejection check
- **AND** they do not advertise it as a supported control command
