# Spec Delta

## ADDED Requirements

### Requirement: Cross-workstream handoffs route through Root over routefs with loop bounds
When a focused child workstream receives input or discovers a dependency belonging to another stream, it SHALL emit a typed handoff message to `/mnt/route/send` instead of calling sibling Processes directly. Root SHALL consume `/mnt/route/ports/root`, log decisions to `/mnt/route/log`, enforce `max_handoff_hops = 2` with `dead-letter` escalation, and never widen authority.

#### Scenario: Focused stream reroutes misdirected prompt through Root to target stream
- **WHEN** a user focused on stream A submits a prompt belonging to stream B
- **THEN** stream A writes a typed `reroute_request` message to `/mnt/route/send`
- **AND** Root consumes the message, appends an audit record to `/mnt/route/log`, and dispatches the task to stream B

#### Scenario: Handoff exceeding loop bound escalates to Root
- **WHEN** a routed handoff message reaches `hop_count > 2`
- **THEN** `routefs` delivers it to `dead-letter` and Root surfaces the ambiguity to the user instead of forwarding in a loop

#### Scenario: Cross-stream handoff never widens authority
- **WHEN** stream A triggers a handoff to stream B
- **THEN** stream B executes strictly under its own validated Host Mount grants and `task_budget` without inheriting extra privileges from stream A
