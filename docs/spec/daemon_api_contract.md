# Legacy Bridge: Daemon API Contract

This path is not the authoritative contract source.

Authoritative OpenSpec owners:

- `openspec/specs/daemon-api-contract/spec.md`
- `openspec/changes/harden-agent-operating-system-contracts/specs/daemon-api-contract/spec.md`

Remote access is not a daemon API concern: the OS remote-entry contract is
`remote-access-service` (`openspec/changes/define-remote-access-service/`),
and `remote-control-contract` is frozen legacy per ADR-0028 D11 — the
daemon-era remote surface takes no new requirements.

Keep new daemon API requirements in OpenSpec. Remove this bridge once active
references stop using the legacy `docs/spec/daemon_api_contract.md` path.
