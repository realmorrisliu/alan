## Why

ADR-0024 D1 makes the LLM a typed stream a process consumes, and D6 makes the
provider file server the chokepoint where cost and access live. To reach the
north-star milestone (talking to an agent in Alan Shell), an agent process needs
to open an LLM stream through the namespace rather than calling a provider SDK.
This change defines `alan-llmfs`: the file server that exposes models as
Generations under `/mnt/llm` (handle posted at `/srv/llm`), wrapping the existing
`alan-llm` adapters.

## What Changes

- Add `alan-llmfs`, a file server (speaks aP, the `alan-ap` protocol) that posts
  a handle at `/srv/llm` and serves its tree at `/mnt/llm`, wrapping `alan-llm`.
- Separate Provider (a wire driver, introspect-only at `/mnt/llm/providers/<provider>`)
  from Connection (a callable endpoint binding Provider + Model + Credential at
  `/mnt/llm/connections/<connection>`). Generations happen on Connections.
- Model a Generation as a clone-via-open connection directory: the caller opens
  `clone`, writes one complete neutral request document to `data`, and reads a
  typed token stream from `events`; `ctl` aborts a running Generation; `status`
  exposes progress and cost.
- Use an independent, versioned wire DTO for the request document and the stream
  events; `alan-llmfs` maps the DTO to and from `alan-llm` internal types.
- Keep credentials in the Connection / host secret store — never in the request
  document or an agent's namespace as plaintext. Cost, metering, and
  rate-limiting live in `alan-llmfs`, not in a global quota service.

## Capabilities

### New Capabilities

- `llm-file-server`: `alan-llmfs` — the `/mnt/llm` file server (handle at `/srv/llm`) exposing Providers
  (introspect) and Connections (callable), Generations as clone-dir interactions,
  the versioned request/event wire DTO, and in-server metering.

### Modified Capabilities

- None.

## Impact

- Depends on `define-plan9-kernel-substrate` (the aP protocol, clone-via-open,
  retained streams) and `alan-llm` (provider adapters).
- Enables the north-star milestone: an agent opens a Connection, streams tokens,
  and the result reaches Alan Shell as files.
- ADRs: implements ADR-0024 D1 (LLM as a stream), D2 (provider-neutral request,
  wire format provider-local), and D6 (metering in the provider file server).
