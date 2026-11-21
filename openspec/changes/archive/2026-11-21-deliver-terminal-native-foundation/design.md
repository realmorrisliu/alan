# Design

## Context

ADR-0061 records four decisions; this change delivers the parts that need code and
canonical contract updates. The engine currently reads the whole generation
response before publishing, forces server continuation and provider compaction off,
and the renderer drops thinking presentation. Provider adapters lack stable cache
keys and several fidelity gaps. Terminal tests relied on a host-specific harness.

## Goals / Non-Goals

- Goals: incremental, truthful output; observable usage; four correct providers;
  a calm terminal that never freezes or leaks keys; PTY-based acceptance.
- Non-Goals: trust-posture changes, `q` package kinds, delegation, continuity,
  automatic input routing, desktop topology, terminal-host detection or integration.

## Decisions

Streaming stays a file operation. The engine reads generation output as it arrives
and appends output and thinking records to the existing AgentFS streams; tape
reconciliation remains the single source of truth for the final message, and
response guardrails apply to the reconciled result without stalling live display.
Waiting on an empty generation read uses a notification or tail wake, never a
yield loop.

Usage is a derived UI snapshot beside the existing activity, plan and notice files.
It carries no authority and is rebuilt from engine accounting. Optional cost is
shown only with provider-supplied pricing provenance.

An ephemeral invocation is a boot option on the Host composition. It rejects
combination with explicit recovery and never reads or writes the root rollout
pointer; durable stores keep their existing owners.

First-party providers are a product-level list owned by Connection Service
descriptors, not a kernel concept. Each provider keeps its wire adapter in
`crates/llm`; stable per-session cache keys replace random per-request values;
credentials remain with Host adapters. Anthropic keeps its wire adapter, gains
prompt caching, and is excluded from new-profile defaults and onboarding.

Terminal verification drives the built binary through `openpty` and a `vt100`
screen model. Ratatui `TestBackend` checks remain for layout. Ctrl+Q inside any
modal, chooser, form or project selection exits that surface and never the input
reader; idle Esc dismisses completion or does nothing and never interrupts.

## Risks / Trade-offs

- Partial output then guardrail failure → publish only reconciled final state as
  authoritative and mark incomplete streams.
- Provider fidelity drift → wire-mock tests per adapter; live harness stays opt-in.
- Dropping Anthropic defaults surprises users → existing profiles are untouched and
  the protocol remains available through `q` in M3.
- Concurrent edits to large files → split into submodules to keep every Rust file
  under the repository size gate.

## Migration Plan

Additive for runtime surfaces. Provider defaults change only for new profiles. If a
provider adapter regresses, the profile fails closed with a clear Connection error
rather than falling back to another provider.
