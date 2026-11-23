## RENAMED Requirements
- FROM: `### Requirement: Quartermaster is one extension hub with four package kinds`
- TO: `### Requirement: Quartermaster is one extension hub with five package kinds`

## MODIFIED Requirements

### Requirement: Quartermaster is one extension hub with five package kinds
The Package Service SHALL install, catalog, upgrade and remove packages of exactly
five kinds, `skill`, `mcp`, `tool`, `provider` and `fileserver`, through one
atomic lifecycle with explicit sources and deterministic identities. A manifest
SHALL declare exactly one kind.

#### Scenario: A mixed set of packages is installed
- **WHEN** a user installs one package of each of the five kinds (`skill`, `mcp`, `tool`, `provider`, `fileserver`)
- **THEN** each has a deterministic identity and independent lifecycle
- **AND** a failed install of one leaves the others unchanged

#### Scenario: A manifest declares an unknown kind
- **WHEN** a manifest names a kind outside the five
- **THEN** installation fails closed and the gap remains visible

## ADDED Requirements

### Requirement: Fileserver packages run as confined external aP services
A `fileserver` package SHALL declare an aP-speaking service executable, wire
transport (`stdio` or `unix_socket`), and service identifier. Installation MUST
NOT start its process or grant mounts or credentials. When started under Service
Manager, it SHALL run inside OS confinement, speak aP wire framing at
`/srv/<id>`, and enforce explicit `ReadOnly` or `ReadWrite` mount access while
keeping credentials with Host adapters.

#### Scenario: Installing a fileserver package grants no runtime process or mount
- **WHEN** an operator installs a valid `fileserver` package through `q install`
- **THEN** Package Service catalogs the immutable package revision
- **AND** no service process is started, `/srv/<id>` is not yet published, and no Host credential or mount authority is added

#### Scenario: Explicit mount binds a confined fileserver handle with enforced access
- **WHEN** an operator or authorized workflow starts and mounts a `fileserver` package with `ReadOnly` access at `/mnt/<id>`
- **THEN** Service Manager runs the service inside OS confinement connected over aP wire transport and publishes `/srv/<id>`
- **AND** read/walk operations succeed while mutating writes through the `ReadOnly` mount fail closed

#### Scenario: Fileserver package requests ungranted Host network or file access
- **WHEN** a confined `fileserver` service attempts outbound access or Host reads outside its explicit launch grants
- **THEN** the OS confinement or Host adapter denies the operation without expanding authority
