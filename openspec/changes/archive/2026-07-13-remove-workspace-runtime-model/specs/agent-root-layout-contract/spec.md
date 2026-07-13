## ADDED Requirements

### Requirement: Agent Definition layout is file-tree local
Alan SHALL interpret persona, Skills, policy, and model selection relative to
the explicitly supplied Agent Definition tree. Production code MUST NOT derive
global, workspace, default, or named Host-directory root chains.

#### Scenario: Definition descriptor is opened
- **WHEN** an Agent Process receives an Agent Definition descriptor
- **THEN** its assets resolve within that tree
- **AND** no other definition tree is overlaid implicitly

## REMOVED Requirements

### Requirement: Runtime-owned agent-root layout
**Reason**: Global/workspace default and named Host-directory roots are removed.
**Migration**: Use the descriptor-local Agent Definition layout.

### Requirement: Raw layout-string guardrail
**Reason**: The retired Host-directory layout is no longer canonical.
**Migration**: Guard against reintroduction of legacy roots and require explicit definitions.

### Requirement: Agent-name semantics are shared by direct launch callers
**Reason**: Host and CLI callers no longer select named Agent overlays.
**Migration**: Pass an Agent Definition descriptor when spawning an Agent Process.

### Requirement: Direct readers and writers share the runtime layout owner
**Reason**: Direct Host readers/writers and `alan init` are removed.
**Migration**: Author or install Agent Definitions through Alan OS file/package operations.
