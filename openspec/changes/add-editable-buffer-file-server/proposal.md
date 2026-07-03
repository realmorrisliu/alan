## Why

`define-editable-buffer-interaction` establishes the Ring 4 editable-buffer
contract, but the contract is not yet executable. The next useful North Star
step is a headless `editfs` file server that proves `body` / `tag` / `addr` /
`ctl` / `event` semantics through aP before any native UI work.

## What Changes

- Add an `alan-editfs` user-space aP file-server crate.
- Serve one editable buffer directory with `body`, `tag`, `addr`, `ctl`, and
  `event` files.
- Support editable UTF-8 `body` and `tag` files with revisioned body content.
- Support an `addr` file for revision-bound byte ranges over `body`.
- Support snapshot-bearing explicit `exec` through `ctl`, recording accepted or
  denied execution events without granting authority or running privileged side
  effects.
- Append edit, address, and execution records to an observable blocking-read
  `event` stream.
- Keep the slice headless: no macOS UI, mouse behavior, syntax styling, or shell
  product workflow changes.

## Capabilities

### New Capabilities

- `editable-buffer-file-server`: Defines the first headless `editfs` file-server
  implementation of the editable-buffer interaction contract.

### Modified Capabilities

- None.

## Impact

- Adds a new workspace crate, `alan-editfs`, depending on `alan-ap`.
- Adds focused aP integration tests for the buffer file surface, revision-bound
  range addressing, explicit execution, event observation, and M0-M2 independence.
- Does not change `alan-kernel`, agent runtime startup, macOS UI, or the current
  agent `io/` + `ctl` path.
