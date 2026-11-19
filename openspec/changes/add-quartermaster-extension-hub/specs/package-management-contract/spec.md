## ADDED Requirements

### Requirement: Quartermaster is one extension hub with four package kinds
The Package Service SHALL install, catalog, upgrade and remove packages of exactly
four kinds, `skill`, `mcp`, `tool` and `provider`, through one atomic lifecycle with
explicit sources and deterministic identities. A manifest SHALL declare exactly one
kind.

#### Scenario: A mixed set of packages is installed
- **WHEN** a user installs one package of each kind
- **THEN** each has a deterministic identity and independent lifecycle
- **AND** a failed install of one leaves the others unchanged

#### Scenario: A manifest declares an unknown kind
- **WHEN** a manifest names a kind outside the four
- **THEN** installation fails closed and the gap remains visible

### Requirement: Installation grants no authority
Installing a package SHALL NOT add a mount, credential, network right, approval or
standing delegation. Any effect a package causes SHALL pass through Agent Runtime
governance and existing grants.

#### Scenario: A tool package is installed
- **WHEN** a `tool` package is installed
- **THEN** its executable appears in the command namespace by explicit reference
- **AND** running it is governed like any Tool Process

### Requirement: MCP packages run as governed Processes
An `mcp` package SHALL start its server as an ordinary Process inside the caller's
execution boundary, and its tool calls SHALL pass through the same action policy as
native Tools.

#### Scenario: An MCP server proposes an out-of-grant write
- **WHEN** an MCP tool call would write outside the granted mounts
- **THEN** it is denied or escalated exactly as a native Tool call would be

### Requirement: Provider packages register protocol adapters only
A `provider` package SHALL register a protocol adapter that a Connection profile may
name. Credentials SHALL remain with Host adapters and Connection Service SHALL remain
the profile metadata owner.

#### Scenario: A provider package is used by a profile
- **WHEN** a profile selects an installed provider protocol
- **THEN** the credential reference resolves through the Host adapter
- **AND** the package holds no secret
