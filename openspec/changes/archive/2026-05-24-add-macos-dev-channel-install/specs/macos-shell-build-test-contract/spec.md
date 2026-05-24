## ADDED Requirements

### Requirement: Dev channel packaging has focused validation
The Apple client build/test contract SHALL include focused checks for dev
channel app metadata, embedded tools, signing, install targets, and ownership
guards when dev channel packaging changes.

#### Scenario: Dev app layout is checked
- **WHEN** dev channel packaging implementation is ready for review
- **THEN** focused checks verify `Alan Dev.app` is built
- **AND** focused checks verify the bundle identifier is `app.alanworks.macos.dev`
- **AND** focused checks verify embedded `Contents/Resources/bin/alan-dev` and `Contents/Resources/bin/alan-dev-tui` exist and are executable
- **AND** focused checks verify stable `Alan.app` packaging remains unchanged

#### Scenario: Dev install ownership is checked
- **WHEN** dev channel local install checks run
- **THEN** they verify the install creates or refreshes `Alan Dev.app`, `alan-dev`, and `alan-dev-tui`
- **AND** they verify the install does not overwrite `Alan.app`, `alan`, or `alan-tui`
- **AND** they verify dev uninstall does not remove stable app or stable command-line links

#### Scenario: Dev signing is checked
- **WHEN** dev channel local install produces an app bundle
- **THEN** focused checks verify the app and embedded tools are signed according to the local install signing policy
- **AND** checks report whether notarization was skipped because the dev channel is local-only

### Requirement: Channel isolation has focused verification
Changes to install-channel resolution SHALL include focused validation that
stable and dev channels resolve distinct runtime state, daemon, singleton, and
shell-control boundaries.

#### Scenario: Runtime paths are checked
- **WHEN** channel-aware path resolution changes
- **THEN** focused tests verify stable channel paths resolve under `~/.alan`
- **AND** focused tests verify dev channel paths resolve under `~/.alan-dev`
- **AND** tests cover host config, connections, credentials, agents, models, sessions, managed auth, registry, and global public skill sources

#### Scenario: Daemon endpoints are checked
- **WHEN** channel-aware daemon/client defaults change
- **THEN** focused tests verify stable and dev channels use distinct default daemon endpoints
- **AND** a missing dev config does not cause dev clients to connect to the stable daemon implicitly

#### Scenario: Shell-control namespaces are checked
- **WHEN** channel-aware shell-control paths change
- **THEN** focused tests verify stable and dev shell-control socket paths differ
- **AND** tests verify commands for one channel do not read binding files from the other channel

#### Scenario: Side-by-side smoke is checked
- **WHEN** dev channel support is considered ready for local use
- **THEN** maintainers can run an automated or documented manual smoke that keeps stable Alan installed while installing and launching Alan Dev
- **AND** the smoke verifies both apps can be identified independently by bundle id
- **AND** the smoke verifies dev session/config/auth state is written only to dev-channel paths
