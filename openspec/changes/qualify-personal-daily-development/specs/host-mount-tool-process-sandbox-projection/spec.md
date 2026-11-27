# Spec Delta

## MODIFIED Requirements

### Requirement: Native shell guidance distinguishes namespace and shell operands
Model-visible Bash guidance SHALL distinguish cwd-relative project paths, file-Tool namespace paths, and literal shell operands without exposing private Host paths, adding authority, or claiming automatic path rewriting or that grants enable expansion. It SHALL explain that variable, command, brace, and unquoted glob expansion are unsupported even for reads, offer literal paths or quoted filters, and never frame namespace-operand refusals as missing grants.

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
