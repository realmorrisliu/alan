## REMOVED Requirements

### Requirement: Durable state uses a channel System Store
**Reason**: Alan has one product identity; isolated tests use explicit store bindings.
**Migration**: Explicitly adopt one historical paired store into the canonical product layout.

## MODIFIED Requirements

### Requirement: Legacy state cleanup is ownership-safe
Upgrade cleanup SHALL delete only recognized generated state automatically,
SHALL migrate and verify legacy connection metadata before deleting it, and
SHALL require explicit import before removing possibly user-authored content.
No compatibility reader SHALL remain after migration.

#### Scenario: Authored Skill source is found
- **WHEN** cleanup finds a Skill under a former implicit Host-directory source
- **THEN** it reports the source without deleting or loading it
- **AND** removal is offered only after explicit import succeeds

#### Scenario: Explicit legacy roots follow the active channel
- **WHEN** an old inspection request attempts to derive source roots from an active dev-channel override
- **THEN** the current command rejects that obsolete runtime selection
- **AND** it directs the caller to explicit historical-source inspection without changing `.alan` or `.alan-dev` content

#### Scenario: Explicit legacy roots identify historical layouts
- **WHEN** inspection receives an explicit Host project root
- **THEN** it reports `.alan`, `.agents`, `.alan-dev` and `.agents-dev` as distinct historical inputs
- **AND** no running product channel selects or loads them implicitly
- **AND** cleanup or import acts only on its explicitly identified source

### Requirement: Package Service owns installed-package persistence
Package Service SHALL keep all durable installed-package catalog, content,
provenance, digest, and transaction state in its product System Store
subtree. Package Service alone SHALL define that subtree's format. Raw backing
paths MUST NOT become package identity, namespace paths, descriptors, or client
configuration.

#### Scenario: Stable and dev install the same package id
- **WHEN** the historical source stores contain matching package IDs
- **THEN** adoption preserves only the explicitly selected source in canonical storage
- **AND** it leaves the unselected source intact without implicitly resolving its packages

#### Scenario: Independent invocations install the same package id
- **WHEN** Package Services in independent invocations publish the same package id
- **THEN** the owning service coordinates transactions in the product System Store
- **AND** immutable revisions and live references remain valid

#### Scenario: Client inspects a package
- **WHEN** a client reads installed-package state
- **THEN** it reads Package Service's mounted aP tree or a bounded package
  projection
- **AND** it does not read the raw System Store directory
## ADDED Requirements

### Requirement: Durable state uses explicit product store bindings
The Host SHALL supply one channel-free product System Store and a separate Host credential store. Durable File-Server Services SHALL own their subtrees and formats. Tests SHALL use explicit isolated roots. Raw backing paths MUST NOT become Agent identity or implicit mounts.

#### Scenario: Independent invocations persist state
- **WHEN** two Alan invocations use the default product store
- **THEN** they use the same owning service storage with existing concurrency protections
- **AND** their live Process tables, input streams, runtime endpoints and Roots remain independent

#### Scenario: Isolated test starts
- **WHEN** a test supplies temporary store bindings
- **THEN** all metadata and credentials resolve inside those bindings
- **AND** no personal product data is read or modified

### Requirement: Legacy installation adoption is explicit and paired
Adoption SHALL require an explicit historical source, verify its System/Host Store pairing, and preserve source data. It MUST NOT merge channels, overwrite existing canonical data or use legacy stores as runtime fallbacks. Inspection and dry runs MUST NOT expose secrets or modify stores.

#### Scenario: Both legacy stores exist
- **WHEN** ordinary startup finds legacy stable and dev data without canonical data
- **THEN** it reports the explicit adoption choices without creating a new identity
- **AND** adoption uses only the selected source pair and retains both original sources

#### Scenario: Canonical data already exists
- **WHEN** canonical product data is available alongside old stores
- **THEN** normal operations use only canonical data
- **AND** a different historical source cannot overwrite it through adoption

### Requirement: Store adoption is restartable and excludes live state
Adoption SHALL require quiescent writers, validate durable owners and preserve credential boundaries. All current store openers SHALL refuse incomplete publication. Interrupted adoption SHALL support resume or rollback without exposing a partial store pair. Live Process state and rebuildable scratch MUST NOT be adopted.

#### Scenario: Publication stops between stores
- **WHEN** adoption is interrupted after publishing only one destination
- **THEN** auth, metadata and runtime readers refuse the incomplete pair
- **AND** the recorded transaction permits recovery without deleting source data

#### Scenario: Source changes or schema is unknown
- **WHEN** a writer is active, source content changes, or a required owner cannot validate its schema
- **THEN** adoption fails without committing canonical data or modifying the source

#### Scenario: Completed adoption is retried
- **WHEN** the same committed source transaction is requested again
- **THEN** it verifies the receipt and every published component against its saved snapshot
- **AND** missing or changed content/permissions fails without overwriting data or making duplicate imports

### Requirement: Ephemeral service files have bounded ownership
Services SHALL retain ownership of their temporary files through their consumers' lifetime and release them on normal shutdown. Explicit cleanup SHALL report failures. Abrupt-exit reclamation SHALL target only proven generated idle roots and MUST NOT infer deletion authority solely from a PID or cache-like path.

#### Scenario: Ephemeral connection service closes
- **WHEN** the service and its consumers release temporary connection metadata
- **THEN** its metadata and lock files are removed together
- **AND** no personal credential or durable store is removed

#### Scenario: Stale scratch is inspected
- **WHEN** maintenance finds an old process-named directory without reliable generated ownership
- **THEN** it leaves that directory untouched and reports the uncertainty
