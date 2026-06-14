## Context

UPDF is a publishing product for generating multiple pre-rendered PDF layouts
from one manuscript and distributing them as a single package. The package is
conceptually similar to a universal app or fat binary: one distributable
artifact contains several optimized target PDFs.

The product should not force authors to think in page-layout code while they
are writing. Writing and publishing are different cognitive modes:

```text
Writing mode
  author drafts Markdown manuscript

Publishing mode
  UPDF and agents turn manuscript into target-specific Typst/PDF outputs

Review mode
  author reviews previews, QA issues, comments, diffs, and final package
```

Typst remains the publishing backend because it is fast, programmable, and
agent-editable. Markdown becomes the preferred authoring surface because it
keeps authors in prose. Alan for macOS becomes the preview/review consumer
before it becomes a full authoring environment.

The first commercial posture is independent-author distribution. UPDF should
help authors sell from their own websites and deliver reader-owned files instead
of forcing distribution through Kindle-style platform lock-in. Rights management
starts with signed DRM-free packages and optional personalized watermarking.
Blockchain can support optional proof, provenance, and portable license records,
but it is not the content-protection mechanism and must not be required for
ordinary reading.

The first implementation should prove a thin end-to-end path, while this
umbrella design keeps the full product direction visible:

```text
Markdown manuscript
    ↓
Publishing layer
    ↓
Typst targets
    ↓
PDF targets + QA
    ↓
.updf reader package
    ↓
Alan macOS preview/review
```

## Goals / Non-Goals

**Goals:**

- Define UPDF as Markdown-first for writing, Typst-backed for publishing, and
  agent-assisted for layout review.
- Keep authoring projects, publishing intermediates, and reader packages as
  separate trust and workflow boundaries.
- Define the first standalone `updf` harness direction in the Alan Cargo
  workspace without making it an `alan` subcommand.
- Define a source-free `.updf` reader package with multiple pre-rendered PDF
  targets.
- Define independent distribution for authors through signed DRM-free packages,
  personalized watermarks, and optional proof/license records.
- Define an agent-friendly publishing contract: stable JSON, QA reports,
  artifact paths, mutation lanes, and reviewable patches.
- Define Alan for macOS as a read-only package preview surface for the first
  slice, and as a future review surface for comments and agent patches.
- Decompose the product into follow-up implementation specs.

**Non-Goals:**

- Runtime PDF reflow.
- Full direct-manipulation PDF WYSIWYG editing in the first slice.
- Alan authoring mode in the first slice.
- Automatic agent repair loop as a required first-slice behavior.
- Source files included in `.updf` reader packages by default.
- Traditional DRM or token-gated encrypted content in the first rights model.
- Mandatory blockchain wallets, gas payments, or on-chain checks for ordinary
  readers.
- LaTeX ingestion, EPUB compatibility, DRM, cloud builds, or collaboration.
- A web previewer or Tauri reader in the first slice.

## Decisions

### Separate Writing, Publishing, And Review

UPDF should optimize for author flow first. Authors should write manuscript
content in Markdown and avoid layout code during drafting:

```text
book/
├── manuscript/
│   ├── 01-introduction.md
│   ├── 02-core-idea.md
│   └── assets/
├── book.updf.toml
├── publishing/
│   ├── theme.typ
│   └── components.typ
└── build/
```

The publishing layer maps Markdown manuscript structure into Typst templates,
components, target profiles, and generated/intermediate Typst. The reader layer
packages the completed target PDFs into `.updf`.

This lets authors stay in prose while agents and publishing tools handle
layout, QA, target-specific fixes, and package generation.

Alternative considered: make Typst the primary authoring interface. Typst is a
good publishing language, but asking authors to write layout code while drafting
can break writing flow. Typst remains the right backend for publishing and the
right repair surface for agents.

### Use Agents Primarily In The Publishing And Review Loop

The strongest authoring experience is not direct PDF editing. It is:

```text
author writes Markdown
  ↓
UPDF builds multi-target previews
  ↓
QA and author comments identify issues
  ↓
agent patches publishing layer or manuscript when appropriate
  ↓
UPDF rebuilds and shows before/after
  ↓
author accepts or rejects
```

Comments should be attached to preview targets, pages, and regions. UPDF should
resolve each comment to the nearest known block when possible and present the
agent with relevant context: manuscript source, publishing templates, target
profile, QA report, preview crop, and package artifacts.

Agents should be role-specific:

```text
Writing agent
  improves structure, clarity, citations, examples, and manuscript consistency
  edits Markdown by default

Publishing agent
  improves layout, tables, figures, code blocks, equations, target profiles,
  and theme behavior
  edits publishing templates/profiles first
```

This boundary keeps publishing fixes from silently changing the author's ideas,
and keeps prose editing from being driven by page-layout failures.

### Treat WYSIWYG As Semantic Editing, Not PDF Pixel Editing

Full WYSIWYG PDF editing conflicts with UPDF's single-source, multi-target
model. If a user drags a table on the phone PDF, UPDF still needs to preserve
tablet, desktop, and print outputs from the same source.

The long-term editor should be WYSIWYG-like but semantic:

```text
click a table in preview
  ↓
edit table options, target behavior, caption, or source block
  ↓
agent proposes source/publishing diff
  ↓
rebuild all relevant targets
```

The editor manipulates manuscript blocks, publishing components, target profile
settings, or Typst macro parameters. It does not mutate PDF pixels as the source
of truth.

### Start Implementation With One `crates/updf` Crate And A Standalone Binary

The first implementation should add:

```text
crates/updf
├── src/lib.rs
├── src/main.rs
├── src/core/
├── src/typst/
├── src/qa/
├── src/package/
└── templates/
```

`src/main.rs` exposes the `updf` binary. The crate remains independent from
Alan runtime and macOS app code. Internal modules provide the same logical
boundaries as the long-term architecture: core models, Typst integration, QA,
package handling, and templates. Separate `updf-core`, `updf-typst`, and
`updf-qa` crates can be extracted later if the interfaces stabilize.

Alternative considered: split into several crates immediately. That would make
the architecture look clean on paper, but the first slice needs to validate the
harness contract and package shape quickly. A single crate with firm module
boundaries is lower risk.

### Prove The First End-To-End Slice With Harness And Thin Preview

The first useful experience is:

```text
updf init
updf build
updf qa
updf package
open book.updf in Alan macOS
```

This combines the harness and reader preview, but keeps their dependency
direction strict:

```text
Alan macOS → reads .updf package contract
updf crate → does not depend on Alan runtime or app code
agents/CI → call updf binary through CLI and JSON
```

Alternative considered: build only the harness first. That would make the
compiler boundary clean but would not prove reader package usefulness.
Alternative considered: build preview first. That would risk designing the
package as a private Alan UI artifact instead of an agent-friendly publishing
harness.

### Use Reader Packages And Authoring Projects As Separate Trust Boundaries

The `.updf` artifact is a reader package:

```text
book.updf
├── manifest.json
├── targets/
│   ├── phone.pdf
│   ├── tablet.pdf
│   └── print.pdf
├── qa/
│   ├── report.json
│   └── thumbnails/
└── metadata/
    └── cover.png
```

It does not include source files by default. Source lives in the project
directory. A future authoring bundle may include source, but it should be a
different artifact or an explicit packaging mode.

This protects users from accidentally distributing private source, drafts,
comments, or unreleased assets while still allowing Alan and other readers to
inspect completed packages.

### Prefer Reader-Owned Files Over Traditional DRM

UPDF's first distribution model should favor independent authors and reader
ownership:

```text
author website
  ↓
payment provider or optional crypto payment
  ↓
signed .updf download
  ↓
reader verifies package and opens offline
```

The default package should be DRM-free. It should be signed and hashable so a
reader can verify author identity, edition, package integrity, and target PDF
integrity. This solves trust and provenance without locking the reader to a
single retailer or cloud account.

Alternative considered: require a traditional DRM system from the start. That
would make UPDF less attractive for independent authors and readers, add support
burden, and recreate the same platform-locking behavior the product is trying
to avoid.

### Add Personalized Watermarking Before Encrypted Access Control

The first anti-piracy layer should be optional personalized watermarking:

```text
purchase
  ↓
license id / transaction id
  ↓
personalized .updf
  ↓
visible and/or hidden watermark
```

Watermarks can appear in package metadata, title-page/license pages, PDF
metadata, or subtle target-specific trace markers. The purpose is not to prevent
copying absolutely. It is to make legitimate purchases traceable enough that
authors have a practical response if a copy leaks.

This is Level 1. Token-gated encryption or license-server key delivery is a
future Level 2 and is outside the initial rights model.

### Use Blockchain As Optional Proof, Not Required DRM

Blockchain can help UPDF with proof and portability:

```text
author identity
edition release hash
package hash
license receipt hash
optional transferable license token
optional content-addressed package reference
```

It should not be required for normal purchase or reading. A reader can buy with
ordinary payment and download a file. Advanced users may connect a wallet,
receive a token or verifiable credential, transfer a license, or prove ownership
across compatible readers later.

This keeps the product approachable while preserving a path toward portable
licenses, resale, lending, provenance, and author-owned distribution.

### Support Typst First, Then Add Markdown Manuscript As The Authoring Layer

The first harness can start Typst-first because it is the shortest path to
multi-target PDF output. That does not define the final authoring model.

UPDF should support generic Typst projects so early users and agents can produce
packages quickly. Generic Typst mode generates target wrappers around a
`document(target)` entrypoint and relies on visual/metadata QA.

`updf init` should also create a recommended template:

```typst
#import "../templates/updf.typ": updf

#let document(target) = {
  updf.setup(target)

  = Chapter

  #updf.figure(...)
  #updf.code(...)
  #updf.table(...)
}
```

The `updf.typ` helper library gives agents and future QA rules stable semantic
handles for figures, tables, code blocks, equations, notes, and target-aware
layout behavior. Advanced semantic QA should become stronger for projects that
use these helpers, but generic mode remains supported.

The later authoring implementation should add Markdown manuscript support above
this Typst layer. Markdown is the human writing source. Typst templates and
generated Typst are the publishing source. `.updf` is the reader artifact.

### Treat Mutation As Layered, Not Forbidden

Agents should be allowed to modify Markdown and Typst, but the harness should
document mutation lanes so agents can state and constrain their changes:

```text
config lane
  book.updf.toml or updf.toml target profiles

manuscript lane
  manuscript/*.md prose, structure, figures, citations, examples

publishing lane
  publishing/*.typ, templates/updf.typ, target-aware layout macros

generated lane
  generated Typst intermediates, normally not edited directly
```

For publishing fixes, the recommended repair order is config first, publishing
templates second, manuscript only when the content or structure itself is the
problem. UPDF should not pretend that all layout problems can be solved by
target-profile parameters. Tables, figures, code blocks, equations, and heading
behavior often require Typst macro or local document changes.

The first slice documents these lanes and surfaces `suggested_lanes` in QA
issues. It does not need to implement an automatic repair loop.

### Keep CLI Output Stable For Agents

The v0 command surface should be:

```bash
updf init
updf build
updf build --target phone
updf qa
updf package
updf inspect book.updf --json
```

Every command supports `--json`. JSON output includes `status`, `diagnostics`,
`artifacts`, and machine-readable error codes. Paths should be project-relative
where possible.

`updf inspect` must work on `.updf` without source access, so Alan for macOS,
CI, and other tools can validate packages safely.

`updf preview` is deferred. Alan macOS is the preview consumer for this slice;
a local web previewer can be a later independent feature.

### Make QA Useful Without Overclaiming Semantic Understanding

QA v0 should detect practical problems:

- Typst compile success and diagnostics.
- Page count per target.
- Blank pages.
- Thumbnail generation.
- Obvious overflow or clipped content when detectable from rendered pages.
- Low or high page density warnings.
- Manifest and package consistency.

The report is intentionally agent-readable:

```json
{
  "status": "warning",
  "targets": [
    {
      "id": "phone",
      "pdf": "build/targets/phone.pdf",
      "page_count": 42,
      "issues": [
        {
          "id": "qa-phone-0003",
          "severity": "warning",
          "rule": "low_text_density",
          "page": 17,
          "bbox": null,
          "message": "Page has unusually low content density.",
          "suggested_lanes": ["config", "layout"]
        }
      ]
    }
  ]
}
```

Semantic anchors, table/figure/code/equation-specific rules, and automatic
layout repair are future extensions, especially for projects using `updf.typ`.

### Alan For macOS Opens `.updf` As Read-Only Content

Alan for macOS should add a document preview surface that can:

```text
Open .updf
  ↓
Read manifest
  ↓
Show target list
  ↓
Render selected PDF with PDFKit
  ↓
Show QA badge and issue list when qa/report.json exists
```

This likely maps to a new shell content kind such as `updfPreview`, parallel to
the existing terminal, markdown, and settings content kinds. It should fit the
current pane/split model rather than creating a separate application mode.

The first preview is read-only. It should not edit source, mutate packages,
invoke agent repair, or display raw build internals by default.

Later Alan review surfaces may add comments, issue routing, before/after target
comparison, and agent patch approval while still preserving source-first
publishing boundaries.

### Record Additional Publishing Industry Concerns

The umbrella should keep several publishing concerns visible for later specs
without turning them into first-slice requirements:

- Accessibility: reading order, alt text, table semantics, math accessibility,
  contrast, accessibility metadata, and accessibility reports.
- Metadata and discoverability: title, author, edition, language, ISBN or other
  identifiers, subject/category metadata, descriptions, cover assets, rights
  metadata, and web/catalog metadata.
- Validation and ingestion QA: package validation, manifest validation, broken
  links, embedded fonts, target completeness, metadata completeness, and release
  checks similar in spirit to EPUBCheck.
- Editioning and errata: versioning, minor corrections, major editions, update
  eligibility, release notes, and errata records.
- Samples and marketing assets: sample packages, excerpts, landing-page data,
  cover images, table-of-contents previews, and social/share assets.
- Direct sales operations: payment-provider integration, receipts, license ids,
  download links, re-downloads, refunds, updates, mailing-list hooks, and tax or
  VAT provider integration.
- Reader data portability: bookmarks, highlights, annotations, comments, and
  reading position as portable sidecars rather than app-locked state.
- Print and POD: print-ready PDF targets, trim/bleed/margins, cover spreads,
  font embedding, and preflight checks.
- Citations, bibliography, glossary, and index: manuscript semantics and
  publishing output for technical books, academic books, and reference works.
- Archival provenance: source hash, package hash, build tool versions, Typst
  version, release timestamp, signatures, and deterministic rebuild metadata.

These are important to the long-term product, especially if UPDF becomes an
independent publishing stack. They should be split into focused follow-up specs
after the first harness, preview, authoring, review, and rights slices are
clear.

## Risks / Trade-offs

- Typst CLI may be missing locally -> pure Rust tests and package inspection
  must run without Typst; integration tests should skip or report a clear
  missing-dependency result.
- Markdown manuscript conversion can become a second language design -> keep the
  first Markdown support conservative and define only a small set of semantic
  extensions for figures, tables, code, equations, citations, and cross
  references.
- Visual QA can produce false positives or miss semantic layout problems -> v0
  reports conservative warnings and avoids claiming complete document quality.
- Generic Typst support can limit optimization quality -> `updf.typ` provides
  the recommended path for richer QA and agent edits without blocking generic
  projects.
- Adding preview and harness together broadens the first change -> keep Alan
  preview thin and read-only while putting durable complexity in the package
  contract.
- `.updf` package extraction can expose unsafe paths -> package readers must
  normalize archive entries and reject traversal, absolute paths, or unexpected
  required files.
- Source-free reader packages reduce rebuildability -> authoring remains tied
  to the project directory or future explicit source bundle rather than the
  default reader artifact.
- Agent publishing edits can accidentally alter manuscript meaning -> role
  separation, mutation lanes, diffs, and human approval are required before
  applying agent patches in authoring/review workflows.
- Blockchain can add reader friction and support burden -> make wallet and
  on-chain flows optional, not required for ordinary purchase or reading.
- Watermarking is not strong copy protection -> present it as traceability and
  deterrence, not as absolute piracy prevention.
- Signed DRM-free packages can be redistributed -> use signatures to prove
  authenticity and watermarks to trace leaks rather than promising impossible
  technical prevention.

## Migration Plan

1. Land this umbrella design so the product boundaries are explicit.
2. Split the first implementation change for `crates/updf`, standalone binary,
   Typst-first build, QA, package, and inspect.
3. Split the Alan macOS read-only `.updf` preview implementation.
4. Add a follow-up Markdown manuscript authoring project spec.
5. Add a follow-up preview-comment-agent review-loop spec.
6. Add a follow-up signed package and personalized watermark distribution spec.
7. Add a later semantic editing/WYSIWYG-like authoring spec once comments,
   anchors, and agent patches are proven.

Rollback of the umbrella is documentation-only. Rollback of later implementation
slices should be additive and scoped to the affected harness or preview surface.

## Open Questions

- Which PDF rendering backend should the CLI QA use for thumbnails and page
  image analysis in the first implementation can be selected during planning.
- Whether the first `updf.typ` should include only setup helpers or also figure,
  table, code, and equation wrappers can be finalized in the implementation
  plan.
- Whether package inspection should stream archive entries or extract to a
  temporary directory can be decided during planning, but traversal-safe path
  handling is required either way.
- Which Markdown dialect and extension set should become the authoring default
  should be decided in the Markdown authoring follow-up, but the default should
  stay small enough for ordinary authors to write comfortably.
- Which optional proof layer to use first, such as package signatures only,
  verifiable credentials, or chain-anchored release hashes, should be decided in
  a rights/distribution follow-up.
