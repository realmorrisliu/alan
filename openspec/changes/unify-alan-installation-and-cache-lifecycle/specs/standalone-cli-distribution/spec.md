## MODIFIED Requirements

### Requirement: Standalone distribution contains the foreground CLI

The supported local and release distribution SHALL provide `alan` as a
standalone executable with its linked foreground alan9 composition. It SHALL
NOT require or distribute separate product executables named `alan-os-host` or
`alan-os-host-dev`. The distribution MUST NOT provide an `alan-dev` alias or a second install channel. Native platform adapters and service
libraries remain part of the CLI composition.

#### Scenario: Release archive is assembled

- **WHEN** a release archive is produced for a supported target
- **THEN** it contains only the `alan` executable and distribution metadata
- **AND** its manifest identifies the target and the CLI entry points
- **AND** it does not contain separate Host executables
- **AND** it does not contain `Alan.app`, a GUI executable, Sparkle metadata, or
  an appcast

#### Scenario: CLI alias is installed

- **WHEN** an old installation contains an owned `alan-dev` alias
- **THEN** the unified installer verifies ownership before retiring it
- **AND** it does not create another development alias or runtime channel

#### Scenario: Local development build is installed

- **WHEN** a developer explicitly installs a local build
- **THEN** it installs the same `alan` entry point
- **AND** compiler profile selection does not select a different product data identity

### Requirement: Installation is explicit and non-destructive

The repository SHALL provide one installer with a configurable destination for
the standalone `alan` CLI. The installer MUST refuse to overwrite
or retire a non-owned or modified file and MUST NOT edit shell startup files,
user data stores, credentials, installed applications, or launchd registrations.
Before changing files, it MUST verify every existing owned current or legacy installation path it
will replace or retire against the applicable verified ownership manifest.

#### Scenario: Destination is empty

- **WHEN** the installer is run with an empty destination directory for Alan
- **THEN** it installs `alan`
- **AND** it does not install a separate Host executable
- **AND** a subsequent `alan --version` exits successfully without starting a
  foreground instance

#### Scenario: Owned legacy Host is retired on upgrade

- **WHEN** an existing install manifest records a legacy Host executable and
  its current digest still matches
- **THEN** the installer removes that executable while upgrading to the
  foreground CLI distribution
- **AND** a modified or unowned path fails preflight without changing the
  existing CLI, Host, or manifest

#### Scenario: Upgrade is interrupted by a handled signal

- **WHEN** the installer handles SIGHUP or SIGTERM before installing its new
  ownership manifest
- **THEN** the prior CLI and any retired legacy Host are restored
- **AND** the previous ownership manifest remains unchanged

#### Scenario: Destination contains an unrelated file

- **WHEN** a requested target path contains a file not owned by the installer
- **THEN** installation fails with the exact conflicting path
- **AND** the existing file is unchanged

#### Scenario: Old stable and dev entry points coexist

- **WHEN** installation upgrades verified stable and dev ownership manifests
- **THEN** it publishes one ownership manifest and one `alan` entry point
- **AND** it retires only unchanged owned aliases and obsolete Host executables
- **AND** a conflict in either old installation fails preflight before any replacement
- **AND** installation does not select or migrate either old data store
