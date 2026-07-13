## ADDED Requirements

### Requirement: macOS is a Connection Service native adapter
Alan for macOS SHALL observe Connection Service native requests and perform
approved browser/device login and Keychain operations. It SHALL return only
opaque credential references and bounded results and MUST NOT maintain a second
profile/default registry.

#### Scenario: App reconnects after login
- **WHEN** the profile already exists in Connection Service
- **THEN** macOS reads its service status
- **AND** it does not recreate metadata from local preferences
