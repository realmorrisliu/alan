## MODIFIED Requirements

### Requirement: Installed content is projected by explicit reference

Installing a package SHALL NOT itself grant runtime process, mount, network, credential, or Tool execution access. At Process creation, an explicit package reference SHALL resolve to one immutable revision handle projected read-only at `/lib/pkg/<package-id>`. Unreferenced installed packages SHALL be absent from that Process namespace. Knowledge-only (`SkillsOnly`) Root bindings MAY attach read-only `skills/` manuals to `/lib/pkg/<package-id>` without projecting executables, starting MCP servers, or launching file servers, while active runtime bindings (`Active`) explicitly enable those runtime capabilities according to Process/Operator grants. Before capability-view assembly, Process launch SHALL reject duplicate runtime Skill ids across the complete selected installed-package and explicit-descriptor set. Packages with colliding Skill ids MAY remain installed when they are not selected together. The package reference SHALL retain its File-Server handle for namespace and descriptor reads. Package Store backing MUST NOT enter Host Mount grants, native Tool authority, Agent metadata, or loader identity.

#### Scenario: Selected packages export the same runtime Skill id

- **WHEN** one Process launch selects two package or descriptor roots whose
  Skill ids normalize to the same runtime id
- **THEN** launch fails before Agent capability assembly
- **AND** neither package is made ambient or silently given precedence

#### Scenario: Process receives a package reference

- **WHEN** a parent or Boot Unit creates a Process with a valid installed
  package reference
- **THEN** the referenced revision is readable at `/lib/pkg/<package-id>`
- **AND** only manifest-selected or binding-selected Skill roots enter Agent capability resolution

#### Scenario: Process omits a package reference

- **WHEN** a package exists in the product Package Service catalog without a Root binding or explicit Process reference
- **THEN** the Process cannot read `/lib/pkg/<package-id>`
- **AND** its Agent capability view does not contain that package's Skills

#### Scenario: Process attempts to write package content

- **WHEN** a Process opens a projected package file for write
- **THEN** the namespace denies the operation

#### Scenario: Agent Runtime loads a referenced Skill

- **WHEN** Agent Runtime Service resolves a manifest-selected Skill from a
  package reference
- **THEN** it reads the Skill and package resources through namespace/aP
- **AND** no Package Store Host path or Host Mount grant is created

### Requirement: Installation grants no authority
Installing a package SHALL NOT add a mount, credential, network right, approval or standing delegation. Attaching Companion Skills (`SkillsOnly`) by default during `q install` exposes only immutable read-only instructions to Root while keeping executable, MCP, fileserver, network, and Host mount authority disabled until explicit activation. Any effect a package causes SHALL pass through Agent Runtime governance and existing grants.

#### Scenario: A tool package is installed
- **WHEN** a `tool` package is installed
- **THEN** its executable is absent from `/bin` until explicitly enabled or referenced
- **AND** any Companion Skills under `skills/` are readable without granting execution authority

#### Scenario: A fileserver package with companion skills is installed via q
- **WHEN** the operator runs `alan q install <path>` on a `fileserver` package containing `skills/*/SKILL.md`
- **THEN** Package Service catalogs the immutable revision and attaches its read-only Companion Skills to Root by default unless `--catalog-only` is passed
- **AND** no fileserver process is started, `/srv/<id>` is not published, and network/mount grants remain disabled until explicit `alan q enable`

## ADDED Requirements

### Requirement: Non-skill Quartermaster packages MAY export Companion Skills
Any `tool`, `mcp`, `provider`, or `fileserver` package SHALL preserve its single functional `kind` while optionally bundling read-only Companion Skills strictly beneath its `skills/` directory (`skills/<skill-dir>/SKILL.md` or command-style `skills/<name>.md`), installed and retired atomically with the package revision. Unrelated root markdown files such as `README.md` MUST NOT be ingested as Companion Skills.

#### Scenario: Fileserver package bundles setup and workflow Companion Skills
- **WHEN** a `fileserver` package contains `skills/github-setup/SKILL.md` and top-level `README.md`
- **THEN** Package Service records `github-setup` in the package revision's skill exports alongside its `fileserver` descriptor
- **AND** `README.md` is not exported as a skill

#### Scenario: Invalid Companion Skill fails package installation atomically
- **WHEN** a `fileserver` or `tool` package contains malformed frontmatter or duplicate skill IDs inside `skills/`
- **THEN** `q install` fails closed before updating catalog or Root bindings

### Requirement: System Store Root Package Bindings separate knowledge attachment from runtime activation
Package Service SHALL maintain versioned System Store Root bindings separating `SkillsOnly` knowledge attachment from `Active` runtime capability activation (`alan q enable` / `alan q disable`, plus per-invocation `--package` / `--fileserver` overrides), rejecting Skill ID collisions fail-closed and cleaning up bindings on uninstall. Explicit `--ephemeral` invocations passing `--package` or `--fileserver` SHALL replace global Root bindings with only those explicitly named invocation packages.

#### Scenario: Explicit enable activates a fileserver with least-privilege defaults
- **WHEN** an operator runs `alan q enable github-fileserver` without escalation flags
- **THEN** the Root binding transitions to `Active` with `ReadOnly` access and `allow_network = false` unless explicitly granted
- **AND** `alan q disable github-fileserver --keep-skills` revokes runtime activation while keeping read-only Companion Skills readable

#### Scenario: Ephemeral workflow invocation isolates package bindings
- **WHEN** an automated run invokes `alan -p ... --ephemeral --fileserver github-fileserver:rw:net`
- **THEN** only the explicitly requested package and fileserver grant are bound into that invocation
- **AND** unrelated interactive Root bindings are excluded from the isolated run
