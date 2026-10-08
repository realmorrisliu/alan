# Read-only evaluation projection entry — 2026-10-07

This implements only the AgentFS storage/publication boundary in
[machine-shadow-contract.md](machine-shadow-contract.md). `machine/evaluation`
starts with a version-1 envelope and null observation. The owning runtime can
publish a bounded object through `AgentFs::publish_evaluation_observation`;
external aP clients cannot open it for writing or write directly. The method
stores already-acknowledged Machine evidence, not a second durable history.
Semantic validation and the durability barrier remain the Machine producer's
responsibility. This slice introduces no evaluator dispatch or runtime caller.

Publication uses the existing 1 MiB document bound, finite-read snapshot behavior,
qid version table and aggregate watch stream (`evaluation` record). Identical
publication does not emit a duplicate update. Invalid or oversized publication
preserves the last snapshot. Conformance now checks that the file is present.

`cargo test -p alan-agentfs` passes, including the public-boundary regression in
`tests/evaluation_projection.rs`: initial unknown state, Write/ReadWrite/direct
write rejection, stable chunked reads across publication, version/watch update
and deduplication, and rejected-publication preservation. These are AgentFS tests,
not evidence of Machine consumption, durable recovery or real routing accuracy.
The runtime publisher, Machine consumer and recovery checks remain unfinished;
no further implementation task is checked off by this entry.

Independent Spec and Standards review passed against base `37998cb1`.
