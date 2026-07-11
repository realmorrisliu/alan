## ADDED Requirements

### Requirement: Renderer hosts project Programmable Client Surface files
An Alan renderer host SHALL render a Programmable Client Surface by reading the
mounted editable-buffer files and evaluator Process files and by translating
user intent into ordinary file operations and `run` Tool spawn. Renderer-local
cursor, viewport, hover, selection highlight, completion cache, and layout state
MAY optimize presentation but SHALL NOT become editable text, execution, or
domain authority.

#### Scenario: Renderer attaches to a programmable buffer
- **WHEN** a renderer receives a Namespace containing `/mnt/edit`, `/bin/run`,
  and `/proc`
- **THEN** it can render `body` and `tag`, project `addr`, observe `event`, and
  follow evaluator Process output without a daemon session or semantic view
  snapshot
- **AND** another file client can perform the same authoritative operations

### Requirement: Renderer discovery is a Namespace projection
Renderer hosts SHALL derive completion and selection surfaces for Paths, Tools,
Skills, commands, and writable or observable files from the mounted Namespace and
its package metadata. The renderer SHALL treat its candidate model as a
replaceable cache and SHALL NOT require a generic service UI schema.

#### Scenario: Namespace availability changes
- **WHEN** a Tool, Skill, service tree, or Path becomes absent from the
  renderer's Namespace
- **THEN** the corresponding generic candidate disappears when the projection
  refreshes
- **AND** the renderer does not preserve it as an independently authoritative
  action

### Requirement: Renderer hosts keep host and Namespace action planes distinct
Renderer hosts SHALL keep Space, Tab, Pane, window, and other host-presentation
actions separate from namespace operations such as read, write, tail, `ctl`,
and executable spawn. A renderer MAY present both action planes in one visual
surface but SHALL NOT merge their authority into a universal action registry.

#### Scenario: User executes selected buffer text
- **WHEN** a user invokes execution for the current editable range
- **THEN** the renderer spawns `/bin/run` with the bounded buffer snapshot
- **AND** it does not dispatch the selected text through a host layout action
  registry or renderer-private callback

### Requirement: Renderer hosts treat live output as a transient projection
A renderer host SHALL read a running evaluator's `/proc/<pid>/io/output` from a
Stream Offset and SHALL NOT automatically copy live `tail` bytes into editable
`body`. Explicit finite capture SHALL use ordinary Alan Shell commands and the
same result-materialization contract as a headless client.

#### Scenario: Renderer reconnects to a live evaluator
- **WHEN** a renderer reattaches while a `tail` evaluator Process remains live
- **THEN** it resumes the Process output Stream from its saved offset
- **AND** it does not reconstruct execution from renderer history or duplicate
  the live bytes into editfs
