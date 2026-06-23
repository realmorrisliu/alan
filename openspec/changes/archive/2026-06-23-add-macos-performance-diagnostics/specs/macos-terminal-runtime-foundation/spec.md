## ADDED Requirements

### Requirement: Performance diagnostics capture behavior-neutral terminal events
The macOS terminal runtime SHALL support a behavior-neutral performance
diagnostics recorder that captures recent terminal and shell performance events
only while diagnostics are enabled.

#### Scenario: Diagnostics disabled
- **WHEN** performance diagnostics are disabled
- **THEN** terminal and shell probe points do not append diagnostic events
- **AND** terminal scheduling, rendering, focus, publication, and process
  lifecycle behavior are unchanged

#### Scenario: Terminal events recorded
- **WHEN** performance diagnostics are enabled and terminal runtime activity
  occurs
- **THEN** Alan records compact event metadata for Ghostty wakeup, app tick,
  surface refresh, surface attach, catch-up refresh, runtime snapshot
  publication, metadata callbacks, scrollback or renderer updates, render
  priority, and visibility where those events occur
- **AND** the recorder includes event timestamps, categories, durations, counts,
  pane or content correlation IDs, and foreground / visible background / hidden
  background grouping

#### Scenario: Shell projection events recorded
- **WHEN** performance diagnostics are enabled and terminal runtime state is
  projected into shell state
- **THEN** Alan records compact event metadata for runtime projection,
  pane-state publication, selection changes, focus changes, and priority
  synchronization
- **AND** the recorder marks threshold-crossing long events as automatic
  stutter markers

#### Scenario: Process pressure sampled
- **WHEN** performance diagnostics are enabled
- **THEN** Alan samples Alan process CPU, memory, thread count, and known
  terminal child-process aggregate CPU at a bounded low frequency
- **AND** process samples do not include command lines, command arguments,
  working directories, environment variables, or terminal text

### Requirement: Performance diagnostics preserve content privacy
Performance diagnostics SHALL NOT record terminal text, prompts, stdout/stderr
content, command lines, working directories, repository names, file paths,
environment variables, secrets, bearer tokens, API keys, refresh tokens, or raw
provider/auth store values.

#### Scenario: Terminal output is active
- **WHEN** diagnostics are enabled while terminal panes produce output
- **THEN** the diagnostics trace records timing, count, priority, visibility,
  and process metrics only
- **AND** the exported diagnostics bundle does not contain the terminal output
  text

#### Scenario: Process sampling runs
- **WHEN** the process sampler observes Alan or terminal child processes
- **THEN** diagnostics record numeric process metrics and process identifiers
- **AND** diagnostics do not record command-line strings or current working
  directory strings

#### Scenario: Diagnostics bundle is inspected
- **WHEN** a diagnostics bundle is exported
- **THEN** the bundle contains `events.jsonl` and `summary.json`
- **AND** those files omit terminal content, command-line, cwd, path,
  environment, and secret fields

### Requirement: Performance diagnostics are bounded and exportable
The macOS terminal runtime SHALL keep diagnostics bounded in memory and SHALL
export only the currently retained recent diagnostics when requested.

#### Scenario: Long-running diagnostics session
- **WHEN** diagnostics remain enabled for longer than the configured retention
  window
- **THEN** Alan evicts older diagnostic events and summaries according to the
  bounded ring-buffer policy
- **AND** diagnostics memory usage does not grow without bound

#### Scenario: Export recent diagnostics
- **WHEN** the user exports recent diagnostics
- **THEN** Alan writes a local diagnostics bundle containing retained events,
  aggregate summary, app version, install channel, schema version, sampling
  intervals, and capture window metadata
- **AND** exporting diagnostics does not change terminal runtime scheduling,
  rendering, focus, or process lifecycle behavior
