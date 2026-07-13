## MODIFIED Requirements

### Requirement: Host mount declarations project into SandboxSpec
Alan OS SHALL derive native subprocess sandbox roots from the exact Host Mount
grants present in the Process launch context. Writable grants SHALL contribute
writable Host roots; read-only grants SHALL contribute no write authority; and
virtual mounts SHALL NOT grant Host access. There is no implicit workspace seed.

#### Scenario: Writable Host Mount becomes sandbox writable root
- **WHEN** a Process receives one writable Host Mount at `/mnt/source`
- **THEN** the derived sandbox includes only that grant's Host backing as a
  writable root

#### Scenario: No Host Mount is present
- **WHEN** a Process has only virtual Alan OS mounts
- **THEN** no workspace, cwd, home, or other implicit Host writable root is added

## ADDED Requirements

### Requirement: Host files are invisible by default
A Host directory SHALL enter a Process namespace only after explicit Host
authorization creates a Host Mount. Kernel and Agent runtime files MUST use its
Alan OS mount path and MUST NOT expose the raw Host path.

#### Scenario: Alan starts inside a Host directory
- **WHEN** the CLI is launched with that directory as Host cwd
- **THEN** the directory remains absent until the Host explicitly grants and
  mounts it
