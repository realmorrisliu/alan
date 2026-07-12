## Context

The Developer ID-signed Alan Dev app and privileged helper install successfully, and the helper
reports healthy. The available `univer` account has no current Alan ownership marker, so the
helper correctly refuses to treat it as Alan-managed. The operator has deferred account setup and
the real Managed User PTY smoke until a later session.

## Goals / Non-Goals

**Goals:**

- Preserve a truthful, independently actionable record of the remaining live verification.
- Reuse the current signed helper, ownership-marker diagnosis, and live PTY smoke path.
- Require sanitized evidence before this follow-up is completed.

**Non-Goals:**

- Automatically adopt, repair, create, or delete any local account.
- Reintroduce sudoers, daemon-era persistence, migration, or compatibility behavior.
- Change the Managed User product contract or implement a new verification path.

## Decisions

1. Keep the verification as its own active OpenSpec change. This lets the legacy cleanup archive
   without turning an operator-deferred prerequisite into a false pass. Leaving only an unchecked
   archived task was rejected because archived planning is non-normative.
2. Require explicit operator selection and authorization of the test account. An existing account
   without a current ownership marker remains non-Alan-owned and must not be inferred or adopted.
3. Use the existing Alan Dev signed-helper status, diagnosis, and live PTY smoke workflow. A new
   test harness would add another oracle without improving the product boundary.
4. Record only sanitized status and outcome evidence. Account credentials, tokens, and unrelated
   local-account details remain outside the change.

## Risks / Trade-offs

- [The live path remains unverified until the operator schedules it] -> Keep this change active
  and do not claim end-to-end Managed User readiness in release evidence.
- [Local account mutation is privileged and destructive if mis-scoped] -> Require explicit
  operator account choice and use only the current previewed helper plan.
- [A failure may reveal a product defect] -> Record the failure here, then open a separately
  scoped implementation change rather than expanding this verification change implicitly.
