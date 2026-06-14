## 1. Umbrella Product Design

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
- [ ] 2.4 Define `macos-updf-preview` for Alan's read-only `.updf` preview and
  QA summary consumer role.

## 3. Implementation Slice Decomposition

- [ ] 3.1 Identify the first follow-up implementation slice for `crates/updf`,
  standalone binary, Typst-first build, QA, package, and inspect.
- [ ] 3.2 Identify the Alan macOS read-only `.updf` preview follow-up slice.
- [ ] 3.3 Identify the Markdown manuscript authoring project follow-up slice.
- [ ] 3.4 Identify the preview-comment-agent review loop follow-up slice.
- [ ] 3.5 Identify the signed package and personalized watermark distribution
  follow-up slice.
- [ ] 3.6 Leave additional publishing-industry concerns as a backlog for later
  focused specs rather than first-slice implementation requirements.
- [ ] 3.7 Defer WYSIWYG-like semantic editing to a later slice after comments,
  anchors, and agent patch review are proven.

## 4. Verification

- [ ] 4.1 Run `openspec validate define-updf-product-umbrella --strict`.
- [ ] 4.2 Review the umbrella for placeholders, contradictions, implementation
  leakage, and unclear boundaries.
