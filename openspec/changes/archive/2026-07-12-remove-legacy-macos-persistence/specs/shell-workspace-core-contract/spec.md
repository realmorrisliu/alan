## MODIFIED Requirements

### Requirement: Manifest semantics are portable and versioned
The shell core SHALL own portable workspace manifest semantics for the current
schema, including schema validation, default manifest creation, materialization
into workspace state, TTL pruning, and pinned and live restore snapshots. It
SHALL NOT decode or upgrade terminal-only manifests or tolerate a legacy
`quick_terminal` field.

The manifest SHALL store restorable workspace intent and SHALL NOT store
terminal process handles, PTY file descriptors, renderer objects, Ghostty
surface pointers, platform window handles, delivery queues, or unbounded
scrollback.

#### Scenario: Current manifest materializes workspace state
- **WHEN** a valid current workspace manifest is materialized by shell core
- **THEN** the resulting workspace state preserves Space, Tab, PaneSlot,
  ContentInstance, selection, split tree, lifecycle, profile reference, and
  restore snapshot semantics
- **AND** platform adapters create new terminal runtimes from returned runtime
  intents rather than restoring renderer or process objects from the manifest

#### Scenario: Unsupported manifest shape is submitted
- **WHEN** a terminal-only manifest, a manifest containing `quick_terminal`, or
  an unsupported schema version is submitted to shell core
- **THEN** shell core rejects it as unsupported
- **AND** no compatibility decoder, discard pass, or upgrade output is produced

#### Scenario: Manifest pruning runs
- **WHEN** shell core prunes unpinned inactive Tabs outside the configured
  lifecycle TTL
- **THEN** pinned Tabs and Tabs protected by active-task metadata are retained
- **AND** empty Spaces remain durable until explicitly deleted
