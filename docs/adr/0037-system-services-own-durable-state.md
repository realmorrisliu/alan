# System services own durable state

Status: accepted; storage identity refined 2026-10-14 after merged PR #1042.
The [current System Store contract](../../openspec/specs/alan-os-system-store/spec.md)
uses an explicitly bound product store rather than install channels.

Alan OS Host provides an explicitly bound product System Store backing root, while each
File-Server Service owns the format and lifecycle of its packages, rollouts,
Memory Stores, Agent Definitions, or service metadata. Alan OS does not write
runtime state into host-directory `.alan` trees, treat `~/.alan` as system
identity, expose the raw backing path to Agent Processes, or consolidate state
into a global document. Live Process and descriptor state remains ephemeral;
durable evidence and service data survive through their owning stores, while
credential secrets remain in their platform credential store.
