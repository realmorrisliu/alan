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

#### Scenario: Destination is empty

- **WHEN** the installer is run with an empty destination directory for a
  selected channel
- **THEN** it installs the selected channel's CLI entry point
- **AND** it does not install a separate Host executable
- **AND** a subsequent `alan --version` exits successfully without starting a
  Host

#### Scenario: Destination contains an unrelated file

- **WHEN** a requested target path contains a file not owned by the installer
- **THEN** installation fails with the exact conflicting path
- **AND** the existing file is unchanged

### Requirement: Host lifecycle remains a runtime concern

Standalone installation SHALL NOT register or start an Alan runtime. Running
`alan` SHALL own its foreground instance through existing product composition.
An explicitly targeted auxiliary client MAY connect to a live instance, but
installation or a missing target MUST NOT implicitly create a background Host.

#### Scenario: CLI is installed but no Host is running

- **WHEN** installation completes on a machine without an active Host
- **THEN** installation succeeds
- **AND** no Host process or launchd product registration is created by the
  installer

#### Scenario: A Host-backed command runs later

- **WHEN** a user subsequently starts ordinary `alan`
- **THEN** the CLI boots its own foreground instance with the selected channel stores
- **AND** the installer is not re-entered as a side effect

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
