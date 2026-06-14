## ADDED Requirements

### Requirement: UPDF Provides A Standalone Agent-Friendly Harness
The Alan workspace SHALL provide a standalone `updf` binary for UPDF publishing
harness workflows without exposing UPDF as an `alan` CLI subcommand.

The `updf` binary SHALL be usable by Alan agents, Codex, CI systems, and other
automation through stable CLI commands, JSON output, machine-readable error
codes, and deterministic artifact paths.

#### Scenario: Binary is standalone
- **WHEN** a developer builds the Cargo workspace
- **THEN** the workspace includes a binary named `updf`
- **AND** UPDF workflows are not exposed as `alan updf`

#### Scenario: Commands support JSON output
- **WHEN** an automation caller invokes a supported `updf` command with `--json`
- **THEN** the command returns JSON containing status, diagnostics, artifact
  paths when applicable, and machine-readable error codes when applicable

#### Scenario: JSON paths are stable
- **WHEN** `updf` reports project artifacts in JSON
- **THEN** paths are project-relative where practical
- **AND** generated build artifacts use stable directories under `build/`

### Requirement: UPDF Builds Multiple Typst PDF Targets
The first UPDF harness implementation SHALL support Typst-first projects that
compile one source document into multiple target-specific PDF layouts.

The long-term UPDF product SHALL add a Markdown manuscript layer above the Typst
publishing backend, but the first harness slice may validate the PDF target and
package contract through Typst-first projects.

Target profiles SHALL be defined in `updf.toml` and control page size, margins,
typography scale, line height, and related layout settings.

#### Scenario: Project is initialized
- **WHEN** a user runs `updf init`
- **THEN** UPDF creates a project containing `updf.toml`, a Typst source entry,
  asset/template directories, and a recommended `templates/updf.typ` helper
  surface

#### Scenario: All targets are built
- **WHEN** a user runs `updf build`
- **THEN** UPDF compiles every configured target into a PDF under
  `build/targets/`
- **AND** the output preserves each target id in the artifact path or manifest
  metadata

#### Scenario: One target is built
- **WHEN** a user runs `updf build --target phone`
- **THEN** UPDF compiles only the requested target
- **AND** errors identify the target id when compilation fails

#### Scenario: Generic Typst project is supported
- **WHEN** a Typst source exposes a `document(target)` entrypoint without using
  the UPDF helper library
- **THEN** UPDF can generate a target wrapper and compile the configured target
- **AND** QA remains limited to generic visual and metadata checks

#### Scenario: Recommended helper library is available
- **WHEN** a project uses the generated starter template
- **THEN** it can import `templates/updf.typ`
- **AND** the helper surface provides a recommended path for target setup and
  future target-aware figure, table, code, equation, and note components

### Requirement: UPDF Distinguishes Reader Packages From Authoring Source
The default `.updf` artifact SHALL be a zip-compatible reader package that does
not include source files by default.

#### Scenario: Package is generated
- **WHEN** a user runs `updf package` after building targets
- **THEN** UPDF creates a `.updf` package containing `manifest.json`, target
  PDFs, QA report artifacts when available, thumbnails when available, and
  metadata assets when configured
- **AND** it does not include Typst source files or source assets by default

#### Scenario: Package manifest lists targets
- **WHEN** a reader inspects `manifest.json`
- **THEN** the manifest lists each target id, target PDF file path, page
  dimensions, intended device hints, and package-level QA metadata when present

#### Scenario: Package is inspectable without source
- **WHEN** a caller runs `updf inspect book.updf --json`
- **THEN** UPDF validates and reports package manifest, target files, QA summary,
  and package errors without requiring source files

#### Scenario: Unsafe archive entries are rejected
- **WHEN** a `.updf` package contains archive entries with absolute paths,
  traversal components, or missing required manifest/target files
- **THEN** package reading or inspection fails with a structured package error
- **AND** the reader does not extract or trust those unsafe paths

### Requirement: UPDF Emits Practical V0 QA Reports
UPDF SHALL produce a machine-readable QA report that helps humans and agents
inspect generated targets without claiming complete semantic document
understanding in v0.

QA v0 SHALL cover compile status, page count, blank pages, thumbnail generation,
obvious overflow or clipped-content findings when detectable, density warnings,
and manifest/package consistency.

#### Scenario: QA report is generated
- **WHEN** a user runs `updf qa`
- **THEN** UPDF writes `build/qa/report.json`
- **AND** the report includes package-level status, per-target status, page
  counts, issue lists, severity, rule ids, page numbers when applicable, and
  suggested mutation lanes when applicable

#### Scenario: Thumbnails are generated
- **WHEN** QA renders page thumbnails for a target
- **THEN** thumbnail artifacts are written under `build/qa/thumbnails/`
- **AND** the report references generated thumbnail paths when available

#### Scenario: Semantic-specific QA is deferred
- **WHEN** a project has tables, figures, code blocks, equations, or headings
- **THEN** UPDF v0 may report conservative visual or metadata findings
- **AND** it is not required to provide full semantic table, figure, code,
  equation, or heading repair diagnostics

### Requirement: UPDF Documents Agent Mutation Lanes
UPDF SHALL document and expose mutation-lane guidance so agents can decide
whether a change belongs in manuscript source, publishing templates, target
profiles, generated intermediates, or package artifacts.

#### Scenario: QA suggests mutation lanes
- **WHEN** UPDF emits a QA issue with a likely repair scope
- **THEN** the issue may include `suggested_lanes` such as `config`,
  `manuscript`, `publishing`, `generated`, or `package`
- **AND** the absence of a suggestion does not imply that source edits are
  forbidden

#### Scenario: Agents may modify Typst
- **WHEN** an agent uses UPDF to improve layout
- **THEN** the harness guidance recommends target profile edits first,
  publishing-template edits second, and manuscript edits only when the content
  or structure is itself the problem or when explicitly intended
- **AND** UPDF does not assume all multi-target layout problems can be repaired
  only through target-profile parameters
