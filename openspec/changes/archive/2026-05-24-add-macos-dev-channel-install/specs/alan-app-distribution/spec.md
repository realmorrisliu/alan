## ADDED Requirements

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
