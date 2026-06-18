## ADDED Requirements

### Requirement: Workspace manifest algorithms are shell-core authoritative at runtime
The macOS shell SHALL use Rust shell core for workspace manifest defaulting,
legacy migration, lifecycle pruning, and materialization into current shell
state. Swift SHALL own manifest file IO, corrupt-file preservation, and
platform diagnostics, but SHALL NOT retain a runtime Swift implementation of
the same portable manifest algorithms after shell core covers them.

#### Scenario: Missing manifest creates default through core
- **WHEN** Alan for macOS starts and no workspace manifest exists
- **THEN** Swift asks shell core to create the default workspace manifest
- **AND** Swift writes that manifest to the macOS manifest path
- **AND** Swift does not call a separate `ShellContentWorkspaceManifest`
  defaulting algorithm as a fallback

#### Scenario: Valid manifest materializes through core
- **WHEN** Alan for macOS loads a valid workspace manifest
- **THEN** Swift asks shell core to materialize the current shell state
- **AND** the launched shell state is derived from the shell-core result
- **AND** Swift does not materialize an alternate state through a platform
  `ShellWorkspaceMaterializer`

#### Scenario: Startup pruning runs through core
- **WHEN** Alan for macOS prunes expired unpinned Tabs during startup
- **THEN** Swift asks shell core to apply the pruning semantics
- **AND** Swift persists the returned manifest when it differs from the loaded
  manifest
- **AND** Swift does not apply a separate pruning algorithm after a core failure

#### Scenario: Corrupt manifest recovery preserves evidence
- **WHEN** Alan for macOS detects an unreadable or unsupported manifest file
- **THEN** Swift quarantines the corrupt file and records recovery diagnostics
- **AND** Swift asks shell core for the replacement default manifest
- **AND** Swift does not restore from legacy shell-state snapshots as a domain
  fallback

#### Scenario: Core manifest authority fails
- **WHEN** shell core cannot create, prune, migrate, or materialize a workspace
  manifest
- **THEN** Alan for macOS reports an explicit shell-core manifest failure
- **AND** it does not silently launch from a Swift-computed workspace state for
  the same manifest

