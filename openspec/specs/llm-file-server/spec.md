# llm-file-server Specification

## Purpose
Defines `alan-llmfs` provider, Connection, and Generation file trees, including
clone-via-open lifecycle, clunk-committed current-version requests, rejection of
unversioned or unsupported request documents, typed event streams, metering,
and phase-specific errors.
## Requirements
### Requirement: llmfs posts a handle in `/srv` and serves its tree under `/mnt/llm`
Alan OS SHALL provide `alan-llmfs`, a file server that speaks aP (the `alan-ap`
protocol). It SHALL post a single mountable handle under `/srv` (`/srv/llm`) —
`/srv` holds rendezvous handles only, never service state — and its file tree
(providers, connections, generations) SHALL be mounted at a real namespace
location, canonically `/mnt/llm`, with specific Connections bound into an agent's
own namespace. It SHALL wrap the existing `alan-llm` adapters and SHALL NOT
require an agent to call a provider SDK directly. An agent SHALL gain model access
only by having an `llmfs` Connection bound into its namespace.

#### Scenario: The handle and the tree are separate
- **WHEN** `alan-llmfs` starts
- **THEN** it posts a mountable handle at `/srv/llm` and serves its tree at
  `/mnt/llm` (or wherever a client mounts the handle)
- **AND** `/srv/llm` is a handle, not a directory of provider/connection/
  generation state

#### Scenario: An agent reaches a model
- **WHEN** an agent needs to call a model
- **THEN** it opens a Generation under a Connection bound into its namespace
- **AND** it never imports or calls a provider SDK

#### Scenario: A model is withheld
- **WHEN** a sub-agent must be denied model access
- **THEN** no `llmfs` Connection is bound into its namespace
- **AND** the denial needs no global policy check

### Requirement: Provider and Connection are distinct
`alan-llmfs` SHALL expose Providers and Connections as distinct surfaces. A
Provider SHALL be a wire driver served introspect-only at `/mnt/llm/providers/<provider>`
(its available models, capabilities, status) and SHALL NOT be callable on its
own. A Connection SHALL bind a Provider, a Model, and a Credential into a callable
endpoint at `/mnt/llm/connections/<connection>`, where Generations happen. Changing the model
SHALL mean binding a different Connection.

#### Scenario: A provider is inspected
- **WHEN** a caller reads `/mnt/llm/providers/<provider>`
- **THEN** it sees the driver's models, capabilities, and status
- **AND** it cannot start a Generation there (no Model or Credential)

#### Scenario: Connections track configuration
- **WHEN** a user adds or removes a connection profile (provider + model +
  credential)
- **THEN** a corresponding `/mnt/llm/connections/<connection>` endpoint appears or disappears
- **AND** the credential is referenced by the Connection, not copied into it as
  agent-visible plaintext

### Requirement: A Generation is a clone-via-open connection directory
`alan-llmfs` SHALL model each Generation as a connection directory allocated by
opening a `clone` file under a Connection. The directory SHALL contain `data`
(write the request), `events` (read the typed token stream), `ctl` (abort), and
`status` (progress and cost). Concurrent Generations SHALL be isolated by
directory.

#### Scenario: A Generation is started
- **WHEN** a caller opens `/mnt/llm/connections/<connection>/clone`
- **THEN** `alan-llmfs` allocates `/mnt/llm/connections/<connection>/<n>/` with `data`,
  `events`, `ctl`, and `status`
- **AND** two concurrent callers receive independent directories

#### Scenario: A Generation is observed
- **WHEN** any permitted process reads `/mnt/llm/connections/<connection>/<n>/status`
- **THEN** it sees progress, token counts, and accumulated cost
- **AND** the Generation is inspectable as files, not hidden in a caller's fd

### Requirement: The request is framed; commit is on clunk
`alan-llmfs` SHALL treat a Generation request as one provider-neutral document
that MAY be written to `data` across multiple aP writes (model requests can exceed
one transport write). The Generation SHALL commit — and only then start — when the
caller clunks (closes) the `data` fid; a partial or multi-chunk write SHALL NOT
start generation. A malformed or truncated document SHALL be rejected at commit,
not run with partial context. `events` SHALL be retained from offset 0 so reading
it after generation begins loses no records. `ctl` SHALL be used only to abort a
running Generation. The request document SHALL NOT carry credentials.

#### Scenario: A large request is submitted in chunks
- **WHEN** a caller writes a request document to `data` over several writes and
  then clunks the `data` fid
- **THEN** the Generation commits and starts only at clunk, with the full document
- **AND** no generation ran on a partial write

#### Scenario: A truncated request is rejected
- **WHEN** the document committed at clunk is malformed or incomplete
- **THEN** the commit fails: the `data` clunk/write returns an error and `status`
  shows rejected, and no generation starts (this is a commit-time error, distinct
  from a dial-time `open` error and from a mid-generation terminal record)
- **AND** the model is never run with truncated context

#### Scenario: A Generation is aborted
- **WHEN** a caller writes an abort command to `ctl`
- **THEN** the running Generation stops
- **AND** the abort is not expressed as a separate file or side API

### Requirement: The request and events use an independent versioned wire DTO
`alan-llmfs` SHALL define its own versioned wire DTO for the request document and
for the stream-event records, decoupled from `alan-llm` internal types. It SHALL
accept only the current explicitly versioned request document and SHALL reject
unversioned or unknown-version request shapes without migration or fallback. It
SHALL map the DTO to and from `alan-llm` (`GenerationRequest` / `StreamChunk`)
internally. The events stream SHALL be a byte-stream record convention (for
example one JSON record per line) per the aP stream model.

#### Scenario: The wire DTO is versioned
- **WHEN** the request or event wire format is defined
- **THEN** it is a versioned DTO owned by `alan-llmfs`, not a re-export of
  `alan-llm` internal structs
- **AND** an `alan-llm` internal refactor does not change the wire format unless
  the DTO version changes

#### Scenario: Current request document is committed
- **WHEN** a caller commits a valid current-version request document to a
  Generation
- **THEN** `alan-llmfs` maps the full document to `GenerationRequest`
- **AND** generation starts only after normal commit validation succeeds

#### Scenario: Unversioned request document is committed
- **WHEN** a caller commits the retired unversioned `{system,user}` request
  shape or a document without the required version discriminator
- **THEN** the Generation commit is rejected before provider dispatch
- **AND** no legacy DTO fallback or migration is attempted

#### Scenario: Unknown request version is committed
- **WHEN** a caller commits a request document with an unsupported version
- **THEN** the Generation commit is rejected with an unsupported-version error
- **AND** no provider request starts

### Requirement: Metering lives in llmfs; errors split by phase
`alan-llmfs` SHALL enforce cost, metering, and rate-limiting itself, reached only
through a bound Connection, with no global model-quota service. Errors SHALL split
by phase: a dial-time failure (no access, rate limited, unknown model) SHALL
return an `open` error code; a commit-time failure (a malformed/truncated request
document at `data` clunk) SHALL fail the clunk and set `status` rejected with no
generation started; and a mid-generation failure SHALL surface as a terminal error
record in `events`.

#### Scenario: A Connection is over budget
- **WHEN** a Connection has exhausted its configured budget
- **THEN** opening a new Generation returns a dial-time error code
- **AND** the limit is enforced by `alan-llmfs`, not by an ambient global service

#### Scenario: The provider fails mid-stream
- **WHEN** the upstream provider errors after streaming has begun
- **THEN** a terminal error record appears in `events`
- **AND** the reader observes it by reading the stream, not through a side channel
