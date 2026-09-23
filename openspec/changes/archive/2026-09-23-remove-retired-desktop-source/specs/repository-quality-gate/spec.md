## ADDED Requirements

### Requirement: Removed desktop implementation stays absent
The repository quality gate SHALL reject tracked Alan desktop source and
shell-core/FFI workspace membership. It SHALL continue validating standalone
CLI/Host and Rust platform safety without requiring Swift, Xcode or desktop UI.

#### Scenario: Desktop source returns
- **WHEN** a tracked file is added under clients/apple or either retired shell-core crate
- **THEN** the quality gate fails

#### Scenario: Developer retains local build artifacts
- **WHEN** ignored Apple build products remain on disk without tracked desktop source
- **THEN** their presence does not require deleting local files or running desktop verification
