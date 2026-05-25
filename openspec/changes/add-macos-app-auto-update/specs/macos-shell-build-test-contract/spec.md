## ADDED Requirements

### Requirement: Auto-update packaging has focused validation
The Apple client build/test contract SHALL provide focused validation for
Sparkle integration, appcast generation, release archive trust metadata, and
direct-install update behavior when macOS auto-update support changes.

#### Scenario: Sparkle integration is checked
- **WHEN** auto-update implementation is ready for review
- **THEN** focused checks verify the release app includes the Sparkle framework and helper code required for updates
- **AND** focused checks verify Sparkle public-key and feed URL metadata are present in the release app bundle
- **AND** focused checks verify Sparkle nested code and the final app bundle are Developer ID signed

#### Scenario: Appcast artifact is checked
- **WHEN** a release appcast is generated
- **THEN** focused checks verify `appcast.xml` references the expected GitHub Release zip asset
- **AND** focused checks verify the appcast includes Sparkle EdDSA signature metadata for the update archive
- **AND** focused checks verify the appcast version and short version match the release package metadata

#### Scenario: Cloudflare appcast headers are checked
- **WHEN** `https://alanworks.app/appcast.xml` is deployed for stable updates
- **THEN** focused checks verify the response is served as XML
- **AND** focused checks verify cache headers do not create a long-lived stale feed

#### Scenario: Direct app update smoke is checked
- **WHEN** auto-update support is considered release-ready
- **THEN** verification installs or launches an older signed and notarized `Alan.app`
- **AND** verification confirms the appcast short version and build match the newer app bundle
- **AND** verification confirms Sparkle detects the newer appcast item
- **AND** verification confirms the update downloads, verifies, installs, and relaunches into the newer version

#### Scenario: Homebrew path is checked
- **WHEN** update behavior is tested for a Homebrew-managed install
- **THEN** focused checks verify Alan does not let Sparkle replace the Homebrew-managed app bundle
- **AND** focused checks verify the user-facing update path points to Homebrew instead
