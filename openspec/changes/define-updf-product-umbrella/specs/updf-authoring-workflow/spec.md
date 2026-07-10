## ADDED Requirements

### Requirement: UPDF Separates Writing From Publishing
UPDF SHALL treat author writing, publishing production, review, and reader
consumption as distinct workflow stages.

The preferred authoring source SHALL be a Markdown manuscript. Typst SHALL be
the publishing backend for layout, target-specific rendering, and PDF
generation.

#### Scenario: Author writes in Markdown
- **WHEN** an author drafts a UPDF project
- **THEN** the primary writing surface is Markdown manuscript files
- **AND** the author is not required to edit Typst layout code during normal
  drafting

#### Scenario: Publishing uses Typst
- **WHEN** UPDF publishes a manuscript
- **THEN** UPDF maps the manuscript into Typst templates, components, target
  profiles, or generated Typst before producing PDF targets
- **AND** Typst remains the source of publishing layout behavior

#### Scenario: Reader package is separate from authoring source
- **WHEN** UPDF creates a default `.updf` reader package
- **THEN** the package contains reader artifacts such as PDFs, manifest metadata,
  QA summaries, thumbnails, and metadata assets
- **AND** it does not include manuscript source or publishing source by default

### Requirement: UPDF Supports Agent-Assisted Publishing Review
UPDF SHALL support a publishing review workflow where humans review rendered
targets, QA findings, comments, and agent-proposed patches instead of manually
maintaining multiple target-specific editions. The review host SHALL open
bounded project, package, QA, preview, and writable proposal descriptors, bind
the `updf` Tool and role Skill, and spawn a role-specific Agent Executable rather
than embedding an agent engine or calling a daemon/session API.

#### Scenario: Comment is attached to preview context
- **WHEN** a reviewer comments on a target preview page or region
- **THEN** the review system records the target id, page, region when available,
  comment text, and nearby source or block identity when resolvable

#### Scenario: Agent receives bounded publishing context
- **WHEN** an agent is asked to address a review comment or QA issue
- **THEN** UPDF opens the relevant manuscript source, publishing templates,
  target profile, QA report entry, preview artifact or crop, mutation-lane
  guidance, role Skill, and `/bin/updf` into a bounded child namespace
- **AND** it spawns an Agent Executable visible through `/proc` and `/agent`

#### Scenario: Agent patch is reviewable
- **WHEN** an agent proposes a publishing fix
- **THEN** the proposal includes a source diff, affected mutation lane, reason,
  rebuilt target artifacts when available, and QA status change
- **AND** a human can accept or reject the patch before it becomes final
- **AND** the agent writes only to the authorized proposal tree until acceptance

### Requirement: UPDF Distinguishes Writing And Publishing Agent Roles
UPDF SHALL distinguish agents that edit manuscript content from agents that
adjust publishing layout.

#### Scenario: Writing agent edits manuscript
- **WHEN** an agent is acting as a writing assistant
- **THEN** it may propose Markdown manuscript edits for clarity, structure,
  citations, examples, and consistency
- **AND** it does not treat page-layout failures as permission to change the
  author's meaning

#### Scenario: Publishing agent edits layout first
- **WHEN** an agent is acting as a publishing assistant
- **THEN** it should prefer target profile and publishing-template edits for
  layout issues
- **AND** it changes manuscript content only when the content or structure is
  itself the problem or when the user explicitly requests manuscript edits

### Requirement: UPDF WYSIWYG Direction Is Semantic Source Editing
UPDF SHALL treat future WYSIWYG-like authoring as semantic source editing rather
than direct mutation of PDF pixels.

#### Scenario: Preview object is edited
- **WHEN** a user interacts with a table, figure, code block, equation, heading,
  or note in a rendered preview
- **THEN** the editor maps the interaction back to a manuscript block,
  publishing component, target profile, or Typst macro parameter
- **AND** changes are applied through source or publishing-layer edits followed
  by rebuild

#### Scenario: PDF layout is not source of truth
- **WHEN** a rendered PDF target is displayed
- **THEN** direct PDF coordinates and pixels are not treated as the persistent
  authoring source
- **AND** target-specific visual changes must round-trip through the manuscript
  or publishing layer
