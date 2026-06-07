## ADDED Requirements

### Requirement: Alan Emacs Is Installed From Alan-Owned Distribution State
Alan SHALL provide a restrained `alan emacs` command group that manages the
Alan-owned Emacs distribution install state without wrapping system service
managers or shadowing the `emacs` executable.

The command surface SHALL be limited to `status`, `install`, `doctor`, and
`uninstall` unless a future change expands the distribution-management
contract.

#### Scenario: Commands are listed
- **WHEN** a user inspects `alan emacs --help`
- **THEN** the command group exposes `status`, `install`, `doctor`, and
  `uninstall`
- **AND** it does not expose Homebrew service, launchctl, systemctl, daemon
  restart, or editor-launch sugar commands

#### Scenario: Distribution is installed
- **WHEN** the user runs `alan emacs install`
- **THEN** Alan materializes the bundled Alan Emacs distribution into an
  Alan-managed user data location
- **AND** the user's selected Emacs config entry points to the Alan-managed
  installed copy rather than to the source checkout

#### Scenario: Bundled distribution is found through command link
- **WHEN** the installed CLI is invoked through a PATH-visible symlink
- **THEN** Alan resolves the executable target before falling back to
  development source discovery
- **AND** it can locate the bundled `Contents/Resources/alan-emacs` distribution

#### Scenario: Status reports ownership
- **WHEN** the user runs `alan emacs status`
- **THEN** Alan reports whether Alan Emacs is installed
- **AND** it reports the selected config entry and whether it is Alan-owned
- **AND** it reports the installed distribution location when present

### Requirement: Config Entry Selection Is Detector Driven
Alan SHALL select exactly one Emacs config entry through programmatic detection
instead of hard-coding `~/.emacs.d` or `$XDG_CONFIG_HOME/emacs`.

Candidate entries SHALL include `~/.emacs.d` and `$XDG_CONFIG_HOME/emacs`, using
`~/.config/emacs` when `XDG_CONFIG_HOME` is unset.

Alan SHALL treat legacy startup files `~/.emacs.el` and `~/.emacs` as
non-Alan-owned conflicts because they take precedence over directory init files
in ordinary Emacs startup.

#### Scenario: Existing Alan-owned entry is reused
- **WHEN** one candidate config entry already points to Alan-managed Emacs
  distribution state
- **THEN** `alan emacs install` reuses that entry
- **AND** it does not migrate to another entry solely because that other entry is
  also a possible Emacs default

#### Scenario: Empty candidate is selected
- **WHEN** exactly one candidate entry exists and is empty
- **AND** no candidate contains non-Alan-owned user configuration
- **THEN** `alan emacs install` selects the empty candidate for installation

#### Scenario: Missing candidates use Emacs probe
- **WHEN** no candidate entry determines the choice
- **THEN** `alan emacs install` probes the installed `emacs` default user config
  directory
- **AND** it selects the probed entry when it corresponds to a supported
  candidate path

#### Scenario: User-owned config is not overwritten
- **WHEN** a candidate config entry contains non-Alan-owned user configuration
- **THEN** `alan emacs install` does not overwrite it
- **AND** it reports the conflicting path and stops safely

#### Scenario: Legacy startup file shadows config directory
- **WHEN** `~/.emacs.el` or `~/.emacs` exists
- **THEN** `alan emacs install` does not report success
- **AND** it reports the startup file as a conflict before linking or replacing
  the selected config directory

### Requirement: Bare Emacs Loads Alan Emacs After Install
After successful installation, invoking ordinary `emacs` SHALL load the
Alan-managed Emacs distribution without requiring wrapper commands, PATH
shadowing, or `--init-directory` arguments.

#### Scenario: Install verifies bare Emacs
- **WHEN** `alan emacs install` completes
- **THEN** it verifies through ordinary Emacs startup discovery that bare
  `emacs` loads Alan Emacs from the installed distribution

#### Scenario: Doctor checks daemon without controlling it
- **WHEN** `alan emacs doctor` detects an Emacs daemon or `emacsclient` endpoint
- **THEN** it may report whether the daemon appears to load Alan Emacs
- **AND** it does not start, stop, restart, or unload Homebrew, launchctl,
  systemctl, or Emacs daemon services

### Requirement: Uninstall Removes Only Alan-Owned State
`alan emacs uninstall` SHALL remove Alan-owned Emacs distribution links and
installed data without deleting or mutating non-Alan-owned user Emacs
configuration.

#### Scenario: Alan-owned install is removed
- **WHEN** the selected config entry points to Alan-managed Emacs distribution
  state
- **THEN** `alan emacs uninstall` may remove that config entry link and the
  Alan-managed installed copy

#### Scenario: User config is preserved
- **WHEN** the selected config entry is not Alan-owned
- **THEN** `alan emacs uninstall` refuses to remove it
- **AND** it reports that the path is outside Alan ownership
