# alan-app-distribution Specification

## Purpose
Defines Alan's macOS app-first distribution contract, including bundled CLI/TUI
artifacts, Developer ID signing, notarization, Homebrew cask behavior, local
install flow, and deprecated install paths.

## Requirements
### Requirement: Alan.app is the primary macOS distribution artifact
Alan SHALL distribute macOS releases as an app-first package where `Alan.app`
contains the GUI app executable plus release CLI/TUI executables embedded under
`Contents/Resources/bin`.

#### Scenario: Release app is assembled
- **WHEN** a macOS release package is assembled
- **THEN** the package contains `Alan.app`
- **AND** the bundle contains the app executable at `Contents/MacOS/alan`
- **AND** the bundle contains the CLI at `Contents/Resources/bin/alan`
- **AND** the bundle contains the TUI at `Contents/Resources/bin/alan-tui`

#### Scenario: Version cohesion is verified
- **WHEN** a release package is validated
- **THEN** verification confirms the app, embedded CLI, and embedded TUI came from the same source revision or release version
- **AND** assembly records SHA-256 checksums after embedded CLI and TUI signing
- **AND** verification recomputes the delivered embedded CLI and TUI SHA-256 checksums and compares them with the package manifest
- **AND** verification fails if the app bundle contains stale CLI/TUI binaries from an earlier assembly

### Requirement: Distribution signing uses Developer ID
macOS release packaging SHALL sign the embedded CLI, embedded TUI, and app bundle
with a configured Developer ID Application identity. Ad-hoc signing MUST NOT be
used as a supported local install or distribution fallback.

#### Scenario: Signing identity is missing
- **WHEN** local install or release packaging runs without a configured Developer ID signing identity
- **THEN** packaging fails with an actionable error naming the required signing configuration
- **AND** packaging does not fall back to ad-hoc signing

#### Scenario: Bundle is signed
- **WHEN** release assembly signs the package
- **THEN** the embedded `alan` binary is signed before the app bundle is signed
- **AND** the embedded `alan-tui` binary is signed before the app bundle is signed
- **AND** the embedded `alan-tui` binary includes the hardened-runtime entitlement it needs to launch its standalone runtime
- **AND** the app bundle is signed after all embedded executables and resources are in their final bundle locations
- **AND** signing uses hardened runtime and timestamp options required for Developer ID distribution

### Requirement: Published artifacts are notarized
macOS artifacts intended for Homebrew cask or direct public download SHALL be
notarized and stapled before publication.

#### Scenario: Published package is produced
- **WHEN** the release process creates an artifact intended for public download or Homebrew cask
- **THEN** the artifact is notarized through Apple's notarization service
- **AND** the notarization ticket is stapled to the app bundle or distributable container when applicable
- **AND** release validation fails if notarization or stapling fails

#### Scenario: Local install runs
- **WHEN** `just install` performs a local developer install
- **THEN** the app and embedded binaries are Developer ID signed
- **AND** the local install may skip notarization when no publish artifact is produced
- **AND** the local install output states whether notarization was skipped or completed

### Requirement: Direct app installs can explicitly install CLI and TUI
Alan for macOS SHALL provide an explicit direct-install action that creates
PATH-visible `alan` and `alan-tui` entries from the embedded app resources when
Homebrew has not already provided authoritative binary links.

#### Scenario: Direct app install action is invoked
- **WHEN** a user invokes the direct app command-line tools install action
- **THEN** the app creates or refreshes `alan` and `alan-tui` symlinks that point at `Contents/Resources/bin`
- **AND** the target directory is a user-visible PATH directory such as `/usr/local/bin` or a user-selected override
- **AND** the app does not write into `~/.alan/bin`
- **AND** the app does not modify shell startup files

#### Scenario: User file would be overwritten
- **WHEN** the direct-install action finds a non-alan-owned file at a target CLI/TUI path
- **THEN** the action does not overwrite that file
- **AND** the app reports the skipped CLI/TUI install with an actionable path

#### Scenario: Homebrew links are present
- **WHEN** the app detects Homebrew-managed `alan` and `alan-tui` links for the installed app
- **THEN** the app treats Homebrew as the authoritative binary installer
- **AND** the app does not attempt to modify Homebrew's linked binaries
- **AND** the app does not create duplicate direct-install links in another PATH directory

#### Scenario: App launches directly
- **WHEN** a user launches `Alan.app` directly
- **THEN** the app does not silently install CLI/TUI entries
- **AND** the app remains usable even when command-line tools have not been installed

### Requirement: Homebrew cask installs app and binaries from one artifact
The Homebrew distribution SHALL use a cask that installs `Alan.app` and exposes
the embedded CLI/TUI binaries from inside the installed app bundle.

#### Scenario: Cask installs alan
- **WHEN** a user installs the Homebrew cask for alan
- **THEN** Homebrew installs `Alan.app`
- **AND** Homebrew links `Alan.app/Contents/Resources/bin/alan` as `alan`
- **AND** Homebrew links `Alan.app/Contents/Resources/bin/alan-tui` as `alan-tui`
- **AND** the cask does not require a separate formula to provide the CLI or TUI

#### Scenario: Cask documentation is shown
- **WHEN** install documentation describes the Homebrew path
- **THEN** it uses `brew install --cask alan` as the canonical command
- **AND** it only describes `brew install alan` as equivalent when the selected tap has no formula/cask token ambiguity

### Requirement: just install performs local release installation
`just install` SHALL install the release-shaped signed app/CLI/TUI package
locally without killing, launching, or restarting the macOS app.

#### Scenario: Local install runs
- **WHEN** a developer runs `just install`
- **THEN** the command builds the release CLI
- **AND** the command builds the release standalone TUI
- **AND** the command builds and assembles release `Alan.app`
- **AND** the command installs the app into a user-level app directory
- **AND** the command installs or refreshes CLI/TUI symlinks in a configurable PATH directory
- **AND** the command does not install CLI/TUI entries under `~/.alan/bin`

#### Scenario: App is already running
- **WHEN** `just install` runs while `Alan.app` is already running
- **THEN** the install process does not kill the app
- **AND** the install process does not launch or relaunch the app
- **AND** the install process reports that the user should restart the app manually to use the newly installed version

### Requirement: ~/.alan/bin is not a distribution path
Alan SHALL NOT install, refresh, document, or resolve `alan` or `alan-tui`
through `~/.alan/bin` as part of macOS app distribution, Homebrew cask
distribution, direct app command-line tool installation, or `just install`.

#### Scenario: Install paths are inspected
- **WHEN** local install scripts, direct app install actions, Homebrew cask metadata, and macOS command resolution paths are inspected
- **THEN** they do not use `~/.alan/bin` as a CLI/TUI install target
- **AND** they do not present `~/.alan/bin` as a PATH setup recommendation

### Requirement: just app is removed
The repository SHALL remove `just app` as a supported recipe and MUST NOT add a
replacement debug app runner recipe for the same force-rebuild-and-launch
workflow.

#### Scenario: Just recipes are listed
- **WHEN** a developer runs `just --list`
- **THEN** the listed recipes do not include `app`
- **AND** the listed recipes do not include a replacement debug app runner such as `app-debug-run`

#### Scenario: Contract checks run
- **WHEN** focused Apple contract checks inspect local app workflow scripts
- **THEN** they reject reintroducing a justfile recipe that builds, kills, and launches the app as the default local app workflow
- **AND** they accept `just install` as the supported local app installation workflow

### Requirement: macOS install channels are explicit
Alan SHALL define separate `stable` and `dev` macOS install channels. The
stable channel SHALL preserve the existing public Alan distribution identity,
while the dev channel SHALL be a local-only development install identity.

#### Scenario: Stable channel identity is inspected
- **WHEN** the stable macOS install channel is assembled or installed
- **THEN** the app bundle is `Alan.app`
- **AND** the bundle identifier is `app.alanworks.macos`
- **AND** the embedded command-line tools are exposed as `alan` and `alan-tui`
- **AND** the channel uses `~/.alan` as its default alan home

#### Scenario: Dev channel identity is inspected
- **WHEN** the dev macOS install channel is assembled or installed
- **THEN** the app bundle is `Alan Dev.app`
- **AND** the bundle identifier is `app.alanworks.macos.dev`
- **AND** the embedded command-line tools are exposed as `alan-dev` and `alan-dev-tui`
- **AND** the channel uses `~/.alan-dev` as its default alan home

### Requirement: Dev install does not overwrite stable install
The dev install workflow SHALL install and uninstall the dev channel without
modifying the stable app bundle, stable command-line links, or stable alan home.

#### Scenario: Dev local install runs
- **WHEN** a developer runs the dev local install workflow
- **THEN** the workflow installs `Alan Dev.app` into the configured user-level app directory
- **AND** the workflow installs or refreshes only `alan-dev` and `alan-dev-tui` links
- **AND** it does not replace `Alan.app`
- **AND** it does not replace `alan` or `alan-tui`
- **AND** it does not write generated data under `~/.alan`

#### Scenario: Dev local uninstall runs
- **WHEN** a developer runs the dev local uninstall workflow
- **THEN** the workflow removes `Alan Dev.app` when it is owned by the dev install
- **AND** it removes `alan-dev` and `alan-dev-tui` links when they point at the dev app bundle
- **AND** it leaves `Alan.app`, `alan`, `alan-tui`, and `~/.alan` untouched
- **AND** it leaves `~/.alan-dev` intact unless a future explicit data-removal command is added

### Requirement: Dev channel remains local-only in V1
The first dev channel implementation SHALL NOT create a public distribution
channel for Alan Dev.

#### Scenario: Public release packaging runs
- **WHEN** a public macOS release package is produced for direct download, Sparkle, or Homebrew
- **THEN** the package contains the stable `Alan.app` distribution artifacts
- **AND** it does not publish `Alan Dev.app`
- **AND** it does not publish a Homebrew cask, Sparkle feed item, or public release archive for the dev channel

#### Scenario: Dev install is documented
- **WHEN** developer documentation or just recipes describe the dev channel
- **THEN** they describe it as a local testing install
- **AND** they do not present it as a beta, nightly, or user-facing release channel
