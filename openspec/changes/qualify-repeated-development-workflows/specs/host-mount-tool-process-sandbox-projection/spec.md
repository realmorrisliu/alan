## ADDED Requirements

### Requirement: Native shell guidance distinguishes namespace and shell operands
Model-visible native Bash Tool guidance SHALL distinguish Host-resolved Process
cwd and file-Tool namespace paths from operands embedded in unchanged shell
command text. It SHALL describe cwd-relative operands for mounted project files
and file Tools for absolute Alan namespace paths. A namespace-absolute shell
operand refusal MUST NOT be described as proof that the selected project lacks
authorization. Guidance MUST NOT expose private Host backing, add authority or
claim that arbitrary namespace paths are automatically rewritten for a shell.

#### Scenario: Model receives Bash guidance for a mounted project
- **WHEN** a real Agent receives the builtin Bash definition and command schema
- **THEN** both explain the supported cwd-relative shell path behavior
- **AND** the guidance distinguishes a namespace operand limitation from missing project authority

#### Scenario: Qualification follows a guidance correction
- **WHEN** Tool guidance changes after a failed task
- **THEN** the original failed attempt remains retained
- **AND** a fresh candidate runs the unchanged frozen task before claiming a qualified completion or resolved model-usage finding
