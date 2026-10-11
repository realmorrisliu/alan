# Present terminal actions without role labels

Status: accepted design, 2026-10-09; implemented in PR #1044, merged 2026-10-10.

Alan Shell presents concrete actions and authoritative outcomes without
renderer-generated `xxx>` role labels. Routine successful read-only work may
share a compact presentation, while individual Action identity, raw evidence,
errors and requests remain distinguishable. Plan changes retain brief history
records and full snapshots. Literal content and input route markers are preserved.

The trade-off is less default visual detail in exchange for readable execution
history with deliberate disclosure. Presentation is a projection of existing
AgentFS and Process-owned truth, not a new event log, execution owner or inference
layer. The Runtime owns semantic metadata; the renderer does not infer actions
from text. Rewriting raw content or discarding historical identities would make
copying, recovery and diagnosis unreliable, so those alternatives were rejected.

The user confirmed this direction through the
[design interview](../../openspec/changes/archive/2026-10-10-refine-terminal-output-presentation/decision-record.md).
The delivered proposal and acceptance are recorded in the
[OpenSpec change](../../openspec/changes/archive/2026-10-10-refine-terminal-output-presentation/proposal.md),
which refines ADR-0058's terminal presentation without changing its input routes
or lifecycle ownership. Linux tooling and broader development qualification are
separate deliveries.

Delivery checks and canonical synchronization are recorded in the archived
[delivery receipt](../../openspec/changes/archive/2026-10-10-refine-terminal-output-presentation/delivery.md).
Current normative behavior belongs to
[terminal presentation](../../openspec/specs/rust-inline-tui/spec.md),
[Tool results](../../openspec/specs/tool-result-presentation/spec.md) and
[renderer attachment](../../openspec/specs/alan-renderer-host-contract/spec.md).
