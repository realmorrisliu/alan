## REMOVED Requirements

### Requirement: Skill vocabulary is stable
**Reason**: The vocabulary contract names daemon response and catalog consumers.
**Migration**: Use package, registry, resolver, prompt, direct CLI, and namespace projection owners.

### Requirement: Management surfaces expose skill-level state
**Reason**: The management contract requires daemon skill APIs and daemon override writes.
**Migration**: Retain direct CLI management and package/catalog state; future file-server management needs a separate accepted contract.

### Requirement: Validation covers resolution, prompts, availability, and docs
**Reason**: The validation matrix includes daemon API/catalog behavior as a required surface.
**Migration**: Validate package resolution, prompt injection, availability, delegation, CLI, namespace projection, and docs.

## ADDED Requirements

### Requirement: Skill vocabulary is owned by packages and resolution
Alan SHALL use stable Skill, Skill Package, Registry, Resolver, Active Skill, Exposure Mode,
Invocation Mode, and Resource vocabulary across packages, prompt assembly, direct CLI, namespace
projection, documentation, and authoring tools.

#### Scenario: A Skill is described on a current surface
- **WHEN** a package, prompt, CLI response, namespace projection, or document describes a Skill
- **THEN** it uses the canonical skill-level vocabulary
- **AND** it does not depend on a background catalog API

### Requirement: Skill management remains local-first
Alan SHALL expose Skill discovery, validation, enablement, override, authoring, and evaluation
through direct CLI and package operations. Catalog snapshots MAY be derived locally but SHALL NOT
be a second authority or require a persistent product server.

#### Scenario: Operator inspects the Skill catalog
- **WHEN** an operator runs the direct Skill list or package command
- **THEN** Alan resolves installed packages and effective skill-level state locally
- **AND** the result comes from the package, registry, resolver, and override owners

### Requirement: Skill validation covers durable owners
Skill validation SHALL cover package structure, registry resolution, prompt rendering,
availability, delegation, CLI behavior, namespace projection, authoring, and current docs.

#### Scenario: A removed management surface returns
- **WHEN** a Skill test or current document adds a background API as a required management owner
- **THEN** validation rejects the change
