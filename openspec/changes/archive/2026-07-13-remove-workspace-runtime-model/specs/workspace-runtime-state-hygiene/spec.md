## REMOVED Requirements

### Requirement: alan Home Workspace State
**Reason**: Alan home and workspace runtime state are removed.
**Migration**: Use channel System Store backing owned by services.

### Requirement: Canonical Workspace Identity
**Reason**: Alan OS has no workspace identity.
**Migration**: Use Process Reference and explicit Host Mount identity.

### Requirement: Authored workspace content remains shared by workspace semantics
**Reason**: Runtime no longer scans authored Host directories.
**Migration**: Explicitly import or descriptor-pass authored content.

### Requirement: Ignore rules cover channel-scoped generated state
**Reason**: Generated state no longer lives in Host project directories.
**Migration**: Service-owned System Store paths remain outside authored trees.

### Requirement: Generated Process and machine state is ignored and separated from authored roots
**Reason**: Process state is ephemeral and durable evidence belongs to service stores.
**Migration**: Remove recognized generated `.alan` state.

### Requirement: Generated runtime state is channel-scoped by its actual owner
**Reason**: The workspace path model is retired in favor of System Store owners.
**Migration**: Each service writes its channel System Store subtree.
