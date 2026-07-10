## MODIFIED Requirements

### Requirement: Runtime validates model-mediated memory write plans
alan SHALL use model-mediated semantic judgment for automatic memory promotion
while separating runtime candidate planning from Memory Store commit authority.

Write-plan contract:

1. Agent Execution Engine chooses when to invoke bounded write planning and which
   active-turn records are in scope.
2. The model returns bounded structured output with memory kind, selected mounted
   store, namespace target, confidence, disposition, observation, evidence, and
   promotion rationale.
3. Runtime validates the response schema and writes an eligible candidate to the
   selected Memory Store proposal file; runtime does not directly mutate durable
   memory documents.
4. The Memory Store validates target containment, access rights, dedupe,
   redaction, current content, and transaction preconditions before commit.
5. The Memory Store is the only authority that mutates its durable documents,
   ledger, and revert state.
6. Invalid, mismatched, or over-broad candidates are rejected; low-confidence or
   ambiguous candidates may be staged by the store.

Direct stable writes require a validated `promote_now` disposition and at least
one of:

1. the user explicitly says to remember it
2. the user directly states the fact as stable identity, preference, or
   constraint
3. the user authorizes a source lookup that directly states the fact
4. the fact is already in stable memory and the new turn updates it

#### Scenario: Write plan is over-broad
- **WHEN** the model proposes a write that spans unrelated facts, mismatches its
  mounted store or target, lacks evidence, or exceeds the bounded schema
- **THEN** runtime does not directly mutate memory and the Memory Store rejects
  or stages the candidate

#### Scenario: User asks Alan to remember a stable preference
- **WHEN** a validated proposal marks the preference as `promote_now` and a
  writable authorized Memory Store is mounted
- **THEN** the store commits the durable change and ledger record with source
  evidence

#### Scenario: Target store is not mounted
- **WHEN** a proposal names a Personal, App, or Workspace Memory Store unavailable
  in the agent namespace
- **THEN** runtime cannot submit the proposal to that store
- **AND** no global store id or host path bypasses namespace reachability
