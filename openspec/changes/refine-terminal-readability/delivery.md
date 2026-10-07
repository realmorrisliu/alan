# Delivery — 2026-10-07

Status: local implementation, native acceptance and independent review passed;
remote current-head CI remains separate. Base `a239021e` includes model catalog
PR #1033; this UI slice is stacked on that branch until it merges.

- 335 terminal library tests passed, including continuous 16–80-column queue
  and model-control states, streamed fence labels, literal code, Unicode,
  repeated drain, reconciliation, completion anchors and accessibility policy.
- Final-tree `just quality` and strict OpenSpec validation passed.
- Independent Spec and Standards review passed on frozen patch SHA-256
  `d65644f740bba16883ec9b24a9ec46d049b3c85a9b818d91abfb13f3741cad25`.
  Review caught queue metadata consuming model space and bare ellipses at
  critical widths. Both root causes and the missing boundary tests were fixed.
- Native evidence: `~/Library/Caches/Alan/ui3/`, pinned binary/source in
  `build.json`, artifact hashes in `evidence-sha256.json`. PID `80519`, boot
  `1ca9beff-f2d3-4242-8bfd-080650df0a2c`, Root `8`, owned named Herdr session
  `alan-ui-readability-20261007`, pane `w1:p1`.
- Real Sol/medium output verified Rust and diff boundary labels, four-space
  code indentation, diff signs and answer separation at 94 and 47 columns.
  The narrow prompt retained `next gpt-6.1-sol`, ready and queue state.
- Typing `/project` character by character kept the input at visible row 28
  throughout candidate filtering. Existing native-backend tests also verify
  actual cursor coordinates and no candidate-driven history drain.
- A second real 40-line answer forced host scrollback. Both earlier code blocks,
  literal code and every numbered answer line remained available exactly once;
  widening back to 94 columns preserved the same result.
- `/quit` exited normally with code 0 after the queue settled; native PID gone.
  Test-only pane/session were closed. No runtime or profile defaults changed.
