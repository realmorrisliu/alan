# Spec Delta

## REMOVED Requirements

### Requirement: Alan.app is the primary macOS distribution artifact
**Reason**: Alan for macOS is retired as a product and no app bundle is a supported delivery artifact.
**Migration**: Use the standalone CLI/Host distribution defined by `standalone-cli-distribution`.

### Requirement: Distribution signing uses Developer ID
**Reason**: Developer ID app-bundle signing is an app-release obligation, not a standalone CLI contract.
**Migration**: Sign standalone artifacts only when a future platform distribution requires it; local CLI installation does not use app signing.

### Requirement: Published artifacts are notarized
**Reason**: The retired app publication pipeline is no longer a supported release path.
**Migration**: Publish the standalone archive through the CLI distribution contract and add platform-specific notarization only if that artifact is later shipped as a native package.

### Requirement: Direct app installs can explicitly install the CLI
**Reason**: There is no supported app bundle from which to install an embedded CLI.
**Migration**: Install the standalone `alan` binary with the repository installer.

### Requirement: Homebrew cask installs app and binaries from one artifact
**Reason**: The app cask and embedded-binary relationship is retired.
**Migration**: Use a standalone archive or a future formula that owns the CLI binaries directly.

### Requirement: just install performs local release installation
**Reason**: The old requirement coupled installation to app assembly and app lifecycle.
**Migration**: `just install` uses the standalone CLI/Host installer.

### Requirement: ~/.alan/bin is not a distribution path
**Reason**: This app-specific prohibition is superseded by the explicit standalone installer destination contract.
**Migration**: Keep the installer destination explicit and document the selected path; do not infer a path from the retired app.

### Requirement: just app is removed
**Reason**: The app runner is retired and no replacement app runner is needed.
**Migration**: Use `just build`, `just install`, and the terminal host workflow.

### Requirement: macOS install channels are explicit
**Reason**: Stable/dev app bundle identities are no longer distribution identities.
**Migration**: Preserve channel isolation in `InstallChannel`, CLI names, and System/Host Store roots.

### Requirement: Dev install does not overwrite stable install
**Reason**: There is no dev app install to protect.
**Migration**: The standalone installer protects channel-specific CLI names and store roots.

### Requirement: Dev channel remains local-only in V1
**Reason**: The app dev publication channel is retired.
**Migration**: Keep `alan-dev` as a local CLI alias only where channel-aware development is needed.

### Requirement: Direct macOS installs receive Sparkle updates
**Reason**: Sparkle is an app-bundle update mechanism and is not part of standalone CLI delivery.
**Migration**: Update standalone artifacts through the selected archive/package distribution mechanism.

### Requirement: alanworks.app owns the Sparkle feed
**Reason**: There is no supported Sparkle feed after app retirement.
**Migration**: Keep the domain available for documentation or future distribution without an appcast contract.

### Requirement: GitHub Releases own macOS update archives
**Reason**: Appcast-referenced macOS update archives are retired.
**Migration**: Release standalone target archives with their own manifest and checksum.

### Requirement: Homebrew-managed installs use Homebrew updates
**Reason**: Homebrew cask update behavior no longer applies to a retired app.
**Migration**: A future Homebrew formula, if added, owns only its standalone binaries.

### Requirement: Release versions are monotonic across appcast and bundle metadata
**Reason**: Appcast and bundle metadata do not exist in the standalone contract.
**Migration**: Keep archive version and manifest metadata coherent in the standalone release check.

### Requirement: Current installers manage only current channel artifacts
**Reason**: The old installer managed app bundles and app channel artifacts.
**Migration**: The standalone installer manages only the explicitly named CLI/Host binary targets.

## ADDED Requirements

### Requirement: Retired app distribution is not a supported surface
Alan SHALL NOT advertise or require an Alan.app bundle, embedded CLI, Sparkle
feed, appcast, or Homebrew cask as part of current product delivery. The
standalone CLI/Host distribution owns current installation and release behavior.

#### Scenario: Current release entry points are inspected
- **WHEN** a developer or operator inspects current `just`, CI, packaging, or
  release documentation
- **THEN** those entry points select the standalone CLI/Host contract
- **AND** they do not invoke app-bundle assembly, Sparkle, appcast, notarization,
  or cask publication

#### Scenario: Historical app artifacts are encountered
- **WHEN** an archived change or retained Apple source mentions Alan.app or its
  update pipeline
- **THEN** it is treated as historical or maintenance-only context
- **AND** it does not authorize a current release or installation path
