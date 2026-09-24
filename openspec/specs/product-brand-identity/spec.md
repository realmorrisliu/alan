# product-brand-identity Specification

> Lifecycle scope (ADR-0054): Alan remains the product brand. Requirements
> describing the former native macOS App/category, bundle or dev App are
> historical, not active maintenance obligations or part of the
> terminal-neutral CLI/Host product. Herdr is a preferred external terminal
> host, not an Alan rebrand.

## Purpose
Defines Alan's product-brand identity, public domain, historical macOS app
naming, AlanNative removal, and brand validation rules.

## Requirements

### Requirement: The agent operating system is named alan9
Alan SHALL use `alan9` as the canonical lowercase name for its agent operating
system, formerly called `Alan OS`. `Alan` SHALL remain the complete product
brand. System component prose SHALL use `alan9 Kernel` and `alan9 Host` for the
existing Kernel and system Host. Alan Shell and service role names SHALL retain
their existing meanings. Explanations of the system name SHALL distinguish its
Plan 9 inspiration from protocol compatibility: alan9 uses aP and does not
claim Plan 9 or 9P compatibility or a standalone bootable OS.

#### Scenario: Product and system are introduced
- **WHEN** active documentation introduces the product and its system
- **THEN** it names the product Alan and the system alan9
- **AND** lowercase alan9 is valid system branding without weakening Alan's
  capitalized product-brand rule

#### Scenario: System Host and terminal host are described
- **WHEN** documentation describes system lifetime and terminal presentation
- **THEN** it names the system authority alan9 Host
- **AND** it distinguishes that component from an external terminal host such as Herdr

#### Scenario: The system name is explained
- **WHEN** documentation explains the name alan9
- **THEN** it describes Plan 9 inspiration and the system's own aP protocol
- **AND** it does not promise 9P compatibility or a bootable replacement operating system

### Requirement: System naming preserves identifiers and historical evidence
Adoption of the alan9 system name SHALL preserve existing executable and crate
names, code identifiers, protocol names, namespace and storage paths, install
channels and OpenSpec capability IDs. Active explanatory prose SHALL adopt the
new system vocabulary. Historical ADRs, archived changes, literal identifiers,
quotations and explicit migration explanations SHALL retain their original
spelling where needed. Naming adoption MUST NOT alter Process authority,
input-routing semantics, credentials, persisted formats or service ownership.

#### Scenario: An existing installation uses the new documentation
- **WHEN** a user follows documentation after the system naming update
- **THEN** `alan` and its existing commands and store locations remain valid
- **AND** no new command alias or data migration is required

#### Scenario: A retained identifier contains the former system name
- **WHEN** a link or literal identifier uses `alan-os-host-lifecycle` or an existing Rust name
- **THEN** the identifier remains unchanged and its surrounding current prose uses alan9

#### Scenario: Historical material is inspected
- **WHEN** a reader opens an older ADR or archived OpenSpec change
- **THEN** its original terminology remains intact
- **AND** that terminology does not change its disposition or implementation status

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
- **WHEN** Git history or an archived change contains an old bundle identifier
- **THEN** it is treated as historical context, not an active maintenance
  obligation
- **AND** standalone CLI/Host packaging does not depend on it

### Requirement: Terminal category is separate from shell command syntax
Alan's supported product documentation SHALL describe the terminal-neutral CLI
and alan9 Host without presenting a native macOS app category as the current
product. The literal `alan shell ...` command namespace remains a command
surface, not a product or app name.

#### Scenario: CLI is described
- **WHEN** current docs explain the supported user-facing product
- **THEN** they describe Alan as a programmable personal computing environment
  with a terminal-native CLI/Host path
- **AND** they do not present a desktop app or `alan shell` as a separate
  product

#### Scenario: macOS app is described
- **WHEN** documentation references the former native app or source
- **THEN** it labels them as retired and removed rather than a supported
  desktop product or retained maintenance surface
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
