## ADDED Requirements

### Requirement: Control plane and App Intents share command semantics
The macOS shell control plane SHALL use the same shell command result categories
as App Intents and native command routing for create, split, focus, close, send
text, read summary, and attention activation actions.

#### Scenario: Intent and control command create split
- **WHEN** an App Intent and a control-plane command create the same directional split from equivalent starting state
- **THEN** both paths report aligned success semantics and produce equivalent shell state

#### Scenario: Intent and control command miss target
- **WHEN** both paths target a missing pane
- **THEN** both paths report an aligned missing-target category without mutating shell state

### Requirement: Automation command events are observable
Shell mutations initiated by App Intents SHALL publish the same kind of shell
events as equivalent menu, keyboard, command UI, or control-plane mutations.

#### Scenario: Intent focuses pane
- **WHEN** an App Intent focuses a pane
- **THEN** the shell event stream records the focus change with previous and new pane identity

#### Scenario: Intent sends text
- **WHEN** an App Intent sends text to a pane
- **THEN** the shell event stream records delivery status without logging sensitive text content

### Requirement: Control fixtures cover failure cases
The control-plane test fixtures SHALL cover missing targets, malformed requests,
runtime unavailable, timeout, permission/privacy restrictions, and IO diagnostic
paths that App Intents may also encounter.

#### Scenario: Runtime unavailable fixture
- **WHEN** a fixture command targets a pane whose fake runtime is unavailable
- **THEN** the test asserts stable runtime-unavailable semantics shared by control and intent paths

#### Scenario: Privacy restriction fixture
- **WHEN** a fixture command tries to read sensitive terminal content
- **THEN** the test asserts that only safe metadata is returned
