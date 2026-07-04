## Context

`define-editable-buffer-interaction` defines the Ring 4 editable-buffer contract.
This change is the first executable slice: a headless file server that can be
mounted and tested through aP. The goal is to prove the file semantics before
native UI, shell integration, or policy-rich execution.

Existing Alan OS primitives are enough: aP files/directories, writable document
files with commit-on-clunk, and blocking-read `Stream`s for observation.

## Goals / Non-Goals

**Goals:**

- Add `alan-editfs` as a user-space aP file-server crate.
- Serve one buffer directory exposing `body`, `tag`, `addr`, `ctl`, and `event`.
- Support body/tag edits, revision-bound address ranges, snapshot-bearing
  `ctl exec`, and observable JSON-line events.
- Keep execution policy injectable and default-denied so the buffer never becomes
  a privileged command runner.

**Non-Goals:**

- Native UI or Alan for macOS integration.
- Real shell/process execution.
- Multi-buffer discovery, persistence, CRDT editing, syntax highlighting, or
  rich editor commands.
- Replacing agent `io/` + `ctl`.

## Decisions

1. **One crate, one headless buffer surface.**

   `alan-editfs` implements a single editable buffer rooted at the server root.
   The first slice avoids a `buffers/<id>` hierarchy so tests and future clients
   can focus on the file contract. A later slice can add multi-buffer allocation
   through a clone file without changing the per-buffer shape.

2. **Body and tag are commit-on-clunk UTF-8 documents.**

   Writes buffer by fid and commit at `clunk`, matching the existing document
   pattern in `llmfs` and `routefs`. Invalid UTF-8 fails at commit time with
   `ErrorCode::BadRequest`.

3. **`addr` uses revision-bound byte ranges.**

   V1 write format is `rev:<body-revision> <start>..<end>`. Reads include the
   selected body revision, address revision, and range as
   `rev:<body-revision> addr:<addr-revision> <start>..<end>`. A `ctl exec`
   document carries that full snapshot so execution can reject stale body
   revisions, stale address revisions, and retargeted ranges without inventing a
   full editor address language.

4. **`ctl exec` is explicit, snapshot-bearing, and policy-gated.**

   Writing `exec rev:<body> addr:<addr> <start>..<end>` to `ctl` executes only
   if the supplied snapshot exactly matches the current active `addr` range and
   body revision, then records the injected `ExecutionPolicy` outcome. The
   default policy denies all execution. A test policy can accept so we verify
   both accepted and denied event records without running a real shell command.

5. **Events are JSON lines on a blocking-read stream.**

   `event` records edits, address changes, and execution outcomes. Consumers can
   `read` at the live edge and block until a record arrives, matching ADR-0024
   observation semantics.

## Risks / Trade-offs

- [Risk] Byte-range addressing can split a UTF-8 scalar. -> Mitigation: body and
  tag contents must be UTF-8, and execution range extraction validates selected
  bytes as UTF-8 before policy handling.
- [Risk] A record-only execution policy may look weaker than real execution. ->
  Mitigation: this slice proves explicit control, range validation, permission
  outcome, and event audit; real shell/process adapters remain a later bounded
  change.
- [Risk] A single-buffer root may be too small for product use. -> Mitigation:
  it preserves the final per-buffer file shape; allocation/discovery can be
  layered later.
