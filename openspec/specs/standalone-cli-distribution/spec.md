# standalone-cli-distribution Specification

## Purpose
Defines the supported terminal-neutral distribution boundary for Alan CLI and
Alan OS Host binaries after the macOS desktop product is retired.

## Requirements

### Requirement: Standalone distribution contains the CLI and Host binaries

The supported local and release distribution SHALL provide `alan`,
`alan-os-host`, and `alan-os-host-dev` as standalone executables without an
app bundle, embedded GUI, or app-owned lifecycle. A development CLI alias MAY
be provided as `alan-dev` and SHALL use the existing install-channel contract.

#### Scenario: Release archive is assembled

- **WHEN** a release archive is produced for a supported target
- **THEN** it contains the three required executables
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
- **THEN** it installs that channel's CLI entry point and dedicated Host
  executable
- **AND** the release archive remains the distribution that contains both
  channel Host executables
- **AND** a subsequent `alan --version` exits successfully without starting a
  Host

#### Scenario: Destination contains an unrelated file

- **WHEN** a requested target path contains a file not owned by the installer
- **THEN** installation fails with the exact conflicting path
- **AND** the existing file is unchanged

### Requirement: Host lifecycle remains a runtime concern

Standalone installation SHALL not register or start an Alan OS Host. The
existing CLI Host attachment and start path remains responsible for deciding
whether to attach to or start a Host when a command needs one.

#### Scenario: CLI is installed but no Host is running

- **WHEN** installation completes on a machine without an active Host
- **THEN** installation succeeds
- **AND** no Host process or launchd product registration is created by the
  installer

#### Scenario: A Host-backed command runs later

- **WHEN** a user subsequently invokes a command that needs the Host
- **THEN** the CLI uses the existing channel-aware attachment/start path
- **AND** the installer is not re-entered as a side effect

### Requirement: Quality checks cover the standalone boundary

The canonical repository quality interface and CI SHALL validate the
standalone distribution contract and SHALL NOT require retired app-bundle,
Sparkle, appcast, or Apple GUI checks.

#### Scenario: Quality gate runs

- **WHEN** the canonical quality command runs
- **THEN** it verifies the required standalone binaries and CLI startup check
- **AND** it does not invoke an app-bundle, appcast, or desktop UI test

#### Scenario: CI builds a release target

- **WHEN** CI builds a supported release target
- **THEN** it uploads the standalone CLI and Host executables
- **AND** the build does not depend on an Xcode app archive
