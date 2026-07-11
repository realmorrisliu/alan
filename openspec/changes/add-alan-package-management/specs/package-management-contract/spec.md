# package-management-contract — delta

## ADDED Requirements

### Requirement: Distribution packages are the unit of external adoption
alan SHALL define a distribution package as an external source tree (git
repository or local directory) pinned to a source commit when one is
resolvable, held in a per-install-channel package store, from which alan
materializes skill packages into existing skill sources. Distribution packages
sit above the skill package contract: every materialized unit is an ordinary
single-skill package, and this contract SHALL NOT alter skill discovery,
loading, or exposure rules.

#### Scenario: Repository installed as one package
- **WHEN** `q install` is given a git URL or local directory
- **THEN** alan records one distribution package backed by a checkout in the
  package store
- **AND** each materialized skill is a valid single-skill package under the
  existing skill package contract

#### Scenario: Store is channel-scoped
- **WHEN** a package is installed under a given install channel
- **THEN** its store backing lives under that channel's alan home — the
  stable channel SHALL use `~/.alan/pkg/`, the dev channel
  `~/.alan-dev/pkg/` — and does not affect other channels

#### Scenario: Dev-channel install respects skill-source isolation
- **WHEN** a dev-channel `q install` materializes skills without an explicit
  destination
- **THEN** the skills land under `~/.agents-dev/skills/`
- **AND** nothing is created, modified, or removed under `~/.agents/skills/`

#### Scenario: Backing is reachable only through the runtime
- **WHEN** an agent-authored command references the store backing by host path
- **THEN** the execution guard denies it as it denies any alan-home read,
  while the same content remains readable and executable through
  `/lib/pkg/<package>/`

### Requirement: Install materializes skills from the store
`q install` SHALL fetch the source into the package store and
materialize skill packages into the selected skill source, defaulting to the
channel-selected global public skill source (stable `~/.agents/skills/`, dev
`~/.agents-dev/skills/`) keyed by normalized skill id, preserving the
install-channel isolation defined by `skill-system-contract`. Two
materialization forms are supported in v1:

- **Conversion**: a Claude Code command-style single `.md` file (body text,
  optionally using `$ARGUMENTS`, without portable `SKILL.md` frontmatter)
  becomes a directory-backed alan skill package.
- **Adoption**: a directory containing a valid portable `SKILL.md` is
  validated against the existing skill package contract and copied without
  content edits.

Materialization SHALL NOT modify the source location, and a skill-id collision
with a package the manifest does not own SHALL warn and skip rather than
overwrite. When both source forms in one distribution package yield the same
skill id, conversion from the command-style file SHALL win and the duplicate
portable package SHALL be skipped with a report entry.

#### Scenario: Command-style file is converted
- **WHEN** the package source contains a command-style `.md` skill file
- **THEN** materialization creates a skill package whose `SKILL.md` carries
  derived `name` and `description` frontmatter
- **AND** the package is discoverable by the existing skill discovery rules

#### Scenario: Portable package is adopted
- **WHEN** the package source contains a directory with a valid `SKILL.md`
- **THEN** materialization validates it with the same rules used for discovery
  and copies it without modifying its body

#### Scenario: Skill-id collision
- **WHEN** materialization would write over a skill package not owned by this
  distribution package's manifest
- **THEN** the skill is skipped with a warning naming both parties, and an
  explicit force flag is required to overwrite

#### Scenario: Same skill in both source forms
- **WHEN** one distribution package contains a command-style file and a
  portable package that resolve to the same skill id
- **THEN** the command-style file is converted with alan's adapter preamble,
  the portable duplicate is skipped, and the report records the choice

#### Scenario: Invalid source is rejected
- **WHEN** the source is neither a readable git repository nor a local
  directory containing materializable skills
- **THEN** the install fails with a diagnostic and writes nothing to the store
  or skill sources

### Requirement: The package store is projected into the Alan OS namespace
alan SHALL project the package store read-only at `/lib/pkg` in the Alan OS
namespace, using the host-directory mount machinery, so that
`/lib/pkg/<package>/` exposes each installed package's content to Agent
Processes. `/lib/pkg/<package>/` is the canonical address for package content:
contracts, generated skill content, and reports SHALL reference package
content by namespace path, never by host backing path. When a tool execution
references a path under `/lib/pkg`, the execution backend SHALL resolve it
through the store projection to the backing content.

#### Scenario: Package content is readable through the namespace
- **WHEN** a package is installed
- **THEN** an Agent Process can read its files under `/lib/pkg/<package>/`

#### Scenario: Helper executes via the canonical path
- **WHEN** a materialized skill invokes an interpreter on
  `/lib/pkg/<package>/tools/<helper>`
- **THEN** the execution backend resolves the path through the store
  projection and the helper runs against the installed package content

#### Scenario: Host backing stays out of content
- **WHEN** conversion generates a preamble or a report references package
  content
- **THEN** the reference uses the `/lib/pkg` namespace path and no host
  backing path appears

### Requirement: Conversion preserves the source body verbatim
Conversion SHALL preserve the source body byte-for-byte inside the generated
`SKILL.md` and SHALL NOT rewrite, translate, or delete source prose. The only
generated additions are frontmatter and the adapter preamble.

#### Scenario: Upstream diff stays meaningful
- **WHEN** a converted skill is compared against its source file
- **THEN** the source body appears unmodified as a contiguous block, so
  upstream changes can be diffed and re-materialized mechanically

### Requirement: Conversion injects a standard adapter preamble
Conversion SHALL inject one standard, versioned adapter preamble between the
generated frontmatter and the verbatim body. The preamble SHALL:

- define `$ARGUMENTS` as the user's current request in the conversation;
- map known foreign tool vocabulary to the closest alan surface when an
  equivalent exists;
- explicitly declare foreign vocabulary with no alan equivalent as unavailable
  and instruct the skill to state that limitation to the user instead of
  improvising a substitute;
- resolve upstream-relative helper references (for example repository-root
  `tools/*.py`) to the package's canonical namespace path under
  `/lib/pkg/<package>/`, never to a host path.

The known-vocabulary mapping SHALL be converter data versioned with the
converter. Unknown tool-like tokens SHALL NOT be silently mapped.

#### Scenario: Foreign vocabulary with an alan equivalent
- **WHEN** the source references a foreign surface with an alan equivalent
  (for example file tools or shell execution)
- **THEN** the preamble names the alan surface to use

#### Scenario: Foreign vocabulary without an alan equivalent
- **WHEN** the source references a foreign surface with no alan equivalent
  (for example web search or Team orchestration tools)
- **THEN** the preamble declares the capability unavailable rather than
  mapping it to an unrelated surface

#### Scenario: Shared helper stays resolvable
- **WHEN** the source invokes a repository-root helper shipped in the same
  package
- **THEN** the materialized skill can invoke that helper at
  `/lib/pkg/<package>/...` without the user's original clone present

### Requirement: Recognized capability needs become required-tool declarations
Conversion SHALL scan the source for known foreign tool vocabulary and emit
corresponding `capabilities.required_tools` entries in the generated
frontmatter, so that missing host capabilities surface through the existing
skill availability reporting instead of degrading silently at runtime.

#### Scenario: Missing capability is visible, not silent
- **WHEN** a materialized skill declares a required tool the host does not
  provide
- **THEN** the install report lists the missing capability
- **AND** the runtime's existing availability reporting surfaces the same
  issue for the materialized skill

#### Scenario: Unknown vocabulary is reported
- **WHEN** the scan finds tool-like tokens outside the known-vocabulary table
- **THEN** they are listed in the install report for human review and produce
  no frontmatter entries

### Requirement: Packages record provenance and a materialization manifest
The package store SHALL record, per distribution package: provenance (source
repository when resolvable, source commit when resolvable, converter version)
and a manifest listing every materialized file with a content hash. Each
materialized skill package's `package.yaml` SHALL carry a `provenance` block
naming the owning distribution package. Provenance and manifest are management
metadata and SHALL NOT alter runtime skill behavior.

#### Scenario: Provenance is written on install
- **WHEN** any install completes
- **THEN** the store entry contains provenance and a manifest, and each
  materialized skill's `package.yaml` names the owning package

#### Scenario: Source outside version control
- **WHEN** the source is not inside a git repository
- **THEN** repository and commit fields are recorded as absent and the install
  still succeeds

### Requirement: Upgrade is idempotent and protects local modifications
`q upgrade` SHALL:

- be a no-op when the recorded source commit and converter version are
  unchanged;
- re-fetch and re-materialize when the upstream source or converter version
  changed;
- warn and skip files whose destination no longer matches the manifest content
  hash (local edits), preserving the edits unless the user passes an explicit
  force flag.

#### Scenario: Unchanged package upgraded
- **WHEN** upgrade runs with the same source commit and converter version
- **THEN** the destination is left untouched and the report says the package
  is up to date

#### Scenario: Upstream advanced
- **WHEN** the source repository has new commits
- **THEN** upgrade re-materializes the package and updates provenance and
  manifest

#### Scenario: Locally modified skill
- **WHEN** a materialized file differs from its manifest hash
- **THEN** upgrade warns, skips that file, and preserves the local edits
- **AND** an explicit force flag is required to overwrite

### Requirement: Uninstall is exact and complete
`q uninstall` SHALL remove exactly the files listed in the package's
manifest plus the package's store entry, and nothing else. Files diverging
from their manifest hash SHALL be reported and preserved unless the user
passes an explicit force flag.

#### Scenario: Clean uninstall
- **WHEN** uninstall runs for an installed package
- **THEN** all manifest-listed files and the store entry are removed
- **AND** skill packages not owned by the manifest are untouched

#### Scenario: Locally modified file at uninstall
- **WHEN** a manifest-listed file diverges from its recorded hash
- **THEN** uninstall preserves it, reports it, and requires an explicit force
  flag to delete it

### Requirement: List reports installed packages
`q list` SHALL report each installed distribution package with its
provenance and a summary of materialized skills, including any with
unsatisfied required tools.

#### Scenario: Packages are listed
- **WHEN** `q list` runs
- **THEN** each installed package appears with source, commit, and
  materialized-skill summary

### Requirement: Package operations produce a human-readable report
Every install/upgrade/uninstall run SHALL produce a report covering: skills
materialized, updated, or skipped (with reasons), required tools emitted,
missing host capabilities, unknown vocabulary, and skill-id collisions.

#### Scenario: Report accompanies the operation
- **WHEN** any package operation completes or partially completes
- **THEN** the report states what changed, what was skipped and why, and which
  declared capabilities the host cannot currently satisfy
