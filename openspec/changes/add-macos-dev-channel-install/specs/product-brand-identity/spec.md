## ADDED Requirements

### Requirement: Alan Dev is an allowlisted local development channel name
The product SHALL keep `Alan` as the public product brand while allowing
`Alan Dev` only as the user-visible name for the local dev install channel.

#### Scenario: Dev app metadata is generated
- **WHEN** the local dev macOS app bundle is built
- **THEN** `CFBundleDisplayName` and the generated app product name may use `Alan Dev`
- **AND** the bundle identifier may use `app.alanworks.macos.dev`
- **AND** these identifiers are treated as dev-channel identities rather than public rebrands

#### Scenario: Stable app metadata is generated
- **WHEN** the stable macOS app bundle is built
- **THEN** `CFBundleDisplayName` and product name remain `Alan`
- **AND** the bundle identifier remains `app.alanworks.macos`
- **AND** stable build metadata does not include `Alan Dev` or dev-channel bundle identifiers

#### Scenario: Brand validation runs
- **WHEN** brand validation scans active source, scripts, docs, project metadata, and active OpenSpec changes
- **THEN** it allows `Alan Dev`, `alan-dev`, `alan-dev-tui`, `~/.alan-dev`, `~/.agents-dev/skills`, and `app.alanworks.macos.dev` only in dev-channel contexts
- **AND** it continues to reject obsolete product names and unallowlisted lowercase user-visible app branding
