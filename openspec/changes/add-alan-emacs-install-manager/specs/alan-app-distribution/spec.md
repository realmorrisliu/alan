## ADDED Requirements

### Requirement: Alan.app Bundles Alan Emacs Distribution Resources
Alan release and development distributions SHALL provide a discoverable
Alan Emacs distribution resource that can be installed by `alan emacs install`
without relying on the source checkout path.

#### Scenario: Release resource is available
- **WHEN** a release app/CLI package is assembled
- **THEN** the package contains the Alan Emacs distribution resource
- **AND** the embedded CLI can discover that resource when running
  `alan emacs install`

#### Scenario: Development source is available
- **WHEN** the CLI runs from a source checkout during development
- **THEN** `alan emacs install` can discover `tools/alan-emacs` as a development
  source
- **AND** the normal installed user config entry still points to an
  Alan-managed installed copy rather than directly to `tools/alan-emacs`
