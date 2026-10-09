## ADDED Requirements

### Requirement: Generation context reflects the selected Process directory
The Agent Execution Engine SHALL include the current selected Agent-visible
Process directory in each ordinary generation request when its existing Tool
PID-specific execution binding provides a valid UTF-8 namespace path. It SHALL refresh this
context for subsequent requests and include its prompt-token overhead. It MUST
NOT infer a Host path, rewrite historical Tape or treat selected-directory context
as a substitute for live Tool authority checks.

#### Scenario: Project directory is replaced before a task
- **WHEN** a settled Process selects a replacement namespace directory
- **THEN** its next generation request includes the newly selected directory
- **AND** the dynamic instruction does not reuse the obsolete selection

#### Scenario: A Tool changes directory during a turn
- **WHEN** a Tool updates the existing Process directory binding
- **THEN** the next generation request refreshes the selected-directory context

#### Scenario: Directory context is unavailable or contains control characters
- **WHEN** the binding is missing, non-UTF-8 or has characters requiring escaping
- **THEN** missing/non-UTF-8 context is omitted without a Host/config fallback
- **AND** available path text is represented as escaped data in the instruction
- **AND** selected directory information does not grant file or execution access

#### Scenario: A standalone default exists without a Process binding
- **WHEN** a Tool registry has a global standalone binding but the Runtime's PID has no explicit binding
- **THEN** namespace Runtime context omits that directory instead of projecting the standalone Host root
- **AND** the ordinary standalone Tool execution path retains its existing explicitly configured default behavior
