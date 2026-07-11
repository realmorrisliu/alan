# macos-terminal-activity-semantics Specification

## Purpose

Define how Alan normalizes terminal progress, command completion, bell
attention, Alan CLI process activity, and supported coding-agent lifecycle state
into pane-scoped activity that can drive terminal-first UI, accessibility, and
low-noise notifications.
## Requirements
### Requirement: Terminal Activity Is Normalized

Alan SHALL normalize terminal progress, foreground command state, command completion, bell attention, and supported CLI coding-agent process state into a pane-scoped terminal activity model before projecting that state into UI, accessibility, persistence, or control surfaces.

#### Scenario: Ghostty progress is normalized

- **WHEN** a terminal pane receives a Ghostty progress report
- **THEN** Alan records a pane-scoped activity state with progress kind, optional percentage, freshness timestamp, source kind, and attention priority

#### Scenario: Command completion is normalized

- **WHEN** a terminal pane receives a command-finished event with exit status or duration metadata
- **THEN** Alan records a command completion activity with success or failure status, last command metadata when available, and a bounded freshness window

#### Scenario: Agent state is normalized

- **WHEN** a supported CLI coding agent emits a reliable running, blocked, complete, or error signal
- **THEN** Alan records agent activity using the same pane-scoped activity model instead of introducing a separate sidebar or notification state path

### Requirement: Activity Priority And Freshness Are Deterministic
Alan SHALL compute the visible activity state for a pane from source priority,
attention severity, freshness, focus, and user actionability so stale or
low-priority signals do not mask newer actionable signals.

#### Scenario: Progress becomes stale
- **WHEN** a progress-producing command stops updating without clearing its
  progress state
- **THEN** Alan marks or clears the progress activity according to a bounded
  freshness policy instead of showing permanent in-progress UI

#### Scenario: Agent needs input while command progress exists
- **WHEN** a pane has both command progress and a supported agent signal that
  requires user input
- **THEN** Alan surfaces the agent input-required state as the primary activity
  while retaining progress metadata only as secondary context

#### Scenario: Successful command completes in focused pane
- **WHEN** a focused pane reports successful command completion
- **THEN** Alan may update transient metadata but MUST NOT produce disruptive
  attention UI or a system notification by default

#### Scenario: Sidebar priority is computed
- **WHEN** a tab has input-required, failed, paused, progress, running, bell,
  exited, and idle activity candidates across its panes
- **THEN** Alan chooses sidebar activity in that priority order and falls back
  to worktree and branch context when no sidebar-worthy activity exists

#### Scenario: Progress stops updating
- **WHEN** a progress activity has not updated or cleared within the bounded
  freshness window
- **THEN** Alan marks it stale or removes it from sidebar-worthy activity

#### Scenario: Command failure is acknowledged
- **WHEN** a command failure is displayed in the sidebar and the user focuses
  the owning tab
- **THEN** Alan may demote the command failure from sidebar-worthy activity so
  the tab can return to context display

### Requirement: CLI Coding-Agent Status Is Ingested Conservatively

Alan SHALL ingest CLI coding-agent lifecycle state from reliable structured signals, documented notification hooks, or explicit terminal integration adapters, and SHALL fall back to generic terminal activity when no reliable signal is available.

#### Scenario: Structured agent event arrives

- **WHEN** a supported agent event identifies the agent kind, process or invocation identity, cwd or project, and lifecycle transition
- **THEN** Alan maps it to pane-scoped agent activity
- **AND** default UI surfaces retain only user-facing safe metadata

#### Scenario: Agent support is partial

- **WHEN** Alan can detect a likely coding-agent process but cannot determine whether it is running, blocked, complete, or errored
- **THEN** Alan surfaces generic foreground-command or unknown-agent activity rather than claiming a precise lifecycle state

#### Scenario: Agent event is unsafe or malformed

- **WHEN** an agent integration emits malformed, untrusted, or overly detailed status payloads
- **THEN** Alan drops or sanitizes the payload for default UI
- **AND** raw diagnostics remain available only through explicit debug surfaces

#### Scenario: Initial Codex adapter

- **WHEN** Codex emits a reliable notification or structured lifecycle signal
- **THEN** Alan maps it into the same pane-scoped activity model used by terminal, command, and progress sources

### Requirement: Activity Projects To Lightweight Terminal UI
Alan SHALL surface terminal activity through compact pane title-bar
accessories, sidebar tab metadata, accessibility values, and optional
notifications while preserving terminal-first layout and input ownership.

#### Scenario: Background agent needs input
- **WHEN** a background pane's supported coding agent needs user input
- **THEN** Alan marks the owning tab and pane with lightweight status metadata
  and may issue a notification without stealing terminal focus

#### Scenario: Progress is active in visible pane
- **WHEN** a visible pane reports determinate or indeterminate progress
- **THEN** Alan presents progress as a compact terminal accessory or thin
  pane-local indicator without adding a persistent bottom strip or resizing the
  terminal canvas

#### Scenario: Activity clears
- **WHEN** the primary activity reaches a terminal state or expires
- **THEN** Alan removes or demotes the visible activity indicator without
  changing pane, split, sidebar, or toolbar geometry

### Requirement: Sidebar Activity Is Tab Level
Alan SHALL compute sidebar tab activity from the highest-priority
sidebar-worthy activity across every pane in the tab, while keeping the tab
title derived from the focused or primary pane for the initial implementation.

#### Scenario: Background pane needs input
- **WHEN** a non-focused pane in a split tab reports a supported coding-agent
  input-required state
- **THEN** the sidebar tab row surfaces that input-required activity as the tab
  activity and includes a short pane hint

#### Scenario: Focused pane is idle and background pane has progress
- **WHEN** the focused pane has no sidebar-worthy activity and another pane in
  the same tab owns active progress
- **THEN** the sidebar tab row surfaces the progress activity and progress rail
  for the tab

#### Scenario: No sidebar-worthy activity exists
- **WHEN** every pane in a tab is idle or only has successful completion state
- **THEN** the sidebar tab row falls back to worktree and branch context instead
  of showing idle or success as activity

### Requirement: Sidebar Activity Copy Is Source First
Alan SHALL render sidebar activity copy with source-first labels and SHALL avoid
guessing source labels from arbitrary terminal output.

#### Scenario: Reliable source label exists
- **WHEN** a tab activity has a reliable source label such as a supported agent,
  command boundary label, or explicit task label
- **THEN** the sidebar activity line renders source before state, such as
  `Codex · Input needed` or `Build · 42%`

#### Scenario: Progress label is unavailable
- **WHEN** a progress activity has no reliable source label
- **THEN** the sidebar activity line uses `Progress` as the source label rather
  than parsing terminal output or cwd text

#### Scenario: Activity comes from another pane
- **WHEN** sidebar tab activity belongs to a pane that is not the focused or
  primary pane
- **THEN** the sidebar activity line includes a short pane hint before the
  source-first activity copy

#### Scenario: Success and idle are not sidebar activity
- **WHEN** a pane reports successful command completion, idle shell state, title
  update, or working-directory update
- **THEN** Alan does not render that state as sidebar activity

### Requirement: Sidebar Progress Rail Belongs To Displayed Activity
Alan SHALL render a sidebar progress rail only when the sidebar activity being
displayed owns progress.

#### Scenario: Displayed activity owns progress
- **WHEN** the highest-priority sidebar activity is determinate progress
- **THEN** the sidebar row may render a compact progress rail using that
  activity's progress value

#### Scenario: Different pane owns progress
- **WHEN** the displayed sidebar activity is input-required, failed, paused, or
  another non-progress state and a different pane has active progress
- **THEN** the sidebar row does not render the different pane's progress rail
  alongside the displayed activity

### Requirement: Pane Title Activity Is Pane Local
Alan SHALL project pane title-bar activity from the pane's own normalized
activity and SHALL allow pane title bars to show more pane-local context than
sidebar tab rows.

#### Scenario: Pane has local progress
- **WHEN** a visible pane owns progress activity
- **THEN** the pane title bar may show pane-local progress detail even when the
  sidebar tab row is showing a higher-priority activity from another pane

#### Scenario: Pane activity repeats agent marker
- **WHEN** pane-local activity already identifies a supported agent or Alan state
- **THEN** the pane title bar avoids repeating a separate marker that conveys
  the same agent identity

#### Scenario: Title already contains context
- **WHEN** the terminal title already provides the same cwd or worktree context
  that a pane title accessory would show
- **THEN** Alan may suppress the redundant context accessory while preserving
  branch, process, or activity accessories that add distinct information

### Requirement: Semantic Command Boundaries Are Pane Scoped
Alan SHALL model shell prompt marks, command start, command end, command text,
output range, exit status, and timestamps as semantic metadata owned by the
terminal pane when those signals are available.

#### Scenario: Shell integration reports command boundary
- **WHEN** shell integration or terminal protocol events identify a command
  start and command end
- **THEN** Alan records the command boundary and output range for the owning
  pane without changing terminal rendering or scrollback ownership

#### Scenario: Semantic data is unavailable
- **WHEN** prompt marks or command boundaries are unavailable because of shell,
  tmux, SSH, or application mode limitations
- **THEN** Alan keeps normal terminal behavior and disables semantic-only
  actions instead of guessing output ranges from screen text

### Requirement: Semantic Terminal Actions Are Focused And Reversible
Alan SHALL expose prompt navigation, copy-last-command-output, and
command-output search actions through pane-scoped native action routes that do
not mutate terminal process state.

#### Scenario: User jumps between prompts
- **WHEN** semantic prompt marks exist for the focused pane and the user invokes
  previous or next prompt
- **THEN** Alan scrolls the pane viewport to the target prompt without changing
  shell focus, split layout, or command history

#### Scenario: User copies last command output
- **WHEN** a focused pane has a known last command output range and the user
  invokes copy last output
- **THEN** Alan copies that output to the system pasteboard without sending text
  into the terminal application

#### Scenario: User searches command output
- **WHEN** a focused pane has command output ranges and the user invokes
  command-output search
- **THEN** Alan scopes the interaction to that pane and returns focus to the
  terminal after dismissal

#### Scenario: Command-aware actions use native pane routes
- **WHEN** a focused pane has reliable command boundary metadata
- **THEN** Alan exposes command-aware prompt navigation, copy-last-output, and
  search-last-output actions through the shell action registry using
  pane-scoped menu, context menu, keyboard shortcut, or compact terminal tool
  routes without adding persistent terminal chrome or relying on the removed
  `Go to or Command...` command input

#### Scenario: Command boundaries are unavailable
- **WHEN** a focused pane has no reliable command boundary metadata
- **THEN** Alan falls back to ordinary scrollback search, selection copy, and
  normal scrollback navigation rather than guessing command ranges from screen
  text

#### Scenario: No command browser in MVP
- **WHEN** semantic terminal actions are available
- **THEN** Alan does not render a command browser, visible command blocks, or
  persistent command-output segmentation for the MVP
