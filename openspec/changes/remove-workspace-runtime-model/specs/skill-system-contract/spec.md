## ADDED Requirements

### Requirement: Skills enter through installed packages or descriptors
Alan SHALL resolve Skills only from installed Alan OS packages and explicit
Skill/Agent Definition descriptors. It MUST NOT scan AgentRoot, workspace,
`.agents`, Alan home, or other Host directories as implicit providers.

#### Scenario: Host directory contains a Skill
- **WHEN** a mounted Host directory contains `SKILL.md`
- **THEN** the Skill remains ordinary file content until explicitly imported or
  passed by descriptor

## REMOVED Requirements

### Requirement: Global public skill sources are channel-scoped
**Reason**: Host-directory public Skill sources are no longer implicit providers.
**Migration**: Explicitly import the source into the channel Package Service.

### Requirement: Workspace skill sources remain workspace-authored
**Reason**: Workspace identity and discovery are removed.
**Migration**: Mount authored content explicitly and install or pass it by descriptor.
