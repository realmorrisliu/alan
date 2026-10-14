# Connection Service owns profiles; Host adapters own secrets

Status: accepted; product-store identity refined 2026-10-14 after merged PR #1042.
See the [current connection contract](../../openspec/specs/provider-connection-contract/spec.md).

Alan OS Connection Service owns provider/model settings, profile identity,
defaults, selection, validation status, and callable LLM connection trees in
the System Store and namespace. Platform adapters alone perform browser or
device login and store secrets in Keychain or another owning credential store,
returning opaque credential references that reveal no secret bytes. Legacy
`~/.alan/connections.toml`, Host-owned profile selection, and inline Agent
secrets are removed. The Host explicitly binds the product System Store for
metadata and its paired Host credential store. Isolated fixtures bind separate
stores; no install channel selects metadata or auth state. A Connection uses a
credential only through an explicit reference.
