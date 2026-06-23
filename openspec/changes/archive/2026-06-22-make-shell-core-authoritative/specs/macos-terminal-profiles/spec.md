## ADDED Requirements

### Requirement: Terminal Profile domain decisions use shell core
The macOS shell SHALL use Rust shell core for Terminal Profile validation,
editor-domain results, deterministic profile resolution, and terminal launch
intent construction once those operations are exposed through the shell-core
facade.

Swift SHALL continue to own profile file storage, corrupt-file preservation,
process spawning, privileged helper readiness checks, and user-interface
presentation.

#### Scenario: Profile definition is validated
- **WHEN** Swift validates or creates a Terminal Profile definition
- **THEN** the domain validation result comes from shell core
- **AND** Swift does not maintain a separate validation implementation for the
  same profile fields

#### Scenario: Terminal launch intent is resolved
- **WHEN** a terminal is created with an explicit, Space, content, global
  default, or fallback profile reference
- **THEN** Swift asks shell core to resolve the launch intent
- **AND** Swift translates the returned intent into macOS process or helper
  startup behavior

#### Scenario: Core profile resolution fails
- **WHEN** shell core cannot resolve a Terminal Profile launch intent because
  the facade fails or the payload is invalid
- **THEN** Swift reports an explicit profile-resolution failure
- **AND** Swift does not silently run a duplicate profile resolution algorithm
  for the same launch request

