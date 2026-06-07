## ADDED Requirements

### Requirement: Alan Emacs Install And Editor Env Have Focused Verification
Alan SHALL verify the Alan Emacs install manager and macOS terminal editor
environment through focused tests that do not require launching the full app UI.

#### Scenario: Install manager selection is tested
- **WHEN** focused CLI tests run with temporary home directories
- **THEN** they cover missing, empty, Alan-owned, user-owned, and conflicting
  Emacs config entry candidates

#### Scenario: Install lifecycle is tested
- **WHEN** focused CLI tests run
- **THEN** they cover idempotent install, status reporting, doctor checks, safe
  uninstall, and refusal to mutate non-Alan-owned user configuration

#### Scenario: Terminal editor env is tested
- **WHEN** Apple shell runtime metadata tests inspect terminal boot
  environments
- **THEN** they verify `EDITOR=emacs` and `VISUAL=emacs`
- **AND** they verify existing terminal metadata remains present
