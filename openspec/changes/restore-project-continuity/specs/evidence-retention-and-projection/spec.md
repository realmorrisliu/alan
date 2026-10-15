# Spec Delta

## ADDED Requirements

### Requirement: Authorized retained history is discoverable without Host backing access
Alan SHALL expose bounded discovery and inspection of authorized retained execution records through their owning file service. The renderer SHALL use those references rather than scan raw Host storage. Invalid or inaccessible records SHALL produce scoped diagnostics without suppressing unrelated valid history. Discovery SHALL NOT imply authority to execute or resume.

#### Scenario: History contains an invalid record
- **WHEN** an authorized user lists history with a malformed record among valid records
- **THEN** valid records remain selectable and the malformed record is diagnosed separately

#### Scenario: Project is not authorized
- **WHEN** a renderer requests history outside its accessible context
- **THEN** it receives no implicit access from cwd or a remembered project name
