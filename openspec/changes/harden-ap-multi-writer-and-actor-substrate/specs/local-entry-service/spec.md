# Spec Delta

## ADDED Requirements

### Requirement: Local client namespaces mount /agent with client-only write authority
`LocalEntryService::namespace_for_local_client` and local entry client
attachment paths SHALL project `/agent` through `AgentActorView::Client`,
narrowing client mutation authority to `io/input`, `machine/ctl`, and
`requests/<id>/response`.

#### Scenario: Local client cannot forge assistant output or UI snapshots
- **WHEN** a consumer using `LocalEntryService::namespace_for_local_client` opens `/agent/root/io/output` or `/agent/root/machine/ui/activity` for write
- **THEN** `open` returns `ErrorCode::NoAccess`
- **AND** reading `/agent/root/io/output` and writing `/agent/root/io/input` continue to succeed
