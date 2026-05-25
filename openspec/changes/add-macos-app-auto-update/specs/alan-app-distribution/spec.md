## ADDED Requirements

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
