## MODIFIED Requirements

### Requirement: Historical AlanNative identity is removed from active surfaces
The active repository MUST remove `AlanNative` as a product, project, target,
source-root, bundle, logging, storage, migration, or fallback-read identity
across source, docs, specs, project metadata, scripts, generated app metadata,
logs, persisted support paths, and current tests. Current Alan builds SHALL NOT
discover, read, migrate, copy, or delete state through the historical
`AlanNative` support path.

#### Scenario: Active repository is scanned
- **WHEN** the active repository excluding archived OpenSpec history is scanned
  for `AlanNative`
- **THEN** only the bounded cleanup record for this hard cut may match while the
  change is active
- **AND** no current path, project file, build command, generated product name,
  source type, log subsystem, app-support path, test fixture, or fallback reader
  depends on `AlanNative`

#### Scenario: Local state from old app exists
- **WHEN** local macOS state remains under the historical `AlanNative` support
  path after the hard cut
- **THEN** Alan for macOS does not inspect, migrate, copy, rewrite, or delete it
- **AND** current state is read and written only through current channel paths
