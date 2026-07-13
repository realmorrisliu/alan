# alan-app-distribution Specification

## Purpose
Defines Alan's macOS app-first distribution contract, including the bundled CLI
artifacts, Developer ID signing, notarization, Homebrew cask behavior, local
install flow, and deprecated install paths.
## Requirements
### Requirement: Alan.app is the primary macOS distribution artifact
Alan SHALL distribute macOS releases as an app-first package where `Alan.app`
contains the GUI app executable plus the release CLI executable embedded under
`Contents/Resources/bin`.

#### Scenario: Release app is assembled
- **WHEN** a macOS release package is assembled
- **THEN** the package contains `Alan.app`
- **AND** the bundle contains the app executable at `Contents/MacOS/alan`
- **AND** the bundle contains the CLI at `Contents/Resources/bin/alan`
- **AND** the bundle does not contain a standalone `alan-tui` executable

#### Scenario: Version cohesion is verified
- **WHEN** a release package is validated
- **THEN** verification confirms the app and embedded CLI came from the same source revision or release version
- **AND** assembly records SHA-256 checksums after embedded CLI signing
- **AND** verification recomputes the delivered embedded CLI SHA-256 checksum and compares it with the package manifest
- **AND** verification fails if the app bundle contains stale CLI binaries from an earlier assembly

### Requirement: Distribution signing uses Developer ID
macOS release packaging SHALL sign the embedded CLI and app bundle
with a configured Developer ID Application identity. Ad-hoc signing MUST NOT be
used as a supported local install or distribution fallback.

#### Scenario: Signing identity is missing
- **WHEN** local install or release packaging runs without a configured Developer ID signing identity
- **THEN** packaging fails with an actionable error naming the required signing configuration
- **AND** packaging does not fall back to ad-hoc signing

#### Scenario: Bundle is signed
- **WHEN** release assembly signs the package
- **THEN** the embedded `alan` binary is signed before the app bundle is signed
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

### Requirement: Direct app installs can explicitly install the CLI
Alan for macOS SHALL provide an explicit direct-install action that creates
PATH-visible `alan` entries from the embedded app resources when
Homebrew has not already provided authoritative binary links.

#### Scenario: Direct app install action is invoked
- **WHEN** a user invokes the direct app command-line tools install action
- **THEN** the app creates or refreshes an `alan` symlink that points at `Contents/Resources/bin`
- **AND** the target directory is a user-visible PATH directory such as `/usr/local/bin` or a user-selected override
- **AND** the app does not write into `~/.alan/bin`
- **AND** the app does not modify shell startup files

#### Scenario: User file would be overwritten
- **WHEN** the direct-install action finds a non-alan-owned file at the target CLI path
- **THEN** the action does not overwrite that file
- **AND** the app reports the skipped CLI install with an actionable path

#### Scenario: Homebrew links are present
- **WHEN** the app detects a Homebrew-managed `alan` link for the installed app
- **THEN** the app treats Homebrew as the authoritative binary installer
- **AND** the app does not attempt to modify Homebrew's linked binaries
- **AND** the app does not create duplicate direct-install links in another PATH directory

#### Scenario: App launches directly
- **WHEN** a user launches `Alan.app` directly
- **THEN** the app does not silently install CLI entries
- **AND** the app remains usable even when command-line tools have not been installed

### Requirement: Homebrew cask installs app and binaries from one artifact
The Homebrew distribution SHALL use a cask that installs `Alan.app` and exposes
the embedded CLI binary from inside the installed app bundle.

#### Scenario: Cask installs alan
- **WHEN** a user installs the Homebrew cask for alan
- **THEN** Homebrew installs `Alan.app`
- **AND** Homebrew links `Alan.app/Contents/Resources/bin/alan` as `alan`
- **AND** Homebrew does not link a standalone `alan-tui` binary
- **AND** the cask does not require a separate formula to provide the CLI

#### Scenario: Cask documentation is shown
- **WHEN** install documentation describes the Homebrew path
- **THEN** it uses `brew install --cask alan` as the canonical command
- **AND** it only describes `brew install alan` as equivalent when the selected tap has no formula/cask token ambiguity

### Requirement: just install performs local release installation
`just install` SHALL install the release-shaped signed app/CLI package
locally without killing, launching, or restarting the macOS app.

#### Scenario: Local install runs
- **WHEN** a developer runs `just install`
- **THEN** the command builds the release CLI
- **AND** the command builds and assembles release `Alan.app`
- **AND** the command installs the app into a user-level app directory
- **AND** the command installs or refreshes the CLI symlink in a configurable PATH directory
- **AND** the command does not install CLI entries under `~/.alan/bin`

#### Scenario: App is already running
- **WHEN** `just install` runs while `Alan.app` is already running
- **THEN** the install process does not kill the app
- **AND** the install process does not launch or relaunch the app
- **AND** the install process reports that the user should restart the app manually to use the newly installed version

### Requirement: ~/.alan/bin is not a distribution path
Alan SHALL NOT install, refresh, document, or resolve `alan`
through `~/.alan/bin` as part of macOS app distribution, Homebrew cask
distribution, direct app command-line tool installation, or `just install`.

#### Scenario: Install paths are inspected
- **WHEN** local install scripts, direct app install actions, Homebrew cask metadata, and macOS command resolution paths are inspected
- **THEN** they do not use `~/.alan/bin` as a CLI install target
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
- **AND** the embedded command-line tool is exposed as `alan`
- **AND** the channel uses the `stable` System Store and Host Store roots

#### Scenario: Dev channel identity is inspected
- **WHEN** the dev macOS install channel is assembled or installed
- **THEN** the app bundle is `Alan Dev.app`
- **AND** the bundle identifier is `app.alanworks.macos.dev`
- **AND** the embedded command-line tool is exposed as `alan-dev`
- **AND** the channel uses the `dev` System Store and Host Store roots

### Requirement: Dev install does not overwrite stable install
The dev install workflow SHALL install and uninstall the dev channel without
modifying the stable app bundle, stable command-line links, or stable System
Store and Host Store roots.

#### Scenario: Dev local install runs
- **WHEN** a developer runs the dev local install workflow
- **THEN** the workflow installs `Alan Dev.app` into the configured user-level app directory
- **AND** the workflow installs or refreshes only the `alan-dev` link
- **AND** it does not replace `Alan.app`
- **AND** it does not replace `alan`
- **AND** it does not write generated data to the stable System Store or Host Store

#### Scenario: Dev local uninstall runs
- **WHEN** a developer runs the dev local uninstall workflow
- **THEN** the workflow removes `Alan Dev.app` when it is owned by the dev install
- **AND** it removes the `alan-dev` link when it points at the dev app bundle
- **AND** it leaves `Alan.app`, `alan`, and both stable stores untouched
- **AND** it leaves the dev System Store and Host Store intact unless a future explicit data-removal command is added

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

### Requirement: Direct macOS installs receive Sparkle updates
Alan for macOS SHALL provide Sparkle-based update checking for directly
installed `Alan.app` bundles that are not managed by Homebrew.

#### Scenario: Direct install checks for updates
- **WHEN** a user runs a directly installed `Alan.app`
- **THEN** the app can check the Sparkle feed at `https://alanworks.app/appcast.xml`
- **AND** the app can present a user-visible update flow for available stable releases

#### Scenario: Update archive is trusted
- **WHEN** Sparkle downloads an Alan update archive
- **THEN** the archive is verified with Sparkle EdDSA update-signature metadata from the appcast
- **AND** the installed app bundle remains Developer ID signed and notarized

### Requirement: alanworks.app owns the Sparkle feed
Alan SHALL use `https://alanworks.app/appcast.xml` as the stable Sparkle feed
URL for the default stable macOS update channel.

#### Scenario: Feed URL is configured
- **WHEN** the release app bundle is built with auto-update support
- **THEN** its Sparkle feed URL resolves to `https://alanworks.app/appcast.xml`
- **AND** the app does not depend on a GitHub Pages URL for update discovery

#### Scenario: Appcast is served
- **WHEN** a client requests `https://alanworks.app/appcast.xml`
- **THEN** Cloudflare Pages serves the appcast as an XML document
- **AND** the response uses cache behavior that allows newly published releases to become visible without waiting for a long-lived static cache to expire

### Requirement: GitHub Releases own macOS update archives
Alan macOS update archives SHALL remain GitHub Release assets while
`alanworks.app` owns only the website and Sparkle appcast.

#### Scenario: Appcast references release asset
- **WHEN** an appcast item describes a stable Alan for macOS release
- **THEN** its enclosure URL points at the matching GitHub Release asset
- **AND** the asset name follows `alan-<version>-macos.zip`
- **AND** the corresponding GitHub Release includes checksum metadata for the same archive

#### Scenario: Cloudflare Pages deployment is inspected
- **WHEN** the Cloudflare Pages site for `alanworks.app` is deployed
- **THEN** it does not contain the release zip as a Pages static asset
- **AND** release archive downloads continue to resolve through GitHub Releases

### Requirement: Homebrew-managed installs use Homebrew updates
Alan for macOS SHALL NOT let Sparkle replace a Homebrew-managed app
installation.

#### Scenario: Homebrew-managed install is detected
- **WHEN** Alan detects that the current app installation lives under or resolves through a Homebrew cask path
- **THEN** Sparkle installation is disabled or the update UI directs the user to update with Homebrew
- **AND** Alan does not replace the Homebrew-managed app bundle through Sparkle
- **AND** a Homebrew-prefix command link alone does not mark a directly installed app bundle as Homebrew-managed

#### Scenario: Homebrew documentation is shown
- **WHEN** install or update documentation describes updating a Homebrew cask install
- **THEN** it uses `brew upgrade --cask alan` as the update path
- **AND** it does not tell cask users to rely on Sparkle for app bundle replacement

### Requirement: Release versions are monotonic across appcast and bundle metadata
Alan release packaging SHALL keep macOS app bundle version metadata,
GitHub release naming, release archive naming, and Sparkle appcast metadata in
sync.

#### Scenario: Release version is validated
- **WHEN** a macOS release is prepared for appcast publication
- **THEN** Cargo workspace version, Xcode `MARKETING_VERSION`, GitHub release tag, release archive filename, and appcast short version describe the same version
- **AND** Xcode `CURRENT_PROJECT_VERSION` and appcast version are monotonically greater than the previously published stable release

#### Scenario: Version drift is found
- **WHEN** release validation detects mismatched version metadata or a non-incremented Sparkle version
- **THEN** release validation fails before the appcast is deployed
- **AND** no new appcast item is published for the invalid release

### Requirement: Current installers manage only current channel artifacts
Alan install, uninstall, update, and command-line-link repair flows SHALL know
only the canonical bundle and link identities for the selected install channel.
Normal installer behavior SHALL NOT discover, stop, remove, replace, or use the
retired lowercase `alan.app` bundle.

#### Scenario: Stable install runs
- **WHEN** the stable local installer installs or updates Alan
- **THEN** it manages the channel-owned `Alan.app` bundle and canonical CLI link
- **AND** it does not inspect or delete a sibling lowercase `alan.app`

#### Scenario: Dev install runs
- **WHEN** the dev local installer installs or uninstalls Alan Dev
- **THEN** it manages only `Alan Dev.app` and the `alan-dev` link
- **AND** it does not inspect or delete `Alan.app` or lowercase `alan.app`

#### Scenario: CLI link targets a retired bundle
- **WHEN** direct-install link inspection finds a link whose destination is
  lowercase `alan.app`
- **THEN** current install logic does not treat that destination as an Alan-owned
  canonical bundle eligible for automatic replacement
- **AND** the normal installer leaves the non-canonical destination untouched

#### Scenario: Obsolete bundle path is reintroduced
- **WHEN** current installer source or tests add normal-flow handling for
  lowercase `alan.app`
- **THEN** repository verification fails outside immutable archive history and
  the bounded cleanup record
