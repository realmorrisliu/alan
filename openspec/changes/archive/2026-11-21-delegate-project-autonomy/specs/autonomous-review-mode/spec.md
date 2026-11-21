# Spec Delta

## MODIFIED Requirements

### Requirement: Escalations are classified into four outcomes
Governed execution SHALL classify each action as auto-allow, deny, trusted-user decision or reviewer-judged, using current delegation before requesting repeated approval. Deterministic denials, absent authority, control-policy changes and uncontainable effects SHALL precede reviewer execution. An exact user-origin force-push or remote-delete delegation MAY satisfy that action approval; no such authority SHALL be inferred from ordinary publication.

#### Scenario: Action has scoped authorization
- **WHEN** a publication action exactly matches valid delegated scope
- **THEN** it can pass the authority check without repeated approval; other checks remain

#### Scenario: Action needs new authority
- **WHEN** a proposed action needs a private resource or privilege not delegated
- **THEN** the reviewer cannot grant it

#### Scenario: Contained ambiguous action
- **WHEN** a proposed action is within granted scope but needs judgment
- **THEN** the qualified reviewer evaluates it
### Requirement: Reviewer is a no-tools structured decision, fail-safe
The reviewer SHALL use a configurable dedicated Connection and trusted-user policy, with bounded optional fallback to the admitted main Connection. It SHALL return structured allow, deny or uncertainty without tools. Aggregate deadlines and call budgets SHALL include retries and fallback. Failure or uncertainty after that budget SHALL preserve one pending human decision and MUST NOT auto-allow. A changed action, policy, profile, grant or mode SHALL invalidate a stale assessment.

#### Scenario: Reviewer unavailable
- **WHEN** review fails and bounded fallback cannot resolve it
- **THEN** one correlated pending request remains; only provably independent authorized work may continue

#### Scenario: Decision becomes stale
- **WHEN** authority is revoked while reviewer evaluation is pending
- **THEN** the old allow result cannot authorize execution

#### Scenario: Reviewer isolated
- **WHEN** a review request is evaluated
- **THEN** it has no tools and treats transcript content as untrusted evidence
