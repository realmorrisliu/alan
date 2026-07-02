## Why

ADR-0026 D4 records the final Ring 4 idea in the North Star map: an editable
buffer layer where text is the programmable surface and the interaction surface
is itself a file server. The Ring 3 composition slices now have concrete changes,
so this deferred M4+ layer needs a bounded OpenSpec contract before anyone starts
copying Acme's UI details or mixing the idea into the M0-M2 `io/` + `ctl` path.

## What Changes

- Define the editable-buffer interaction surface as a future Alan OS file-server
  contract above append-only agent `io/` streams.
- Specify the durable file shape: `body`, `tag`, `ctl`, `addr`, and `event`
  semantics adapted to Alan, not literal Acme behavior.
- Define what "execute text" means in Alan: a selected text range resolves to an
  explicit shell/action/process operation through normal namespace capabilities.
- Preserve symmetry: humans and agents can both read, edit, observe, and drive
  the same surface through files.
- Keep this change scoped to the contract and first non-UI harness; native macOS
  UI, mouse chords, syntax highlighting, and broad product workflow are later
  implementation slices.

## Capabilities

### New Capabilities

- `editable-buffer-interaction`: Defines the M4+ interaction file-server surface
  for scriptable shared text buffers, executable text ranges, and observable
  edit/control events.

### Modified Capabilities

- None.

## Impact

- Adds an OpenSpec owner for ADR-0026 D4 / ADR-0027 Ring 4.
- Future implementation will add a user-space file-server crate above `alan-ap`
  without changing `alan-kernel`.
- The current M0-M2 agent path remains `io/` + `ctl`; this change must not be
  used to delay the Ring 2 finish line or require native UI work before the
  file contract exists.
