## 1. SandboxSpec Defaults

- [x] 1.1 Add a deterministic helper that derives the sensitive-read denylist
  from a home directory.
- [x] 1.2 Make `SandboxSpec::seed` include the default sensitive-read denylist
  while keeping network denied and the workspace as the seed writable root.
- [x] 1.3 Cover the denylist contents with focused unit tests.

## 2. Composition Projection

- [x] 2.1 Update host-mount sandbox projection to start from
  `SandboxSpec::seed` and append canonical read-write host mount roots.
- [x] 2.2 Cover host-mount projection preservation of the sensitive-read denylist
  with tests.

## 3. Verification And PR

- [x] 3.1 Run focused Rust tests for sandbox defaults, Seatbelt profile output,
  and host-mount projection.
- [x] 3.2 Run clippy for the touched crates and validate this OpenSpec change.
- [x] 3.3 Update the parent namespace-driven sandbox task list to record this
  P3 macOS sensitive-read slice.
- [x] 3.4 Commit the slice and open a ready stacked PR above
  `feat/northstar-host-dir-fs`.
