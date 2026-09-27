# standalone-cli-distribution Specification

## Purpose
Defines the supported terminal distribution boundary for the standalone Alan
CLI and its linked foreground alan9 composition after the macOS desktop product
is retired.

## Requirements

### Requirement: Standalone distribution contains the foreground CLI

The supported local and release distribution SHALL provide `alan` as a
standalone executable with its linked foreground alan9 composition. It SHALL
NOT require or distribute separate product executables named `alan-os-host` or
`alan-os-host-dev`. A development alias MAY be provided as `alan-dev` and SHALL
use the existing install-channel contract. Native platform adapters and service
libraries remain part of the CLI composition.

#### Scenario: Release archive is assembled

- **WHEN** a release archive is produced for a supported target
- **THEN** it contains the standalone CLI and its `alan-dev` alias
- **AND** its manifest identifies the target and the CLI entry points
- **AND** it does not contain separate Host executables
- **AND** it does not contain `Alan.app`, a GUI executable, Sparkle metadata, or
  an appcast

#### Scenario: CLI alias is installed

- **WHEN** a developer installs the local development channel
- **THEN** `alan-dev` resolves to the development CLI entry point
- **AND** stable and development System/Host Store roots remain channel
  isolated

### Requirement: Installation is explicit and non-destructive

The repository SHALL provide one installer with a configurable destination for
the selected channel's standalone CLI. The installer MUST refuse to overwrite
or retire a non-owned or modified file and MUST NOT edit shell startup files,
user data stores, credentials, installed applications, or launchd registrations.
Before changing files, it MUST verify every existing selected-channel path it
will replace or retire against that channel's ownership manifest.

#### Scenario: Destination is empty

- **WHEN** the installer is run with an empty destination directory for a
  selected channel
- **THEN** it installs that channel's CLI entry point
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

### Requirement: Host lifecycle remains a runtime concern

Standalone installation SHALL NOT register or start an alan9 runtime. Each
ordinary `alan` invocation SHALL own one foreground alan9 instance and its
Root Agent Process. A new invocation SHALL start with an independent Root;
process exit SHALL end the instance owned by that invocation.

#### Scenario: CLI is installed but no Host is running

- **WHEN** installation completes on a machine without an active Alan instance
- **THEN** installation succeeds
- **AND** no runtime process or launchd product registration is created by the
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
- **THEN** it verifies the standalone installer and release archive contract
- **AND** it runs the CLI `--version` startup check
- **AND** it does not invoke an app-bundle, appcast, or desktop UI test

#### Scenario: CI builds a release target

- **WHEN** CI builds a supported release target
- **THEN** it uploads the standalone CLI distribution
- **AND** the build does not depend on an Xcode app archive
