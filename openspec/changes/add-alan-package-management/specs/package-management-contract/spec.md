## ADDED Requirements

### Requirement: Quartermaster is the sole skill-resolution authority
alan SHALL resolve an agent's skill capability set through Quartermaster and
SHALL NOT enumerate skill source directories independently of it. Every skill
reachable by an agent SHALL be a Q package supplied by one of three provider
kinds:

- **Pre-installed provider**: alan's first-party built-in skills, reseeded into
  the package store.
- **Local-source provider**: `AgentRoot`, workspace, and user `.agents/skills/`
  skills, registered with Q at their existing location without being copied
  into the global store.
- **Distribution provider**: external repositories installed into the store.

There SHALL be no bypass discovery source. This requirement governs the
discovery *source* only; skill loading, exposure resolution, and the
one-skill-per-package rule are unchanged. Agent runtime self-discovery by
walking `/lib/pkg` is out of scope for this slice; the engine obtains its
resolved set through Q's host-side resolution interface.

#### Scenario: Engine resolves skills through Q
- **WHEN** the engine assembles an agent's skill capability set
- **THEN** it obtains the set from Quartermaster's resolution
- **AND** it does not separately scan built-in, `AgentRoot`, or `.agents/skills/`
  directories as independent sources

#### Scenario: Built-in skills are pre-installed packages
- **WHEN** alan starts with no external packages installed
- **THEN** its first-party built-in skills are present as Q pre-installed
  packages
- **AND** they are resolved through Q like any other package

#### Scenario: Agent-root skills are local-source providers
- **WHEN** an `AgentRoot` ships `skills/`
- **THEN** Q registers them as a local-source provider at their existing
  location
- **AND** they are not copied into the global store

### Requirement: Distribution packages are the unit of external adoption
alan SHALL define a distribution package as an external source tree (git
repository or local directory) pinned to a source revision token (git commit
or non-git content fingerprint), held in a per-install-channel package store, from which alan
materializes skill packages that Q resolves. Distribution packages sit above
the skill package contract: every materialized unit is an ordinary
single-skill package, and this contract SHALL NOT alter skill loading or
exposure rules.

#### Scenario: Repository installed as one package
- **WHEN** `q install` is given a git URL or local directory
- **THEN** alan records one distribution package backed by a checkout in the
  package store
- **AND** each materialized skill is a valid single-skill package under the
  existing skill package contract

#### Scenario: Default package id is unique
- **WHEN** `q install` strips a terminal `.git` suffix from the source basename,
  lowercases it, replaces each run outside `[A-Za-z0-9]` with `-`, and trims
  edge hyphens
- **THEN** that id identifies the store entry and `/lib/pkg/<package-id>/`
- **AND** the result matches `[a-z0-9]+(?:-[a-z0-9]+)*`
- **AND** `--name <package-id>` is rejected unless it already matches that
  grammar; it is not normalized, and path separators, `.`, `..`, and empty
  values are invalid
- **AND** if the id is already owned by a different source, install writes
  nothing and requires an explicit non-conflicting `--name <package-id>`

#### Scenario: Package source identity is stable
- **WHEN** Q compares an install with an existing package id
- **THEN** source identity is the canonical git URL with userinfo, query,
  fragment, and an optional terminal `.git` suffix removed for a git source,
  or the canonical absolute path for a local source
- **AND** fetch credentials are transient inputs that never enter provenance,
  reports, logs, or store files

#### Scenario: VCS metadata is not package content
- **WHEN** Q installs or upgrades a git source
- **THEN** it resolves the revision in a private staging clone and stores an
  exported working tree without `.git` or other VCS control metadata
- **AND** clone-local config, credentials, and remote URLs are not readable
  under `/lib/pkg/<package-id>/`

#### Scenario: Store is channel-scoped
- **WHEN** a package is installed under a given install channel
- **THEN** its store backing lives under that channel's alan home — the
  stable channel SHALL use `~/.alan/pkg/`, the dev channel
  `~/.alan-dev/pkg/` — and does not affect other channels
- **AND** channel isolation of resolved skills is inherited from the
  channel-scoped backing, with no write to `~/.agents/skills/`

#### Scenario: Backing is reachable only through the runtime
- **WHEN** an agent-authored command references the store backing by host path
- **THEN** the execution guard denies it as it denies any alan-home read,
  while the same content remains readable and executable through
  `/lib/pkg/<package>/`

### Requirement: Install materializes skills from the store
`q install` SHALL fetch the source into the package store and materialize skill
packages **inside the store entry** (never into `~/.agents/skills/` or any
other public skill source), where they are resolved as a distribution provider.
Channel isolation is inherited from the channel-scoped store backing. Two
materialization forms are supported in v1:

- **Conversion**: a Claude Code command-style single `.md` file (body text,
  optionally using `$ARGUMENTS`, without portable `SKILL.md` frontmatter)
  becomes a directory-backed alan skill package.
- **Adoption**: a directory containing a valid portable `SKILL.md` is
  validated against the existing skill package contract and registered in
  place as a manifest-selected root inside the exported `source/` tree, without
  content edits or a second copy.

Materialization SHALL NOT modify the source location. Q SHALL record the
accepted skill package roots in the materialization manifest and SHALL resolve
only those roots through the distribution provider; it SHALL NOT recursively
scan the package's merged `/lib/pkg` content view for skills. A skill-id
collision with a package the manifest does not own SHALL warn and skip rather
than overwrite, including when `--force` is present. When both source forms in
one distribution package yield the same skill id, conversion from the
command-style file SHALL win and the duplicate portable package SHALL be
skipped with a report entry.

#### Scenario: Command-style file is converted
- **WHEN** the package source contains a command-style `.md` skill file
- **THEN** materialization creates a skill package whose `SKILL.md` carries
  derived `name` and `description` frontmatter
- **AND** the package is resolvable through Quartermaster as a distribution
  provider

#### Scenario: Portable package is adopted
- **WHEN** the package source contains a directory with a valid `SKILL.md`
- **THEN** materialization validates it with the same rules used for discovery
- **AND** registers that directory in place as a manifest-selected skill root
  without copying or modifying its body

#### Scenario: Skill-id collision
- **WHEN** materialization would write over a skill package not owned by this
  distribution package's manifest
- **THEN** the skill is skipped with a warning naming both parties
- **AND** `--force` does not transfer ownership or mutate the other package's
  provider entry or manifest

#### Scenario: Same skill in both source forms
- **WHEN** one distribution package contains a command-style file and a
  portable package that resolve to the same skill id
- **THEN** the command-style file is converted with alan's adapter preamble,
  the portable duplicate is skipped, and the report records the choice

#### Scenario: Skipped source package is not discovered
- **WHEN** a portable `SKILL.md` in `source/` is skipped because a converted
  skill with the same id won
- **THEN** that source path is absent from the manifest-selected skill roots
- **AND** Q does not expose it to the loader even though it remains readable as
  package content under `/lib/pkg/<package-id>/`

#### Scenario: Invalid source is rejected
- **WHEN** the source is neither a readable git repository nor a local
  directory containing materializable skills
- **THEN** the install fails with a diagnostic and writes nothing to the store
  or skill sources

### Requirement: All Q providers are projected into the Alan OS namespace
alan SHALL project every Q package read-only at `/lib/pkg/<package-id>` in the
Alan OS namespace. Store-backed pre-installed and distribution providers SHALL
use the host-directory mount machinery over their store entries. Local-source
providers SHALL use read-only namespace binds to their authored package roots
without copying them into the store. `/lib/pkg/<package-id>/` is the canonical
address for package content: contracts, generated skill content, and reports
SHALL reference package content by namespace path, never by host backing path.
When a tool execution references a path under `/lib/pkg`, the execution backend
SHALL resolve it through the owning provider's projection to backing content.

Package content SHALL exclude VCS control metadata. In particular, `.git`
directories and clone-local configuration SHALL never be projected.

#### Scenario: Package content is readable through the namespace
- **WHEN** a store-backed or local-source Q package is resolved
- **THEN** an Agent Process can read its files under `/lib/pkg/<package>/`

#### Scenario: Local-source package is bound without copying
- **WHEN** Q resolves an AgentRoot, workspace, or public local-source package
- **THEN** `/lib/pkg/<package-id>/` is a read-only bind of its authored root
- **AND** no package content is copied into the channel store

#### Scenario: Helper executes via the canonical path
- **WHEN** a materialized skill invokes an interpreter on
  `/lib/pkg/<package>/tools/<helper>`
- **THEN** the execution backend resolves the path through the store
  projection and the helper runs against the installed package content

#### Scenario: Symlink escape is denied
- **WHEN** a package helper path is a symlink whose canonical target is outside
  that package's canonical store entry
- **THEN** install rejects the escaping link or execution denies the target
- **AND** the runtime does not grant the store-path guard exception

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

### Requirement: Recognized capability needs become typed dependencies
Conversion SHALL scan the source for known foreign vocabulary. A vocabulary
item with a real Alan tool or executable equivalent SHALL emit a corresponding
`capabilities.required_tools` entry. A foreign surface with no Alan equivalent
SHALL emit a `compatibility.dependencies` entry of kind `runtime_capability`;
the dependency name SHALL describe the Alan capability needed (for example
`web_access` or `multi_agent_orchestration`), not a foreign executable name.
Missing dependencies SHALL surface through the existing skill availability
reporting instead of degrading silently at runtime.

#### Scenario: Missing capability is visible, not silent
- **WHEN** a materialized skill declares a required tool or runtime capability
  the host does not provide
- **THEN** the install report lists the missing capability
- **AND** the runtime's existing availability reporting surfaces the same
  issue for the materialized skill

#### Scenario: Unsupported surface cannot be satisfied by PATH
- **WHEN** source vocabulary requires web access but Alan has no `web_access`
  runtime capability
- **THEN** conversion emits an unsatisfied `runtime_capability` dependency
- **AND** an unrelated executable named `web_search` or `web_access` on PATH
  does not make the skill available

#### Scenario: Unknown vocabulary is reported
- **WHEN** the scan finds tool-like tokens outside the known-vocabulary table
- **THEN** they are listed in the install report for human review and produce
  no frontmatter entries

### Requirement: Packages record provenance and a materialization manifest
The package store SHALL record, per distribution package: provenance (source
repository when resolvable, the source revision token — commit for git sources,
content fingerprint otherwise — and converter version) and a manifest listing
every materialized file with a content hash. The store's
provider registry and manifest are authoritative for ownership; a materialized
skill package's `package.yaml` MAY additionally carry a `provenance` block
naming the owning distribution package. Provenance and manifest are management
metadata and SHALL NOT alter runtime skill behavior.

Persisted and displayed git provenance SHALL use only the sanitized source
identity; credentials, URL userinfo, query strings, and fragments SHALL NOT be
stored or rendered.

#### Scenario: Provenance is written on install
- **WHEN** any install completes
- **THEN** the store entry contains provenance and a manifest recording which
  package owns each materialized skill

#### Scenario: Source outside version control
- **WHEN** the source is not inside a git repository
- **THEN** repository and commit fields are recorded as absent and the install
  still succeeds

### Requirement: Upgrade is idempotent and protects local modifications
`q upgrade` SHALL detect source change by a **source revision token**: the
source commit for git sources, and a content fingerprint (hash of the source
tree) for sources with no resolvable commit. `q upgrade` SHALL:

- be a no-op only when the recorded source revision token and converter version
  are both unchanged;
- re-fetch and re-materialize when the source revision token or converter
  version changed — for a non-git local directory this means a re-computed
  content fingerprint that differs from the recorded one;
- warn and skip files whose destination no longer matches the manifest content
  hash (local edits), preserving the edits unless the user passes an explicit
  force flag.

#### Scenario: Unchanged package upgraded
- **WHEN** upgrade runs with the same source revision token and converter version
- **THEN** the destination is left untouched and the report says the package
  is up to date

#### Scenario: Upstream advanced
- **WHEN** the source repository has new commits
- **THEN** upgrade re-materializes the package and updates provenance and
  manifest

#### Scenario: Non-git local source is edited
- **WHEN** a package installed from a non-git local directory is upgraded after
  its source files changed
- **THEN** the re-computed content fingerprint differs from the recorded one
- **AND** upgrade re-materializes rather than reporting the package up to date

#### Scenario: Locally modified skill
- **WHEN** a materialized file differs from its manifest hash
- **THEN** upgrade warns, skips that file, and preserves the local edits
- **AND** an explicit force flag is required to overwrite

### Requirement: Uninstall is exact and complete
`q uninstall` SHALL remove exactly the files listed in the package's manifest
plus the package's store entry, and nothing else. Because materialized files
live inside the store entry, uninstall SHALL check for divergence **before**
removing the entry: any file diverging from its manifest hash SHALL be reported
and preserved by relocating it out of the store entry (or by leaving the entry
in place around the diverged files), never deleted with the entry, unless the
user passes an explicit force flag. With force, the entire entry is removed.

#### Scenario: Clean uninstall
- **WHEN** uninstall runs for an installed package with no diverged files
- **THEN** all manifest-listed files and the store entry are removed
- **AND** skill packages not owned by the manifest are untouched

#### Scenario: Locally modified file at uninstall
- **WHEN** a manifest-listed file inside the store entry diverges from its
  recorded hash
- **THEN** uninstall preserves that file by moving it out of the entry before
  removing the rest, reports it, and does not delete it
- **AND** removing it anyway requires an explicit force flag

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
