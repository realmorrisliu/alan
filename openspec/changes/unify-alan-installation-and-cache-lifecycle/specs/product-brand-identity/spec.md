## MODIFIED Requirements

### Requirement: Canonical product brand is Alan
The product SHALL use `Alan` as the canonical standalone user-visible brand
name in app display metadata, docs headings, onboarding text, accessibility
labels, release notes, and visible command labels. Lowercase `alan` SHALL remain
available for CLI commands, package identifiers, dot directories, bundle
identifiers, storage namespaces, and other compatibility-sensitive machine
identifiers.

#### Scenario: Standalone brand is displayed
- **WHEN** a user-visible surface names the product without platform
  disambiguation
- **THEN** the surface renders the product name as `Alan`
- **AND** the surface does not render `alan`, `ALAN`, `AlanNative`, `alanterm`,
  or `alan shell` as the standalone product name

#### Scenario: Command or system identifier is displayed
- **WHEN** docs, help text, scripts, tests, paths, package metadata, or terminal
  output refer to literal command syntax or machine identifiers
- **THEN** they use `alan` for current CLI entry points and may use lowercase
  package, path and other machine identifiers
- **AND** they do not imply those lowercase identifiers are the app's
  user-visible brand spelling

#### Scenario: Migration documentation names retired commands
- **WHEN** documentation explains historical installation adoption or archived behavior
- **THEN** it may identify `alan-dev` and retired desktop identifiers as historical inputs
- **AND** it does not recommend them as a current install or runtime channel
