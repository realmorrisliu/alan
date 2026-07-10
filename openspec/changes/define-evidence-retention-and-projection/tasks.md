## 1. Projection Contract

- [x] 1.1 Define the projection record shape: bounded preview, namespace-path reference (path + optional offset/length), truncation metadata; document the stream/record vocabulary in-band per the self-describing-namespace requirement.
- [x] 1.2 Implement oversized tool-output projection at the tape-persistence seam, referencing `actions/<id>/output`.
- [x] 1.3 Implement oversized child-output projection for delegated results, deciding the reference target (parent-side action record vs child `io/output`) per the open question and recording the decision in design.md.
- [x] 1.4 Assert reference resolvability at emission time; fall back to marked inline preview when no resolvable path exists.
- [x] 1.5 Add tests: long tool output projection, long child output projection, unresolvable-reference fallback.

## 2. Delegated output_ref Migration

- [x] 2.1 Change delegated `output_ref` generation to emit namespace paths; demote raw rollout paths to optional debug metadata.
- [x] 2.2 Return structured errors (missing, retention-expired) preserving preview and child-run metadata on failed resolution.
- [x] 2.3 Add tests: parent resolves child output ref via namespace walk, error paths for missing/expired refs.

## 3. Retention

- [x] 3.1 Ensure content referenced from a durable tape stays reachable at least as long as the citing tape (reachability via content-addressed store roots or equivalent pinning in agentfs).
- [x] 3.2 Implement structured retention-expiry errors on reference resolution.
- [x] 3.3 Add tests: post-exit readability of action output, expiry error after simulated GC.

## 4. Redaction

- [x] 4.1 Apply redaction before durable evidence persistence, emitting explicit redaction markers with reason classes, distinct from truncation metadata.
- [x] 4.2 Add tests: secret redaction marking, auditor-distinguishable truncation vs redaction.

## 5. Verification And Archive Readiness

- [x] 5.1 Run `just verify` and fix fallout.
- [x] 5.2 Run `openspec validate define-evidence-retention-and-projection --strict`.
- [ ] 5.3 Open PR, address review, merge.
- [ ] 5.4 After merge, sync delta specs into `openspec/specs/` and confirm archive readiness.
