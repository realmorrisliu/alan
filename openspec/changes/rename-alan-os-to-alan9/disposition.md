# Disposition — 2026-09-24

Naming direction accepted by the user and recorded in ADR-0057. This change
plans documentation adoption; repository-wide alignment has not been performed.
No runtime or machine-identifier migration is authorized by its tasks.
TUI/Shell unification and model-assisted input routing remain a separate
exploration under the existing interaction and cognition changes.

## Deferred follow-up — brand validation

The `product-brand-identity` specification requires an allowlisted brand
validator, but a repository audit on 2026-09-25 found no implementation in the
quality command, scripts, crates, or CI. Per user direction, this naming change
does not add a checker. Track the pre-existing specification gap as a separate
future OpenSpec change; this migration does not claim that invalid product
branding is currently rejected.
