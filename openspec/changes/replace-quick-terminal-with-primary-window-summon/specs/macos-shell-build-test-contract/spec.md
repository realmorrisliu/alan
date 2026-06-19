## ADDED Requirements

### Requirement: Primary Window Summon And Legacy Cleanup Are Verified
The Apple client SHALL include focused verification for Primary Window Summon,
Quick Terminal removal, and legacy quick-terminal manifest cleanup.

#### Scenario: Primary window summon is verified
- **WHEN** app/window command routing changes
- **THEN** focused checks verify that the former Quick Terminal shortcut invokes
  Primary Window Summon, opens or reopens the single primary shell window,
  activates the app, preserves selected shell Space/Tab/PaneSlot, and focuses the
  selected terminal when the selected content is terminal

#### Scenario: Current Space behavior is verified
- **WHEN** Primary Window Summon is implemented
- **THEN** running-app smoke or documented manual verification covers invoking
  the command from another macOS Space/display and confirms the best-effort
  movement or activation fallback

#### Scenario: Quick Terminal surfaces are absent
- **WHEN** shell contract checks inspect active macOS app source, menus,
  keybindings, control commands, action descriptors, App Intents, FFI adapters,
  and Rust shell-core action/model/reducer surfaces
- **THEN** the checks fail if they find active Quick Terminal actions, Peak
  presenter/window/view ownership, quick-terminal control aliases, promotion
  UI, or quick-terminal runtime-slot creation

#### Scenario: Legacy manifest cleanup is verified
- **WHEN** tests load manifests containing visible or hidden `quick_terminal`
  records
- **THEN** tests verify Alan discards the records, creates no quick-terminal
  runtime or panel, restores normal workspace state, and writes the next
  manifest without `quick_terminal`

## MODIFIED Requirements

### Requirement: Safe terminal close and transcript restore are verified
The Apple client SHALL include focused automated tests and running-app smoke
evidence for terminal close guarding, bounded transcript snapshot persistence,
and app-restart transcript restore.

#### Scenario: Active close guard tested
- **WHEN** tests request pane, tab, window, or app close for terminal content with active work
- **THEN** tests verify that close requires confirmation and does not mutate shell state or finalize runtimes before confirmation

#### Scenario: Idle close bypass tested
- **WHEN** tests request close for idle shell, exited terminal, or non-terminal content
- **THEN** tests verify that the app does not require active-work confirmation solely because a shell process exists

#### Scenario: Manifest transcript round trip tested
- **WHEN** tests persist a workspace manifest containing terminal transcript snapshots
- **THEN** tests verify old manifests without snapshots still decode
- **AND** new manifests preserve bounded transcript lines, dimensions, cwd, title, focus, truncation metadata, and content identity through a round trip

#### Scenario: Restart transcript restore smoke tested
- **WHEN** a running-app smoke produces visible terminal output, closes or quits Alan through a confirmed path, and relaunches the freshly installed app
- **THEN** verification confirms the restored terminal shows the prior output without an extra restored-session banner
- **AND** the restored terminal accepts new input in a newly started shell at the restored cwd

## REMOVED Requirements

### Requirement: Quick Terminal boundary refactor has staged verification
**Reason**: Quick Terminal Peak and boundary refactor work are removed. The
verification target is now Primary Window Summon plus legacy cleanup.

**Migration**: Replace Quick Terminal boundary tests with primary-window summon,
removed-surface, and legacy manifest discard tests.

#### Scenario: First implementation slice is verified
- **WHEN** the Quick Terminal boundary refactor checks would have run
- **THEN** Alan instead verifies that no Quick Terminal Peak setup remains on
  stable launch paths

#### Scenario: Full behavior verification follows
- **WHEN** behavior verification is performed
- **THEN** tests cover Primary Window Summon and Quick Terminal removal rather
  than Quick Terminal show, hide, close, attach, focus, or promotion
