# Connection Service owns profiles; Host adapters own secrets

Status: accepted

Alan OS Connection Service owns provider/model settings, profile identity,
defaults, selection, validation status, and callable LLM connection trees in
the System Store and namespace. Platform adapters alone perform browser or
device login and store secrets in Keychain or another owning credential store,
returning opaque credential references that reveal no secret bytes. Legacy
`~/.alan/connections.toml`, Host-owned profile selection, and inline Agent
secrets are removed; install channels have independent metadata and may share a
credential only through an explicit reference.
