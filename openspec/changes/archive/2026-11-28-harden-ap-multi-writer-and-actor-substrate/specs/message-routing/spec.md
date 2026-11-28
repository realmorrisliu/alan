# Spec Delta

## ADDED Requirements

### Requirement: Routefs executes as a crash-isolated ActorFileServer
`alan-routefs` SHALL execute its routing state inside a single-owner
`FileServerActor` spawned via `spawn_file_actor`, preserving all rule-matching,
`DEAD_LETTER_PORT`, and `MAX_HANDOFF_HOPS` semantics without holding shared
`tokio::sync::Mutex` guards across `.await`.

#### Scenario: Blocked port readers do not stall send commits on RouteFs
- **WHEN** a subscriber blocks reading `/mnt/route/ports/root` while senders commit documents to `/mnt/route/send`
- **THEN** `send` commits complete immediately and wake the blocked port reader with the routed record

#### Scenario: RouteFs panic isolates failure cleanly
- **WHEN** the `RouteFs` actor encounters an injected panic inside its mailbox loop
- **THEN** in-flight and new `RouteFs` calls fail with `ErrorCode::Io` without poisoning kernel mounts and notify `ActorExitHandle`
