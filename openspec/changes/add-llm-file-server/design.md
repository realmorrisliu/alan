## Context

`alan-llm` already provides a provider-neutral abstraction (`GenerationRequest`,
`StreamChunk`) over Anthropic, OpenAI Responses/Chat, Gemini, and OpenRouter.
`alan-llmfs` is a thin file server over it: it turns "call a model" into "open a
Generation, write a request, read a stream" so agents reach models through the
namespace (ADR-0024 D1/D2/D6). This is on the north-star critical path (M2).

## Goals / Non-Goals

**Goals:**

- Expose models through `/mnt/llm` (handle posted at `/srv/llm`) as aP file surfaces.
- Make a Generation an inspectable connection directory (progress and cost are
  `cat`-able).
- Keep the request provider-neutral and credentials out of it.

**Non-Goals:**

- Implement provider wire formats (owned by `alan-llm`).
- Implement the agent loop that assembles requests (that is `alan-agentfs` /
  the agent file-layout contract).
- Implement a wire transport for aP (in-process v1).

## Decisions

Implements [ADR-0024](../../../docs/adr/0024-plan9-kernel-model.md) D1/D2/D6 and
uses the aP protocol from `define-plan9-kernel-substrate`.

- **Provider vs Connection.** Provider = wire driver (introspect-only). Connection
  = Provider + Model + Credential, the callable endpoint. Agents bind a
  Connection; changing model = binding a different Connection.
- **Generation = clone-dir.** `open clone` allocates `/mnt/llm/<connection>/<n>/`
  with `data` (write request), `events` (read typed stream), `ctl` (abort),
  `status` (progress/cost). Concurrency is isolated per connection directory.
- **Implicit commit.** Writing one complete request document to `data` commits
  the Generation; there is no `start` command. `events` is retained from offset 0,
  so reading it after generation begins loses nothing. `ctl` only aborts.
- **Independent wire DTO.** A versioned request DTO and stream-event DTO decouple
  the wire format from `alan-llm` internals; `alan-llmfs` maps DTO ↔ `alan-llm`.
- **Two-fold errors.** Dial-time (no access / rate limited / model unknown) →
  `open` error code. Mid-generation (provider error) → terminal error record in
  `events`.

## Risks / Trade-offs

- **R1 (ADR-0024): in v1 everything is in-process**, so "credentials never reach
  the agent" is enforced by the server boundary, not by isolation; hardening
  awaits the cross-process transport slice.
- **DTO mapping cost.** The independent DTO adds a mapping layer over `alan-llm`;
  accepted to keep the wire format stable and provider-neutral.
- **Connection directory lifecycle.** Finished Generations are retained briefly
  for `cat status` post-mortem, then reaped per server policy.

## Migration Plan

1. Land the aP protocol (`define-plan9-kernel-substrate §5`).
2. Implement `alan-llmfs` over `alan-llm` with one working Connection (M2).
3. `alan-agentfs` opens Connections to drive the agent loop.
