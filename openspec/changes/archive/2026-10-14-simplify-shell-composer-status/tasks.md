# Tasks

## 1. Prompt hierarchy
- [x] 1.1 Implement quiet project/model context and conditional attention; verify idle, read-only, active/next divergence, pause, failure, approval and unknown-state tests.
- [x] 1.2 Extend /status with cwd/access/queue details; verify no authority is inferred and complete binding details remain available.
- [x] 1.3 Integrate boundary spacing and update cursor, completion and scrollback fixtures; pass the full terminal UI test suite.

## 2. Acceptance and delivery
- [x] 2.1 Build and run isolated Herdr acceptance with actual output, completion and project/model context; record observations and limits in delivery.md.
- [x] 2.2 Pass quality and strict change validation, review the final diff and publish a reviewable PR with current-head CI status.

## Workflow follow-up
- [x] User merged #1059 as `f2a98f4b`; verified ancestry, synchronized its canonical prompt requirement and prepared archival on 2026-10-14. This documentation patch remains subject to review and merge.
