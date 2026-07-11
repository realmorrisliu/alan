## Why

PDF remains the best distribution format for high-quality books, research
papers, technical manuals, mathematical documents, source-code-heavy material,
and visually designed publications. Its fixed layout also makes it weak on
modern multi-device reading surfaces. Runtime reflow usually gives up too much
typographic control for these document classes.

UPDF takes a different route: generate several fully typeset PDF targets from a
single manuscript and package them into one reader artifact. A phone target,
tablet target, desktop target, and print target can each be optimized ahead of
time while preserving PDF quality.

The product should serve two users:

- Authors need a calm writing environment that keeps them in prose and out of
  layout machinery while drafting.
- Readers need a high-quality package that opens on the right target layout
  without runtime reflow.
- Independent authors need to sell and distribute from their own sites without
  becoming dependent on Kindle-style platform lock-in.

The bridge is an automated publishing workflow. Authors write Markdown
manuscripts. UPDF uses Typst as the publishing backend. Agents handle most
layout iteration, QA triage, and target-specific production work. Humans review
the generated outputs, comments, diffs, and final package.

For independent distribution, UPDF should prefer reader-owned files over
traditional DRM. The first rights model should support signed DRM-free packages
and optional personalized watermarking. Blockchain, content-addressing, or
verifiable credentials may provide optional proof, provenance, and portable
license records, but they should not be mandatory for ordinary readers.

## What Changes

- Define the umbrella UPDF product model: Markdown-first authoring,
  Typst-backed publishing, multi-target PDF packaging, and agent-assisted
  review.
- Establish `.updf` as a source-free reader package and keep authoring projects
  as a separate trust boundary.
- Define independent-author distribution with signed DRM-free packages,
  optional personalized watermarking, and optional blockchain-backed proof or
  license records.
- Define the standalone `updf` harness direction inside the Alan workspace with
  an independent binary rather than an `alan` subcommand.
- Define Alan for macOS as a package preview and review consumer rather than the
  first authoring editor.
- Capture how future authoring should work: writing flow in Markdown, publishing
  flow through Typst profiles/templates, review flow through preview comments,
  QA issues, bounded Agent Process patches, visual diffs, and human approval.
- Define agent-assisted review as opening project/package/QA descriptors,
  binding the `updf` executable as a Tool, and spawning a role-specific Agent
  Executable through the normal Process and namespace boundary.
- Record additional publishing-industry concerns for later follow-up specs,
  including accessibility, metadata, validation, editioning, samples, sales
  operations, reader data portability, print/POD, citations, indexing, and
  archival provenance.
- Decompose the umbrella into follow-up implementation slices instead of trying
  to implement the full authoring, publishing, and reader product in one change.

## Capabilities

### New Capabilities

- `updf-authoring-workflow`: Defines the Markdown manuscript model,
  source/publishing/review boundaries, preview-comment-agent loop, and future
  WYSIWYG-like semantic editing direction.
- `updf-rights-and-distribution`: Defines DRM-free reader ownership,
  author-owned distribution, signed packages, personalized watermarking, and
  optional blockchain/provenance/license records.
- `updf-harness-contract`: Defines the standalone `updf` binary, project
  layout, Markdown/Typst publishing pipeline direction, Typst target
  compilation, QA report contract, package format, inspection behavior, and
  agent-facing mutation-lane guidance.
- `macos-updf-preview`: Defines Alan for macOS `.updf` opening, manifest
  parsing, PDF target rendering, target switching, QA summary display, and
  read-only package handling.

### Modified Capabilities

- `macos-shell-workspace-interactions`: Alan shell content panes need to support
  a non-terminal document preview content kind for `.updf` packages.
- `macos-shell-build-test-contract`: Apple client verification must cover `.updf`
  package parsing and preview-routing model behavior.

### Dependencies

- `alan-app-service-integration`: UPDF consumes its descriptor-passing, Agent
  Executable spawn, direct file-client, and missing-native-boundary entry
  criteria. UPDF remains a file/package workflow and does not require a
  long-running app service in V0.

## Impact

- This umbrella change captures product direction and capability boundaries.
- Alan for macOS preview remains parked until the host can consume the read-only
  package file contract directly; package preview does not introduce another
  client-facing authority.
- Follow-up implementation changes should separately cover:
  - the first `crates/updf` harness and standalone binary;
  - Alan for macOS read-only `.updf` package preview;
  - Markdown authoring project support;
  - preview comments and agent-assisted publishing review;
  - signed package and personalized watermark distribution support;
  - publishing infrastructure topics such as accessibility, metadata,
    validation, editions, samples, sales operations, reader notes portability,
    print/POD, citations, indexing, and archival provenance;
  - later WYSIWYG-like semantic editing surfaces.
- No first implementation slice should add full WYSIWYG PDF editing, runtime PDF
  reflow, source-in-reader packaging by default, cloud build, traditional DRM,
  mandatory wallet/blockchain flows, or collaboration.
