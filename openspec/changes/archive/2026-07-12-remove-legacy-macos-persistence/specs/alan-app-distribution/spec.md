## ADDED Requirements

### Requirement: Current installers manage only current channel artifacts
Alan install, uninstall, update, and command-line-link repair flows SHALL know
only the canonical bundle and link identities for the selected install channel.
Normal installer behavior SHALL NOT discover, stop, remove, replace, or use the
retired lowercase `alan.app` bundle.

#### Scenario: Stable install runs
- **WHEN** the stable local installer installs or updates Alan
- **THEN** it manages the channel-owned `Alan.app` bundle and canonical CLI link
- **AND** it does not inspect or delete a sibling lowercase `alan.app`

#### Scenario: Dev install runs
- **WHEN** the dev local installer installs or uninstalls Alan Dev
- **THEN** it manages only `Alan Dev.app` and the `alan-dev` link
- **AND** it does not inspect or delete `Alan.app` or lowercase `alan.app`

#### Scenario: CLI link targets a retired bundle
- **WHEN** direct-install link inspection finds a link whose destination is
  lowercase `alan.app`
- **THEN** current install logic does not treat that destination as an Alan-owned
  canonical bundle eligible for automatic replacement
- **AND** the normal installer leaves the non-canonical destination untouched

#### Scenario: Obsolete bundle path is reintroduced
- **WHEN** current installer source or tests add normal-flow handling for
  lowercase `alan.app`
- **THEN** repository verification fails outside immutable archive history and
  the bounded cleanup record
