## ADDED Requirements

### Requirement: Ghostty fork is repository managed
The Apple client SHALL use a repository-managed, pinned Alan-maintained Ghostty
fork at `third_party/ghostty` for Alan-owned PTY integration work instead of
relying only on arbitrary developer-local Ghostty checkouts.

#### Scenario: Submodule is initialized
- **WHEN** a developer prepares macOS terminal dependencies
- **THEN** the supported setup path initializes or verifies the pinned Ghostty fork submodule at `third_party/ghostty`
- **AND** generated Ghostty framework, resources, and terminfo artifacts are derived from that pinned source unless an explicit developer override is used

#### Scenario: Submodule is missing
- **WHEN** the Ghostty fork submodule is absent or uninitialized
- **THEN** setup and build checks report the missing submodule and provide the supported initialization command
- **AND** the failure does not look like an opaque linker or module-map error

#### Scenario: Developer override is used
- **WHEN** a developer intentionally points setup at a non-submodule Ghostty checkout
- **THEN** the setup output identifies the override source and revision
- **AND** review or CI paths continue to use the pinned repository-managed source by default

### Requirement: Ghostty artifact drift is checked
The Apple build/test setup SHALL detect when local Ghostty artifacts do not
match the pinned Ghostty fork revision or declared cache key used for the
current Alan checkout.

#### Scenario: Artifacts match pinned revision
- **WHEN** local Ghostty artifacts were built from the pinned submodule revision
- **THEN** dependency checks accept them and record the revision in setup output or metadata

#### Scenario: Artifacts are stale
- **WHEN** local Ghostty artifacts were built from a different Ghostty revision without an explicit override
- **THEN** dependency checks fail or warn according to the selected strictness mode
- **AND** the output points to the supported rebuild command

### Requirement: Alan-owned PTY runtime has focused verification
The Apple client SHALL include focused tests or documented integration checks
for Alan-owned PTY allocation, process launch, renderer attachment, text
delivery, resize, signals, exit observation, and bounded transcript capture.

#### Scenario: Fake PTY runtime accepts delivery
- **WHEN** focused tests create a fake Alan-owned PTY runtime and send terminal text
- **THEN** tests verify that delivery is acknowledged from the PTY runtime service rather than from renderer visibility

#### Scenario: Fake process group receives interrupt
- **WHEN** focused tests request interrupt for a fake active foreground process group
- **THEN** tests verify that the runtime service records the attempted signal and reports a stable result

#### Scenario: Ghostty integration lane runs
- **WHEN** local Ghostty artifacts are prepared from the pinned fork
- **THEN** the Ghostty integration lane verifies renderer attachment to the Alan-owned PTY path or reports a clear unsupported-seam failure
