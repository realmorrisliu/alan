# Legacy Bridge: Remote Control Architecture

This path is not the authoritative contract source.

Authoritative OpenSpec owners:

- `docs/adr/0028-remote-attachment-model.md` (the remote attachment model)
- `openspec/changes/define-remote-access-service/specs/remote-access-service/spec.md`
  (the OS remote-entry contract)
- `openspec/specs/remote-control-contract/spec.md` (daemon-era surface, frozen
  legacy per ADR-0028 D11 — deletion target, no new requirements)
- `openspec/changes/add-alan-anywhere-mvp/specs/alan-anywhere/spec.md` (the
  product plane)

Keep new remote architecture requirements in OpenSpec. Remove this bridge once
active references stop using the legacy `docs/spec/remote_control_architecture.md`
path.
