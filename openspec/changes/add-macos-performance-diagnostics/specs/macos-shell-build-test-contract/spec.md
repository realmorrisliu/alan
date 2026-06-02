## ADDED Requirements

### Requirement: Performance diagnostics have focused verification
The Apple client SHALL include focused verification for the performance
diagnostics toggle, bounded capture, export format, privacy boundary, and
behavior-neutral probe contract.

#### Scenario: Diagnostics toggle verified
- **WHEN** focused diagnostics tests exercise the Settings diagnostics toggle
- **THEN** tests verify that diagnostics are disabled by default, start
  recording only after the toggle is enabled, and stop recording after the
  toggle is disabled

#### Scenario: Export bundle verified
- **WHEN** diagnostics export is tested with retained events
- **THEN** tests verify the exported bundle contains `events.jsonl`,
  `summary.json`, app/build metadata, schema version, sampling interval, and
  capture-window metadata

#### Scenario: Privacy boundary verified
- **WHEN** diagnostics tests create terminal output, command-like text, cwd-like
  strings, path-like strings, and secret-like strings in fixtures
- **THEN** export validation verifies those values are absent from the
  diagnostics bundle
- **AND** validation verifies process samples do not include command lines,
  command arguments, cwd strings, environment variables, or raw secret values

#### Scenario: Bounded capture verified
- **WHEN** diagnostics tests generate more events than the configured retention
  limit
- **THEN** tests verify older events are evicted and diagnostics memory state
  remains bounded

#### Scenario: Behavior-neutral probes verified
- **WHEN** diagnostics are enabled in focused runtime tests
- **THEN** tests verify terminal scheduling, rendering priority, focus,
  publication, and process lifecycle results match the same scenario with
  diagnostics disabled

#### Scenario: Real-workload diagnosis captured
- **WHEN** maintainers run a real multi-Codex terminal workload for performance
  investigation
- **THEN** the recorded diagnostics summary can distinguish Alan main-thread
  long events, Ghostty tick or refresh spikes, shell projection spikes, runtime
  publication spikes, and child-process aggregate CPU pressure without
  inspecting terminal content
