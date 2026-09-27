## ADDED Requirements

### Requirement: Independent Package Services share channel storage safely

Independent alan9 instances SHALL be able to open the same channel Package Store.
Catalog reads and mutations SHALL observe the latest committed catalog and
serialize against other store operations. A live revision reference in any
instance SHALL prevent upgrade, uninstall or another instance's startup recovery
from reclaiming that revision. Releasing the final live reference SHALL permit
retirement. Store startup recovery SHALL discard abandoned references whose
owning operating-system process has exited, without discarding surviving ones.

#### Scenario: Two instances update the same store
- **WHEN** two Package Services install different packages concurrently
- **THEN** the committed catalog contains both packages
- **AND** both services read the same latest catalog

#### Scenario: Another instance retains an old revision
- **WHEN** one instance upgrades or uninstalls a package referenced by another
- **THEN** the other instance can still read its immutable revision
- **AND** opening another service does not reclaim that live revision
- **AND** releasing the final reference permits retired content to be removed

#### Scenario: A referencing process exits without cleanup
- **WHEN** a Package Service opens the store after a referencing process exits
  without releasing its reference through normal cleanup
- **THEN** startup recovery removes the abandoned reference and finalizes eligible
  retirement while preserving references still held by other live processes
