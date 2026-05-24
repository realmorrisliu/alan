## ADDED Requirements

### Requirement: Single alan binary distribution is verified
The Apple client and release packaging checks SHALL verify that terminal
distribution uses one command-line executable, `alan`, and SHALL reject
reintroducing `alan-tui` as an embedded, signed, linked, installed, or launched
binary.

#### Scenario: Release app layout is checked
- **WHEN** release packaging implementation is ready for review
- **THEN** focused checks verify `Alan.app` contains the app executable and the
  embedded command-line `alan` executable required by the distribution contract
- **AND** focused checks fail if `Alan.app` contains an embedded
  `Contents/Resources/bin/alan-tui` executable

#### Scenario: Signatures are checked
- **WHEN** release signing checks inspect embedded command-line tools
- **THEN** they verify the embedded `alan` binary is signed according to the
  release signing contract
- **AND** they do not require or accept a separately signed `alan-tui` binary as
  part of the release package

#### Scenario: Command-line links are checked
- **WHEN** direct app install scripts or future Homebrew cask metadata are
  inspected
- **THEN** they link or install only `alan` from the app bundle
- **AND** they fail if they link, install, or document `alan-tui`

### Requirement: Legacy TypeScript TUI build paths are blocked
The Apple client build/test contract SHALL include focused validation that
prevents production packaging, install, and shell launch flows from depending on
the deleted TypeScript/Bun/Ink TUI.

#### Scenario: Bun TUI build is reintroduced
- **WHEN** release packaging or local install scripts are inspected
- **THEN** focused checks fail if they build, bundle, sign, or launch
  `clients/tui`, Bun, Ink, or a standalone TypeScript TUI artifact

#### Scenario: TUI path override is reintroduced
- **WHEN** command-line launch or app shell scripts are inspected
- **THEN** focused checks fail if production code uses `ALAN_TUI_PATH` as a
  fallback path for the terminal UI

#### Scenario: Legacy TUI dependency remains
- **WHEN** package metadata and install scripts are inspected after migration
- **THEN** no required production build or install step depends on the deleted
  TypeScript TUI package

### Requirement: Apple shell launches bare alan
The Apple shell SHALL launch the command-line terminal experience through bare
`alan` and SHALL NOT launch `alan chat`, `alan ask`, or `alan-tui` for the
default alan terminal tab.

#### Scenario: Default alan tab command is inspected
- **WHEN** shell contract checks inspect the default alan terminal launch command
- **THEN** the command resolves to bare `alan`
- **AND** it does not include `chat`, `ask`, or `alan-tui`

#### Scenario: Legacy commands are reintroduced
- **WHEN** Apple shell scripts, control paths, or documentation reintroduce
  `alan chat`, `alan ask`, or `alan-tui` as the default terminal launch path
- **THEN** focused shell contract checks fail with a message pointing to the
  single-binary Rust TUI contract
