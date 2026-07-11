# skill-system-contract — delta

## ADDED Requirements

### Requirement: Package provenance is a stable sidecar block
alan SHALL treat `provenance` as a stable, optional `package.yaml` sidecar
block identifying where the skill package's content came from and, when the
package was materialized by a distribution package, which one owns it. Field
semantics are owned by `package-management-contract`. Provenance is management
metadata: alan SHALL exclude it from runtime behavior resolution, exposure
decisions, and prompt rendering, and its absence SHALL NOT affect discovery.

#### Scenario: Provenance block is present
- **WHEN** a skill package's `package.yaml` contains a `provenance` block
- **THEN** discovery, exposure, and prompt rendering behave exactly as they
  would without it
- **AND** management surfaces may display the provenance information

#### Scenario: Provenance block is absent
- **WHEN** a skill package has no `provenance` block
- **THEN** the package remains fully valid under this contract
