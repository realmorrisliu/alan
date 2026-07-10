## Why

Prompt-facing truncation used to erase the evidence behind an answer (observed
in the superseded `harden-agent-operating-system-contracts` change, archived
2026-07-10). The substrate now provides most of the fix structurally —
`io/output` and `machine/tape` are append-only and complete
(`agent-file-layout-contract`), and `content-addressed-knowledge` owns
retention/GC and tamper-evidence — but three gaps remain: tape records still
reference full outputs by raw host path (or not at all), outputs can become
unreadable after the producing process exits, and there is no redaction
contract for durable evidence.

## What Changes

- Define prompt projection for oversized outputs: when a tool or child output
  exceeds the prompt-facing budget, the tape record carries a bounded preview,
  a namespace-path reference to the full content, and truncation metadata.
  References are namespace paths (e.g. an action's output file or a child's
  `io/output` with offset), never raw host filesystem paths and never a new
  artifact-id API (the iron law: extension is a file op/mount, not a
  side-channel API).
- Require delegated `output_ref` values to be namespace paths readable in the
  parent's namespace, replacing the raw rollout-path escape hatch.
- Define post-exit readability: action output files and child output streams
  referenced from a tape remain readable, subject to the retention policy of
  the storing file server (per `content-addressed-knowledge`), so an answer's
  evidence outlives the process that produced it.
- Define redaction for durable evidence: secret material is redacted before
  durable persistence, and redacted spans are marked so an auditor can
  distinguish "redacted" from "missing".

## Capabilities

### New Capabilities

- `evidence-retention-and-projection`: Owns the oversized-output projection
  contract (preview + namespace-path reference + truncation metadata),
  post-exit evidence readability under retention, and redaction marking for
  durable evidence.

### Modified Capabilities

- `delegated-result-handoff`: `output_ref` becomes a namespace path readable in
  the parent's namespace; raw host rollout paths are demoted to optional debug
  metadata.

## Impact

- Affected runtime modules: tool-result persistence and tape projection in
  `crates/agent-engine` (turn executor, tool orchestrator), delegated result
  assembly in `virtual_tools.rs`, action record output files, agentfs retention
  behavior.
- Affected specs: consumes `agent-file-layout-contract` (append-only streams,
  `actions/<id>/` records) and `content-addressed-knowledge` (retention/GC
  ownership); neither is modified.
- Affected tests: projection of oversized tool and child outputs, parent reads
  of child output refs, post-exit readability, redaction marking.
