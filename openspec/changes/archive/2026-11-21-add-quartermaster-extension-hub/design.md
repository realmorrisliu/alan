# Design

## Context

Package Service currently materializes ordinary Skill packages from explicit
namespace snapshots with deterministic ids, atomic lifecycle and immutable Process
references (ADR-0030, ADR-0052). Embedded boot units and Tool registry entries do
not come from `q` installation, and provider protocols are compiled in.

## Goals / Non-Goals

- Goals: one hub for four kinds; installation without authority; the same
  explicit-source and atomic-lifecycle rules for every kind.
- Non-Goals: a remote registry, implicit Host-directory sources, automatic trust,
  background services, or moving credentials into packages.

## Decisions

A package manifest declares exactly one kind and its entry. The Package Service
validates the manifest, materializes content under its System Store subtree and
projects it into the Standard Namespace by explicit reference, as it does for
Skills. Process authority over a package is still the immutable reference passed at
spawn.

`tool` entries become executables in the alan9 command namespace and run as Tool
Processes under existing governance. `mcp` entries describe a server command and
its declared capabilities; the server runs as a governed Process inside the
caller's confined execution boundary and its tool calls pass through the same
action policy as native Tools. `provider` entries register a protocol adapter id
that a Connection profile may name; secrets stay with Host adapters and Connection
Service remains the metadata owner.

Installation is metadata and content only. It adds no mount, credential, network
right or approval. Uninstall and upgrade keep the existing atomic, exact semantics.
Unsupported or unsafe manifests fail closed and remain visible as compatibility
gaps.

## Risks / Trade-offs

- A malicious package → no authority at install; execution is confined and governed.
- Kind sprawl → exactly four kinds; additions need their own change.
- MCP servers expand the effect surface → each server runs inside the existing
  confinement and its calls are governed like native Tools.

## Migration Plan

Additive. Existing Skill packages keep their identity. First-party Skills remain
preinstalled. No kind other than `skill` is advertised until shipped.
