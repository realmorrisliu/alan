# Disposition — accepted 2026-09-24; closed 2026-09-25

Naming direction accepted by the user and recorded in ADR-0057. Active
explanatory prose adoption was delivered by PR #934, merged as
`5db591a3b37d20b21ec23862669fe1948555c8b2`; the canonical specification sync
was delivered by PR #935, merged as
`53b862b06a0cdb98d37f3110a8d6d8a6aae64ce0`. This change is complete and
archived. No runtime or machine-identifier migration was authorized or made.
TUI/Shell unification and model-assisted input routing remain a separate
exploration under the existing interaction and cognition changes.

## Deferred follow-up — brand validation

The `product-brand-identity` specification requires an allowlisted brand
validator, but a repository audit on 2026-09-25 found no implementation in the
quality command, scripts, crates, or CI. Per user direction, this naming change
does not add a checker. Track the pre-existing specification gap as a separate
future OpenSpec change; this migration does not claim that invalid product
branding is currently rejected.
