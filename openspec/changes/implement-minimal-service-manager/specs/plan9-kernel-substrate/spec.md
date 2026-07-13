## ADDED Requirements

### Requirement: Kernel boot creates the Service Manager Process
Kernel bootstrap SHALL provide the Process and namespace primitives needed for
Alan OS Host to create Service Manager as the first system Process. Kernel MUST
remain ignorant of Boot Unit, service policy, and renderer transport details.

#### Scenario: Host starts Service Manager
- **WHEN** a fresh Kernel has no committed Processes
- **THEN** Host creates Service Manager through normal Process creation
- **AND** later services appear as ordinary Process table entries
