## Why

Real-model terminal acceptance after project revoke/remount used the obsolete
project path and failed both reads. Execution binding was current, but generation
requests carried no current Process directory and the model relied on history.

## What Changes

- Include the current Agent-visible selected Process directory in every ordinary
  generation request, refreshing it after Tool-driven directory changes.
- Escape the path as data, account for prompt overhead, and keep directory context
  separate from live Host Mount authority and historical Tape.
- Verify captured requests after real file/API selection, then rerun native
  revoke/remount without a manually supplied replacement path.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `agent-namespace-runtime`: current Process directory in model generation context.

## Impact

Agent Execution Engine generation assembly and focused Runtime tests. This is a
separate reliability repair discovered during terminal acceptance, not an
expansion of `refine-terminal-output-presentation`. No new grants, namespace
fallback, routing, model switching or execution lifecycle owner.
