## MODIFIED Requirements

### Requirement: Package Service exposes one file-native control surface

Package Service SHALL publish a mountable handle at `/srv/package`. After
mounting that handle at `/mnt/package`, callers SHALL access `catalog`,
`status`, `ctl`, and bounded request-keyed `result` data through that
mounted tree. Mutating commands SHALL commit on clunk and malformed or
unauthorized commands SHALL make the clunk fail without a partial catalog change.

#### Scenario: Valid command commits

- **WHEN** a caller writes one valid request document to `/mnt/package/ctl` and
  clunks the fid
- **THEN** Package Service commits exactly one transaction
- **AND** the matching result can be read by request id

#### Scenario: Invalid command is written

- **WHEN** a caller writes an unknown, malformed, oversized, or duplicate
  request document
- **THEN** clunk fails
- **AND** catalog and revision state remain unchanged
