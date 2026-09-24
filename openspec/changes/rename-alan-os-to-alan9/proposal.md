# Proposal

## Why

Alan needs distinct names for the complete personal computing product and its
agent operating system. `alan9` gives the system an identity that acknowledges
its Plan 9 inspiration without implying a standalone bootable OS or 9P compatibility.

## What Changes

- Adopt `alan9` as the system name formerly written `Alan OS`; retain `Alan`
  as the product and `alan` as the user entry command.
- Use `alan9 Kernel` and `alan9 Host` for the existing system components.
  Retain Alan Shell, Agent Runtime Service and other role names.
- Record the accepted naming decision in ADR-0057, then align active explanatory
  prose and agent guidance through a scoped documentation implementation.
- Preserve executable/crate/type identifiers, protocol names, storage paths,
  spec IDs and historical records. This is a naming change, with no runtime migration.
- Keep unified TUI/Shell and model-assisted input routing under the existing
  interaction/cognition changes; this naming decision does not settle that design.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `product-brand-identity`: distinguish Alan's product brand from the lowercase
  alan9 system name and define the migration boundary for existing identifiers.

## Impact

ADR-0057 and this change define the decision. Follow-up implementation covers
README, AGENTS.md, current architecture/operator prose and active OpenSpec prose;
the existing brand validation must accommodate the system name. Older ADRs and
archived changes remain historical. No dependency, command, permission,
Process lifecycle, provider support or persisted format changes are included.
