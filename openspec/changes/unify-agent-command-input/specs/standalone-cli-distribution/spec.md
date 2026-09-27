## RENAMED Requirements

- FROM: `### Requirement: Standalone distribution contains the CLI and Host binaries`
- TO: `### Requirement: Standalone distribution contains the foreground CLI`

## MODIFIED Requirements

### Requirement: Standalone distribution contains the foreground CLI

The supported local and release distribution SHALL provide `alan` as a
standalone executable with its linked foreground alan9 composition. It SHALL NOT
require or distribute independent `alan-os-host` or `alan-os-host-dev` product
executables, an app bundle or an embedded GUI. A development CLI alias MAY be
provided as `alan-dev` using the existing install-channel contract. Native
adapters and service libraries remain part of the product composition.

#### Scenario: Release archive is assembled

- **WHEN** a release archive is produced for a supported target
- **THEN** it contains the foreground CLI executable and no separate product Host executable
- **AND** it does not contain `Alan.app`, a GUI executable, Sparkle metadata, or
  an appcast
- **AND** its manifest identifies the target and each executable

#### Scenario: CLI alias is installed

- **WHEN** a developer installs the local development channel
- **THEN** `alan-dev` resolves to the development CLI entry point
- **AND** stable and development System/Host Store roots remain channel
  isolated

### Requirement: Installation is explicit and non-destructive

The repository SHALL provide one installer with an explicit destination for
the standalone binaries. The installer MUST refuse to overwrite a non-owned
file and MUST NOT edit shell startup files, user data stores, credentials,
installed applications, or launchd registrations.

For an existing installation, the installer MAY replace the selected channel's
CLI executable only when its ownership manifest records that path and its
current content matches the recorded digest. A modified executable or a
missing ownership record is a conflict; the installer MUST preserve both the
file and existing manifest. Before changing any executable or the manifest,
the installer MUST preflight all existing selected-channel paths it will
replace or retire. Any conflict MUST fail the upgrade before mutation, leaving
all existing executables and the manifest unchanged.

#### Scenario: Destination is empty

- **WHEN** the installer is run with an empty destination directory for a
  selected channel
- **THEN** it installs the selected channel's CLI entry point
- **AND** it does not install a separate Host executable
- **AND** a subsequent `alan --version` exits successfully without starting a
  Host

#### Scenario: Owned legacy Host is retired on upgrade

- **WHEN** an existing channel installation records a separate Host executable
  in its ownership manifest and is upgraded to the foreground CLI distribution
- **THEN** the installer removes that executable only if it still matches the
  previously recorded owned content, before replacing the manifest
- **AND** a modified or unowned file is preserved and reported as a conflict
  without discarding its existing ownership record
- **AND** stable and development channel ownership remain separate

#### Scenario: Upgrade replaces the manifest-owned CLI executable

- **WHEN** an existing selected-channel CLI executable matches the digest in
  its ownership manifest and the installer receives a new executable
- **THEN** the installer replaces it and records the new digest
- **AND** a modified executable or missing ownership record fails with the
  conflicting path while preserving the executable and existing manifest
- **AND** the other install channel remains unchanged

#### Scenario: Upgrade preflights the complete selected-channel install

- **WHEN** an upgrade must replace the CLI and remove a previously owned Host
  executable
- **THEN** the installer verifies every existing path it will change against
  the selected channel's ownership manifest before changing any path
- **AND** any modified or unowned executable fails with the conflicting path
  while preserving all existing executables and the manifest
- **AND** when preflight succeeds, the CLI replacement, Host removal and
  manifest update complete as one upgrade operation

#### Scenario: Upgrade is interrupted

- **WHEN** an upgrade is interrupted before its new ownership manifest is
  installed
- **THEN** the prior CLI and any retired legacy Host are restored
- **AND** the previous ownership manifest remains unchanged

#### Scenario: Destination contains an unrelated file

- **WHEN** a requested target path contains a file not owned by the installer
- **THEN** installation fails with the exact conflicting path
- **AND** the existing file is unchanged

### Requirement: Host lifecycle remains a runtime concern

Standalone installation SHALL NOT register or start an Alan runtime. Ordinary
`alan` execution SHALL own its foreground instance through existing product
composition. Metadata-only commands such as `--version` and durable connection
profile or credential operations SHALL NOT require starting that instance.
An explicitly targeted auxiliary client MAY connect to a live instance, but
installation or a missing target MUST NOT implicitly create a background Host.

#### Scenario: CLI is installed but no Host is running

- **WHEN** installation completes on a machine without an active Host
- **THEN** installation succeeds
- **AND** no Host process or launchd product registration is created by the
  installer

#### Scenario: A foreground Alan invocation runs

- **WHEN** a user starts `alan` in a terminal session
- **THEN** that invocation starts and owns its foreground instance
- **AND** another invocation has an independent Root and runtime endpoint
- **AND** exiting the invocation ends its instance

### Requirement: Quality checks cover the standalone boundary

The canonical repository quality interface and CI SHALL validate the
standalone distribution contract and SHALL NOT require retired app-bundle,
Sparkle, appcast, or Apple GUI checks.

#### Scenario: Quality gate runs

- **WHEN** the canonical quality command runs
- **THEN** it verifies CLI packaging, independent foreground startup and owned shutdown
- **AND** it does not invoke an app-bundle, appcast, or desktop UI test

#### Scenario: CI builds a release target

- **WHEN** CI builds a supported release target
- **THEN** it uploads the standalone foreground CLI executable
- **AND** the build does not depend on an Xcode app archive
