# Guardian deadline slice — 2026-10-23

Implementation branch: `codex/guardian-review-bound-20261023`, based on budget
candidate `662d14ba` (PR #1061). This is a narrow prerequisite for tasks 2.1–2.2,
not their completion, installed behavior or reviewer qualification.

## Implemented boundary

Production still uses the admitted main Connection and built-in policy. One review
now has a 30-second aggregate decision deadline covering the existing maximum four
model attempts and their backoff. A shorter nonzero request timeout also shortens
this aggregate deadline. Zero no longer means an unlimited reviewer wait. These are
implementation guardrails, not the frozen daily-use qualification thresholds.

All attempts still charge the original Machine task ledger. Its lower remaining
time or call allowance wins. Review timeout cancels only a child token, awaits
existing namespace retirement and then takes the existing correlated human-wait
path. The decision deadline does not promise that cleanup finishes at the same
instant: cleanup acknowledgement follows its existing finite adapter bound.
Unconfirmed cleanup and task exhaustion retain their typed failure paths, rather
than being converted to an ordinary permission request or a new allowance.

User cancellation also waits for retirement, then propagates cancellation without
creating a confirmation. An allow response containing Tool calls or response
warnings is unavailable; malformed Tool calls must not disappear into an otherwise
successful allow. No reviewer-returned Tool is dispatched.

The request, transcript snapshot, main Connection binding and policy remain fixed
through the existing retry loop. This slice does not add policy/profile selection,
new grants, publication delegation, fallback to a different Connection, concurrent
pending work or full trust.

## Verification

The focused `cargo test -p alan-agent-engine guardian -- --nocapture` run passed
19 tests on this source slice. New namespace-backed cases cover retry backoff
without fresh admission, zero/long request timeouts, acknowledged abort before
return, the four-call limit and original ledger charges, a shorter task deadline,
and user cancellation without cancelling its parent. The authorization boundary
case checks one correlated wait on deadline and no request on user cancellation.
A local response test rejects tools or warnings alongside allow text.

Full engine regression passed 1,574 tests, with one existing ignored test. The two
independent review axes found no issues in implementation tree `03195579` against
`662d14ba`. Strict validation using CI-pinned OpenSpec 1.4.1 passed all 70 items.
The global 1.14.3 CLI additionally rejects omitted historical scenario names in
the pre-existing planning deltas; those deltas are unchanged by this slice. Its
diagnostic remains recorded separately, not counted as a successful validation.
Quality and CI remain separate delivery gates. Private command logs are retained at
`~/Library/Caches/Alan/guardian-review-bound/20261023/`. No paid native model run or
new reviewer comparison was performed by the focused suite; it uses real llmfs
and AgentFS with deterministic provider fixtures.

## Remaining gates

Tasks 2.1 and 2.2 remain unchecked. Dedicated reviewer capture and trusted-user
policy selection, aggregate limits across the future dedicated/main fallback,
grant/policy/profile revocation and stale-result invalidation still need delivery.
The new profile/policy must pass the separately frozen no-effect qualification
before activation. Formal daily-use qualification remains zero tasks / zero days.
