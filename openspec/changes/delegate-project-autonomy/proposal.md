# Proposal

## Why

Ordinary project work still suffers repeated or poorly attributed interruptions, and the existing reviewer contract exceeds its actual configuration wiring. Explicit delegation should permit continuous work while keeping authority with its existing owners.

## What Changes

- **BREAKING** Replace the prohibition on saved project delegation with explicit user-authored standing scopes; a one-call approval still never widens future authority.
- Separate delegated scope, approval posture and actual execution isolation.
- Reuse Guardian with a configurable dedicated Connection and user-selected policy; qualify its rapid decisions separately from input routing.
- Support bounded main-model fallback and correlated pending requests without blocking provably independent work.
- Add task budgets and scoped publication delegation; force-push and remote deletion are distinct from ordinary push.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `auto-approve-policy`: Governed project posture, delegation precedence and nonimplicit persistence.
- `autonomous-review-mode`: Dedicated reviewer, bounded uncertainty/fallback and qualification.
- `host-mount-service`: Saved Host delegation identity, projection and revocation.
- `agent-mount-escalation`: Reuse of previously user-authorized scope without model-created grants.
- `sandbox-autonomy-invariants`: Scoped publication rules and qualified reviewer limits.
- `agent-namespace-runtime`: Task budgets and dependency-aware pending actions.

## Impact

Agent Runtime governance, Host Mount Service, Connection adapters and AgentFS/TUI controls. Does not introduce a new Kernel Process type, permission-learning model or ambient project overlay. Explicit trust posture is a separate successor.
