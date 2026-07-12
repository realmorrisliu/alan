# product-brand-identity Specification

## Purpose
Defines Alan's product-brand identity, public domain, macOS app naming,
historical AlanNative removal, and brand validation rules.
## Requirements
### Requirement: Primary public domain is alanworks.app
The product SHALL use `alanworks.app` as the primary public domain for this
branding pass. Short domains such as `alan.now` MAY be reserved for future
action-oriented entry points, but they MUST NOT drive macOS app identifiers in
this change.

#### Scenario: macOS app identifier is derived
- **WHEN** the macOS app bundle identifier or local automation defaults need a
  reverse-DNS identity
- **THEN** they use the selected primary domain as `app.alanworks.macos`
- **AND** they do not use `dev.alan.macos`, `dev.alan.native`, or
  `com.realmorrisliu.AlanNative`

### Requirement: Terminal category is separate from shell command syntax
Alan's user-facing product category SHALL describe the macOS app as a terminal
emulator or terminal workspace. The phrase `alan shell` MUST NOT be used as the
product name, app name, or product category.

#### Scenario: macOS app is described
- **WHEN** docs or UI explain what the native app is
- **THEN** they describe it as a terminal emulator or terminal workspace
- **AND** they do not describe the app as `alan shell`

#### Scenario: CLI syntax is documented
- **WHEN** docs, help text, skills, scripts, or tests refer to the literal
  `alan shell ...` command namespace
- **THEN** that command syntax remains allowed
- **AND** the surrounding copy makes clear it is a command/control namespace,
  not the product or app name

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
- **THEN** they may use lowercase identifiers such as `alan`,
  `~/.alan`, `app.alanworks.macos`, and `alan-macos`
- **AND** they do not imply those lowercase identifiers are the app's
  user-visible brand spelling

### Requirement: macOS platform label is Alan for macOS
The native macOS app SHALL use `Alan for macOS` as the platform variant label
when a surface needs to distinguish the macOS app from the CLI, runtime, docs,
or other future clients.

#### Scenario: Platform-specific copy is displayed
- **WHEN** a README, release note, download page, architecture doc, or support
  message distinguishes the native macOS app
- **THEN** it uses `Alan for macOS` as the platform label
- **AND** it does not introduce a second app brand such as `AlanNative` or
  `alanterm`

#### Scenario: Product name is enough
- **WHEN** Dock, app menu, window title, or default app metadata only needs the
  product name
- **THEN** it uses `Alan` instead of `Alan for macOS`

### Requirement: Alan Dev is an allowlisted local development channel name
The product SHALL keep `Alan` as the public product brand while allowing
`Alan Dev` only as the user-visible name for the local dev install channel.

#### Scenario: Dev app metadata is generated
- **WHEN** the local dev macOS app bundle is built
- **THEN** `CFBundleDisplayName` and the generated app product name may use `Alan Dev`
- **AND** the bundle identifier may use `app.alanworks.macos.dev`
- **AND** these identifiers are treated as dev-channel identities rather than public rebrands

#### Scenario: Stable app metadata is generated
- **WHEN** the stable macOS app bundle is built
- **THEN** `CFBundleDisplayName` and product name remain `Alan`
- **AND** the bundle identifier remains `app.alanworks.macos`
- **AND** stable build metadata does not include `Alan Dev` or dev-channel bundle identifiers

#### Scenario: Brand validation runs
- **WHEN** brand validation scans active source, scripts, docs, project metadata, and active OpenSpec changes
- **THEN** it allows `Alan Dev`, `alan-dev`, `~/.alan-dev`, `~/.agents-dev/skills`, and `app.alanworks.macos.dev` only in dev-channel contexts
- **AND** it continues to reject obsolete product names and unallowlisted lowercase user-visible app branding
