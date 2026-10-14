# Repeated-development qualification delivery

## Verified merged implementation

| Delivery | Merged revision | Evidence |
| --- | --- | --- |
| Linux toolchain predecessor #1046 | `465e3e47` | Existing native toolchain/authority acceptance |
| Repeated development and observed repairs #1047 | `8802b541` | Thirty-slot matrix and retained attempts |
| Rust query provider follow-up #1056 | `34396740` | Final source `d42b0c58`, all sixteen checks pass, completed Codex review #6136929442, both P1 threads resolved |
| UI and directory canonical closure #1045 | `d2d044b5` | All sixteen checks pass, completed review, no unresolved threads |

Main at `d2d044b5` contains both requested merges. The six production/test files
changed by #1056 match its reviewed head on main; #1045 adds documentation only.
No new model task, changed grant or installed-runtime deployment is claimed.

## Canonical synchronization

This documentation-only delivery transfers thirteen requirement blocks verbatim
from the merged change into seven existing owners: runtime-harness-contract,
os-sandbox-enforcement, host-mount-tool-process-sandbox-projection,
provider-connection-contract, evidence-retention-and-projection, rust-inline-tui
and tool-result-presentation. Three existing requirements are extended; ten are
added. Existing unrelated requirements and all previously archived content remain
unchanged, including the UI requirements merged through #1045.

Tasks 1.1–4.4 are complete (18/19); task 4.5 remains open because the canonical
patch is prepared but not yet adopted on main. This closure patch still needs
its own current-head CI/review and user merge. Until then canonical adoption on
main is pending and the change remains active. Once merged, verify the merged
revision and exact canonical blocks before archive; no further product test is
required for a byte-identical documentation-only adoption.

## Qualification boundaries

The original thirty first outcomes remain 21 PASS/9 FAIL. Ninety-one retained
model attempts remain 68 PASS/23 FAIL; retries do not replace first outcomes.
Linux F4 remains assisted. Delegated child launch remains unqualified; root
fallback completion does not prove delegation. Shared authentication is not
credential independence. Provider usage/cost and physical cursor/scroll gestures
remain unmeasured. This does not establish automatic routing or general
unassisted self-bootstrap. The query repairs retain the historical-provider
applicability audits; adapter regressions are not new model-development attempts.

Detailed controls, source bindings, failed harness pilots and retained evidence
are recorded in [native-runs.md](native-runs.md). Historical statuses in those
records do not override this delivery state.
