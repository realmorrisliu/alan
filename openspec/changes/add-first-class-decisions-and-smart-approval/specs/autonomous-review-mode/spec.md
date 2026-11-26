# Spec Delta

## ADDED Requirements

### Requirement: System 1 smart approval reuses the no-tools decision boundary
Under `TrustPosture::ConfinedSmart`, System 1 smart approval SHALL evaluate
reviewer-judged escalations through `connections/<conn>/decide` with zero
tools, bounded deadlines, untrusted-data transcript framing, and customizable
policy criteria. Denials SHALL return rationale for safer self-correction and
feed the existing denial circuit breaker (3 consecutive, or 10 in 50). Changed
action, policy, grants, or posture SHALL invalidate stale results.

#### Scenario: Prompt injection in tool output cannot coerce System 1 allow
- **WHEN** untrusted tool output contains text instructing the evaluator to
  approve a policy-violating action
- **THEN** the decision request frames that text strictly as untrusted `state`
  data with zero tools
- **AND** System 1 judges conformance against policy alone or fails closed to
  human approval

#### Scenario: System 1 denial triggers self-correction and circuit breaker
- **WHEN** System 1 returns `deny` for a reviewer-judged escalation
- **THEN** the main agent receives the denial rationale with guidance to choose
  a safer path or ask the user
- **AND** reaching the 3-consecutive or 10-of-50 denial threshold aborts the
  turn to the human

#### Scenario: Revoked grant invalidates in-flight System 1 allow
- **WHEN** a Host Mount grant or trust posture changes while a `decide()` call
  is in flight
- **THEN** the completed allow answer is discarded as stale and cannot
  authorize execution
