# Root Agent Is A Process-Tree Role

Status: Accepted. Refined by ADR-0024.

Alan OS has an always-available Root Agent Process for system continuity and
coordination. It is an ordinary Agent Process at the root of the agent process
tree, surfaced through `/agent/root`.

The Root Agent Process does not receive special Kernel typing or root
permission. Work remains bounded in child Agent Processes with explicit
descriptors, credentials, policy, rollout evidence, and Memory Store access.
This preserves a resident system intelligence without creating an unbounded
global conversation or authority center.
