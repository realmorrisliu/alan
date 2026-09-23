## 1. Umbrella Product Design

> PARKED (2026-09-19): read this change's disposition.md before use. Retained
> draft text below is not implementation authorization; superseded desktop and
> renderer-launch assumptions must be replaced before reactivation.

- [ ] 1.1 Capture UPDF as Markdown-first for authoring, Typst-backed for
  publishing, and agent-assisted for layout review.
- [ ] 1.2 Define the writing, publishing, review, and reader package boundaries.
- [ ] 1.3 Define independent-author distribution with reader-owned files,
  signed DRM-free packages, personalized watermarking, and optional
  proof/license records.
- [ ] 1.4 Record additional publishing concerns for future specs, including
  accessibility, metadata, validation, editioning, samples, sales operations,
  reader data portability, print/POD, citations, indexing, and archival
  provenance.
- [ ] 1.5 Confirm the umbrella excludes direct PDF WYSIWYG editing, runtime
  reflow, source-in-reader packages by default, cloud build, traditional DRM,
  mandatory blockchain flows, and collaboration.

## 2. Capability Contracts

- [ ] 2.1 Define `updf-authoring-workflow` for Markdown manuscript projects,
  publishing-layer boundaries, preview comments, agent patch review, and
  WYSIWYG-like semantic editing direction.
- [ ] 2.2 Define `updf-rights-and-distribution` for signed DRM-free packages,
  personalized watermarking, author-owned distribution, and optional
  blockchain/provenance/license records.
- [ ] 2.3 Define `updf-harness-contract` for the standalone `updf` binary,
  package format, QA report contract, JSON outputs, and mutation-lane guidance.
- [ ] 2.4 Cancelled: define the desktop-only `.updf` preview; retain unchecked
  as history because Alan for macOS is retired (see disposition.md).
- [ ] 2.5 Define agent review through bounded descriptors, `/bin/updf`, a role
  Skill, Agent Executable spawn, and a writable proposal tree.

## 3. Implementation Slice Decomposition

- [ ] 3.1 Identify the first follow-up implementation slice for `crates/updf`,
  standalone binary, Typst-first build, QA, package, and inspect.
- [ ] 3.2 Cancelled: identify a desktop `.updf` preview slice; any future
  preview requires a selected, supported host consumer.
- [ ] 3.3 Identify the Markdown manuscript authoring project follow-up slice.
- [ ] 3.4 Identify the preview-comment-agent review loop follow-up slice.
- [ ] 3.5 Identify the signed package and personalized watermark distribution
  follow-up slice.
- [ ] 3.6 Leave additional publishing-industry concerns as a backlog for later
  focused specs rather than first-slice implementation requirements.
- [ ] 3.7 Defer WYSIWYG-like semantic editing to a later slice after comments,
  anchors, and agent patch review are proven.
- [ ] 3.8 Cancelled: keep the retired desktop preview slice parked; this task
  remains unchecked as history (see disposition.md).

## 4. Verification

- [ ] 4.1 Run `openspec validate define-updf-product-umbrella --strict`.
- [ ] 4.2 Review the umbrella for placeholders, contradictions, implementation
  leakage, and unclear boundaries.
