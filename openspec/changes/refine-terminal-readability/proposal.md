# Proposal

## Why

Code fences still appear as raw Markdown, adjacent answers lack separation, and
narrow status lines can allocate zero columns to the model. The user requested
these three focused refinements while preserving stable input and scrollback.

## What Changes

- Present fenced-code boundaries and language labels using semantic spans while
  preserving literal code and source-based scrollback cuts.
- Add one stable leading separation row to nonempty assistant content.
- Allocate narrow prompt space to model and meaningful state before project,
  reasoning detail and redundant queue indicators.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `rust-inline-tui`: qualify fence labels, answer spacing and narrow status priority.

## Impact

Only the existing Rust terminal renderer, its focused tests and this OpenSpec
change. No runtime authority, model-selection behavior or dependency changes.
