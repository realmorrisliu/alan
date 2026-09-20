# product-brand-identity Specification

> Lifecycle scope (ADR-0054): Alan remains the product brand. Requirements
> describing the native macOS App/category, bundle or dev App apply only to
> retained legacy maintenance, not the terminal-neutral future product. Herdr
> is a preferred external terminal host, not an Alan rebrand.

## Purpose
Defines Alan's product-brand identity, public domain, macOS app naming,
historical AlanNative removal, and brand validation rules.

## Requirements

### Requirement: Primary public domain is alanworks.app
The product SHALL use `alanworks.app` as the primary public domain for
documentation and public project identity. Short domains such as `alan.now`
MAY be reserved for future action-oriented entry points, but the current
standalone CLI/Host distribution MUST NOT derive an app bundle identifier or
update feed from this requirement.

#### Scenario: macOS app identifier is derived
- **WHEN** retained compatibility metadata or current documentation needs the
  public project domain
- **THEN** it uses `alanworks.app` only as a documentation/project identity
- **AND** it does not create an app identifier, Sparkle feed, or appcast as a
  consequence

#### Scenario: Historical app metadata is inspected
- **WHEN** retained Apple source contains an old bundle identifier
- **THEN** it is treated as maintenance-only compatibility context
- **AND** standalone CLI/Host packaging does not depend on it

### Requirement: Terminal category is separate from shell command syntax
Alan's supported product documentation SHALL describe the terminal-neutral CLI
and Alan OS Host without presenting a native macOS app category as the current
product. The literal `alan shell ...` command namespace remains a command
surface, not a product or app name.

#### Scenario: CLI is described
- **WHEN** current docs explain the supported user-facing product
- **THEN** they describe Alan as a programmable personal computing environment
  with a terminal-native CLI/Host path
- **AND** they do not present a desktop app or `alan shell` as a separate
  product

#### Scenario: macOS app is described
- **WHEN** maintenance-only documentation explains retained native source
- **THEN** it labels that source as legacy maintenance rather than a supported
  desktop product
- **AND** it does not make the app bundle a prerequisite for the CLI/Host path

#### Scenario: CLI syntax is documented
- **WHEN** docs, help text, skills, scripts, or tests refer to the literal
  `alan shell ...` command namespace
- **THEN** that command syntax remains allowed
- **AND** the surrounding copy makes clear it is a command/control namespace,
  not the product name

### Requirement: Historical AlanNative identity is removed from active surfaces
The active repository MUST remove `AlanNative` as a product, project, target,
source-root, bundle, logging, storage, migration, or fallback-read identity
across source, docs, specs, project metadata, scripts, generated app metadata,
logs, persisted support paths, and current tests. Current Alan builds SHALL NOT
discover, read, migrate, copy, or delete state through the historical
`AlanNative` support path.

#### Scenario: Active repository is scanned
- **WHEN** the active repository excluding archived OpenSpec history is scanned
  for `AlanNative`
- **THEN** only the bounded cleanup record for this hard cut may match while the
  change is active
- **AND** no current path, project file, build command, generated product name,
  source type, log subsystem, app-support path, test fixture, or fallback reader
  depends on `AlanNative`

#### Scenario: Local state from old app exists
- **WHEN** local macOS state remains under the historical `AlanNative` support
  path after the hard cut
- **THEN** Alan for macOS does not inspect, migrate, copy, rewrite, or delete it
- **AND** current state is read and written only through current channel paths

### Requirement: Brand validation is explicit and allowlisted
The repository SHALL include a focused brand validation step that rejects
non-allowlisted uses of obsolete product names and incorrectly-cased
user-visible app branding in active surfaces.

#### Scenario: Obsolete brand name is introduced
- **WHEN** a change introduces `AlanNative`, `alanterm`, or `Alan Shell` as an
  active product/app name
- **THEN** brand validation fails with an actionable message naming the
  canonical replacement

#### Scenario: Lowercase app brand is introduced
- **WHEN** a change introduces `alan.app`, `alan for macOS`, or lowercase
  generated app display metadata in an active user-visible app surface
- **THEN** brand validation fails with an actionable message naming `Alan.app`,
  `Alan for macOS`, or `Alan` as the canonical replacement

#### Scenario: Compatibility-sensitive string is present
- **WHEN** a compatibility-sensitive surface contains `alan shell` as literal
  command syntax, lowercase `alan` as a CLI or path identifier, an archive
  contains historical references, or Swift/Rust code uses idiomatic PascalCase
  identifiers that are not user-visible brand copy
- **THEN** brand validation allows the occurrence through an explicit allowlist
  rather than requiring unsafe global replacement

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
- **THEN** they may use lowercase identifiers such as `alan`, `alan-dev`,
  `app.alanworks.macos`, and `alan-macos`
- **AND** they do not imply those lowercase identifiers are the app's
  user-visible brand spelling
