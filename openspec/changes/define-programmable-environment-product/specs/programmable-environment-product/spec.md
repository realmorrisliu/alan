## ADDED Requirements

### Requirement: Programmable environment is the repo-level product constitution
The programmable environment SHALL be the long-term organizing product model for
the alan repository, including Alan agent, Alan for macOS, environment apps such
as Groove Master, host surfaces, adapters, and future products.

It SHALL NOT be treated merely as a feature of the existing Alan macOS shell,
daemon, terminal runtime, or agent session UI. It also SHALL NOT be treated as
an unrelated side product whose direction does not apply to the rest of the
repo.

#### Scenario: Future work scopes implementation
- **WHEN** a future OpenSpec change implements product, runtime, agent, host,
  app, adapter, extension, or surface behavior
- **THEN** the change identifies how the scope relates to the programmable
  environment model, or explicitly marks itself as legacy/compatibility work
- **AND** it does not assume that existing Alan shell containers, terminal
  workflows, or agent session UI are the required implementation boundary

### Requirement: Product identity is a programmable personal computing environment
The product SHALL organize personal computing activity through a unified runtime
of objects, commands, buffers, views, queries, humans, agents, and extensions.

#### Scenario: Product is described
- **WHEN** product, architecture, or implementation docs describe this product
- **THEN** they describe it as a programmable personal computing environment
- **AND** they do not reduce the product identity to an editor, IDE, AI chat
  app, app launcher, terminal UI, or operating system shell

### Requirement: Environment roles are explicit
Future and aligned existing specs SHALL identify their role in the programmable
environment family as one or more of:

- environment core;
- agent runtime or agent actor;
- host surface;
- environment app;
- adapter;
- legacy or compatibility surface.

#### Scenario: A runtime substrate is proposed
- **WHEN** a future change proposes a runtime substrate such as workbench core
- **THEN** it identifies whether it owns environment-core abstractions,
  projections, ledgers, command/query surfaces, or extension capabilities
- **AND** it does not claim to be the whole programmable environment product

#### Scenario: A host-surface change is proposed
- **WHEN** a future change modifies Alan for macOS, the Rust TUI, or another host
- **THEN** it identifies host-owned concerns such as layout, windowing, native
  chrome, renderer state, input translation, and platform-specific presentation
- **AND** it does not redefine environment objects, buffers, commands, queries,
  or app-domain truth as host-owned state

#### Scenario: An environment app is proposed
- **WHEN** a future change proposes a domain product such as Groove Master
- **THEN** it states the real user-facing product boundary first
- **AND** it identifies the environment adapter that maps domain objects,
  commands, buffers, views, queries, and agent participation into the shared
  environment

### Requirement: Existing specs align through environment metadata
Selected active and future OpenSpec changes SHALL include enough environment
alignment metadata for reviewers to understand how the change fits the
constitution.

At minimum, aligned specs SHOULD state:

- environment role;
- runtime abstraction mapping for objects, commands, buffers, views, queries,
  actors, or why those abstractions do not apply;
- native source-of-truth boundary;
- host/rendering/layout boundary where relevant;
- deferred migration or compatibility boundary.

#### Scenario: Existing macOS shell component work is reviewed
- **WHEN** `add-macos-shell-component-system` or equivalent host presentation
  work is aligned with this constitution
- **THEN** it is classified as a macOS host surface/design-system capability
- **AND** it may own presentational primitives, tokens, preview galleries, and
  host accessibility rules
- **AND** it does not claim ownership of environment-core runtime abstractions

#### Scenario: Existing Alan agent work is reviewed
- **WHEN** Alan agent runtime, session, tool, plan, memory, child-run, or
  governance work is aligned with this constitution
- **THEN** it is classified as agent runtime/actor work or adapter work
- **AND** it identifies how agent actions project into objects, commands,
  buffers, views, queries, tasks, artifacts, evidence, permissions, or audit
  surfaces where applicable

### Requirement: Product is local-first and filesystem-first
The product SHALL treat local-first operation, filesystem-backed inspection, and
UNIX-like composition as core product principles.

#### Scenario: Product stores or represents user work
- **WHEN** the product stores state, discovers work, creates artifacts, or
  represents resources
- **THEN** it prefers ordinary files, directories, manifests, sidecars, caches,
  imports, exports, mounts, or projections where those boundaries are practical
- **AND** it does not require a proprietary object store or universal URI layer
  as the first product principle

### Requirement: Data ownership and activity organization are separate
The product SHALL allow source data to remain in filesystems, projects,
repositories, external services, databases, or OS resources while organizing
user activity through runtime objects and commands.

#### Scenario: External or existing data is used
- **WHEN** a file, project, terminal, Git state, note, task, mail item, calendar
  item, recording, loop, or remote record is brought into the environment
- **THEN** the product can represent it as a runtime object without requiring
  the environment to become the source of truth for the underlying data

### Requirement: Core runtime abstractions are stable
The product SHALL use Object, Command, Buffer, View, and Query as stable
product-level runtime abstractions.

#### Scenario: Runtime concepts are introduced
- **WHEN** a future spec introduces product runtime behavior
- **THEN** it states how the behavior relates to Object, Command, Buffer, View,
  Query, Actor, or explicitly explains why it sits outside those abstractions

### Requirement: Objects are inspectable runtime resources
Objects SHALL have stable identity, metadata, relationships, capabilities, and
runtime inspectability.

#### Scenario: Resource becomes an object
- **WHEN** the environment represents a resource as an object
- **THEN** users and agents can inspect its identity, metadata, relationships,
  and available capabilities through runtime surfaces

### Requirement: Commands are first-class actions
Commands SHALL be first-class runtime entities with stable names, descriptions,
arguments, target types, permission or risk metadata, undo or recovery
semantics when applicable, categories, and invocation hints.

#### Scenario: Action is exposed
- **WHEN** human UI, modal grammar, command palette, automation, extension, or
  agent behavior exposes an action
- **THEN** it routes through a command entry rather than creating a separate
  hidden mutation path

### Requirement: Buffers are work units
Buffers SHALL be the primary units for active work and SHALL NOT be limited to
text.

#### Scenario: Non-text work is opened
- **WHEN** terminal output, Git diff, search results, task lists, agent plans,
  recordings, loop libraries, database results, or calendar ranges are opened
  for work
- **THEN** the product can represent them as buffers with lifecycle, history,
  dirty state, and restoration semantics appropriate to their kind

### Requirement: Views are independent from data
Views SHALL define presentation and navigation independently from the underlying
object or buffer data.

#### Scenario: Object has multiple presentations
- **WHEN** an object or buffer supports multiple useful presentations
- **THEN** the product can expose multiple views without duplicating the source
  data or changing data ownership

### Requirement: Runtime is queryable
The runtime SHALL provide semantic query and introspection surfaces over
objects, buffers, commands, tasks, symbols, relationships, and other registered
runtime entities.

#### Scenario: User or agent searches semantically
- **WHEN** a user or agent asks for runtime state such as modified buffers,
  Git commands, pending tasks, marked recordings, available loops, or function
  symbols
- **THEN** the product provides queryable runtime surfaces rather than forcing
  navigation only through app, window, or file hierarchy

### Requirement: Out-of-the-box use is mandatory
The product SHALL be useful on first launch without requiring the user to first
configure a programmable substrate.

#### Scenario: User opens product for the first time
- **WHEN** a user first opens the product
- **THEN** the product can reveal, organize, or create real personal work from
  local files, projects, workspaces, notes, tasks, terminals, domain app data,
  or similar personal computing objects

### Requirement: Modal grammar is a power layer over ordinary UI
The product SHALL support ordinary UI for onboarding and a modal command grammar
for power use, with both layers routing through the same command model.

#### Scenario: Command has multiple invocations
- **WHEN** a command can be invoked through UI controls, command palette,
  shortcut, modal grammar, automation, or agent action
- **THEN** those invocations preserve the same command identity, target
  semantics, permission checks, and audit behavior

### Requirement: Agents are first-class actors
Agents SHALL be first-class runtime actors alongside humans and SHALL operate
through the same object, command, buffer, view, query, permission, and audit
abstractions.

#### Scenario: Agent performs work
- **WHEN** an agent reads context, creates a buffer, executes a command,
  proposes a patch, opens a view, performs a query, curates a journal entry, or
  updates a plan
- **THEN** the action is mediated by runtime abstractions
- **AND** the agent does not bypass command permissions or audit surfaces to
  mutate underlying systems directly

### Requirement: Environment apps are real products
Environment apps SHALL be treated as user-facing domain products inside the
environment, not as demos whose primary purpose is proving the platform.

#### Scenario: App product is proposed
- **WHEN** a future change proposes or updates an environment app
- **THEN** it defines the app's user-facing job, domain objects, domain commands,
  and product experience before describing platform proof value
- **AND** its domain core remains separable from the current Alan host
  implementation

### Requirement: Extensibility uses Rust core plus WASM components
The product SHALL treat a Rust core plus WASM Component Model extensions and WIT
interfaces as the product's extension direction.

#### Scenario: Extension capability is added
- **WHEN** a future change adds extension behavior
- **THEN** it uses explicit capability grants for access such as filesystem
  read/write, network access, command execution, buffer access, view
  registration, query registration, or agent registration

### Requirement: Incubation validates product assumptions
Future incubation work SHALL validate the product constitution before expanding
into a broad environment implementation.

#### Scenario: First incubation scope is proposed
- **WHEN** a future change proposes the first prototype, runtime substrate, or
  MVP for this product
- **THEN** it identifies the concrete workflow or substrate boundary that proves
  local-first discovery, object representation, buffer/view use, command
  execution, query or introspection, agent participation, host rendering, and
  extension-shaped boundaries
- **AND** it states which constitution criteria are proven by the slice and
  which are intentionally deferred
