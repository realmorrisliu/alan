# ap-wire-transport Specification

## Purpose
TBD - created by archiving change add-ap-wire-transport. Update Purpose after archive.
## Requirements
### Requirement: aP operations are framed over bytes

Alan SHALL provide a byte transport representation for every existing aP
`Request` and for every successful or failed aP response.

#### Scenario: Request survives byte framing

- **WHEN** a client encodes any supported aP request as a wire frame
- **THEN** the receiver decodes the same request value without in-process state
  or borrowed data

#### Scenario: Response result survives byte framing

- **WHEN** a server encodes either a successful aP response or a typed
  `ErrorCode` as a wire frame
- **THEN** the receiver decodes the same `Result<Response, ErrorCode>` value

### Requirement: Any file server can be exported

Alan SHALL provide a server-side transport loop that accepts framed aP requests,
dispatches them to an `alan_ap::FileServer`, and writes framed response results.

#### Scenario: Exported server dispatches normal operations

- **WHEN** a remote client sends framed walk/open/read/write/stat/create/remove
  or clunk requests
- **THEN** the export loop invokes the same `FileServer` methods as the
  in-process transport

#### Scenario: Exported server preserves typed failures

- **WHEN** the exported file server returns an aP `ErrorCode`
- **THEN** the transport sends that typed failure instead of collapsing it into
  an unstructured IO failure

### Requirement: Remote trees import as file servers

Alan SHALL provide a client-side adapter that implements `alan_ap::FileServer`
while forwarding operations over the wire transport to an exported remote tree.

#### Scenario: Imported tree is used like a local tree

- **WHEN** a local client uses the imported adapter through walk/open/read/write,
  stat, create, remove, or clunk
- **THEN** the client observes normal aP results and does not distinguish the
  imported tree from an in-process file server

#### Scenario: Blocking stream reads preserve aP semantics

- **WHEN** a local client reads from an imported stream before the remote stream
  has data at the requested offset
- **THEN** the read remains pending until the remote file server produces data

### Requirement: Kernel remains transport agnostic

Alan SHALL keep aP wire import/export above the kernel boundary; `alan-kernel`
SHALL NOT depend on transport-specific crates or call transport-specific APIs.

#### Scenario: Kernel dependency boundary is preserved

- **WHEN** the aP wire transport is added
- **THEN** `alan-kernel` still depends only on `alan-ap` among Alan crates
- **AND** remote import/export is represented to the kernel as ordinary mounted
  file-server handles

