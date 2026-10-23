# Spec Delta

## MODIFIED Requirements

### Requirement: Native shell guidance distinguishes namespace and shell operands
Model-visible native Bash Tool guidance SHALL distinguish Host-resolved Process
cwd and file-Tool namespace paths from operands embedded in unchanged shell
command text. It SHALL describe cwd-relative operands for mounted project files
and file Tools for absolute Alan namespace paths. A namespace-absolute shell
operand refusal MUST NOT be described as proof that the selected project lacks
authorization. Guidance MUST NOT expose private Host backing, add authority or
claim that arbitrary namespace paths are automatically rewritten for a shell.
Guidance SHALL explain that shell variable, command, brace and unquoted glob
expansion is unsupported even for reads. The Tool description and command schema
SHALL offer literal paths or quoted search filters as supported alternatives;
guidance MUST NOT claim that another project grant enables shell expansion.

#### Scenario: Model receives Bash guidance for a mounted project
- **WHEN** a real Agent receives the builtin Bash definition and command schema
- **THEN** both explain the supported cwd-relative shell path behavior
- **AND** the guidance distinguishes a namespace operand limitation from missing project authority

#### Scenario: A read-only source search needs a filename filter
- **WHEN** a model receives Bash guidance for source inspection
- **THEN** the guidance distinguishes an unsupported shell glob from a quoted search filter, such as `rg --files --glob '*.rs'`
- **AND** shell expansion remains rejected without requesting wider authority or changing resource enforcement

#### Scenario: Qualification follows a guidance correction
- **WHEN** Tool guidance changes after a failed task
- **THEN** the original failed attempt remains retained
- **AND** a fresh candidate runs the unchanged frozen task before claiming a qualified completion or resolved model-usage finding
