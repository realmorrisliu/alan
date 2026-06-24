# Alan Product Context

This glossary names Alan product concepts that cut across macOS shell, runtime,
and future Alan OS work.

## Language

**Alan Agent**:
The built-in Alan App for agent work. It is a user-facing product that uses an
internal execution engine and acts as the user-visible workspace for supervising
agent work, not the Alan OS, Alan Kernel, or System Agent Supervisor itself.
_Avoid_: OS core, agent backend, Alan Kernel, System Agent Supervisor

**Agent Actor**:
An OS-level actor type for AI-mediated work. Agent actors can be invoked by
Alan Apps through OS capability paths without making any app depend on the
Alan Agent product UI.
_Avoid_: chatbot, app-specific assistant, agent backend

**Agent Capability**:
An OS-provided capability that lets Alan Apps request AI-mediated reading,
planning, transformation, delegation, or action through governed command and
audit paths.
_Avoid_: embedded app chatbot, direct LLM feature, Alan Agent UI dependency

**Agent Capability Service**:
The Host Service API that starts, schedules, streams, yields, and completes
agent runs using provider, runtime, memory, and execution implementations outside
Alan Kernel.
_Avoid_: Alan Kernel LLM runtime, app-local agent engine

**Agent Capability Descriptor**:
A typed descriptor for a kind of agent work, such as explain, summarize, plan,
transform, propose commands, delegate, or remember, layered over the common
agent run substrate.
_Avoid_: one untyped run prompt, app-specific prompt convention

**Agent Run**:
A bounded execution of an agent capability against a specific app, object,
task, context, permission scope, and audit record.
_Avoid_: always-on session, global root conversation, background chatbot

**Agent Run Ownership**:
The ownership rule that an agent run is owned by the app, object, or task that
requested it by default, while the System Agent Supervisor may provide memory and
continuity across runs.
_Avoid_: supervisor-owned by default, app as passive context provider

**Context Grant**:
A typed authorization from an Alan App to an agent run describing the app,
objects, views, selected ranges, task goal, allowed reads, allowed commands,
privacy policy, evidence requirements, and expected result shape for that run.
_Avoid_: prompt dump, raw app state, implicit full-context access

**Command Governance**:
The Alan OS decision layer that evaluates whether an actor may invoke a
command now, using policy, effect classification, execution guard capability,
approval checkpoints, and audit records.
_Avoid_: Alan Agent-only tool policy, sandbox alone, app-private permission check

**Effect Class**:
A semantic classification of a command's effect, such as inspect, draft, modify,
delete, publish, execute, delegate, remember, or cross-app, used by Command
Governance alongside coarse capability classes.
_Avoid_: read/write only, tool capability as full risk model

**Command Risk**:
The governance assessment of whether a command can run automatically, must ask
for approval, or must be denied, based on policy, effect class, target scope,
reversibility, execution guard strength, and auditability.
_Avoid_: write means unsafe, read means safe

**Execution Guard**:
The concrete containment or validation mechanism that constrains a command's
effects, such as an OS sandbox, workspace path guard, app object guard, domain
validator, or human approval gate.
_Avoid_: policy alone, sandbox as the only guard

**Result Contract**:
A typed output shape requested from an agent run, such as an answer, citations,
evidence, proposed commands, draft objects, follow-up questions, uncertainty,
memory updates, or an audit summary.
_Avoid_: plain text response only, app-specific text parsing

**User Memory**:
Long-lived memory about the user's preferences, habits, goals, and working
style that may inform agent runs across apps when permitted.
_Avoid_: app history, raw transcript dump

**System Memory**:
Alan OS memory about cross-app activity, active work, relationships between
tasks, and system-level continuity.
_Avoid_: app-owned private memory, global scrape

**App Memory**:
An Alan App's domain-owned long-lived memory, such as reading history, practice
logs, project notes, or app-specific evidence. It is exposed to agent runs
through app-controlled memory surfaces or context grants.
_Avoid_: automatic supervisor memory, global agent memory

**Agent Execution Engine**:
The internal engine used by Alan Agent to run sessions, tools, skills, policy,
memory, and persistence. It is implementation authority for Alan Agent, not the
OS primitive exposed to every app.
_Avoid_: Alan OS, Alan Kernel, OS agent service

**System Agent Supervisor**:
The always-available Alan OS agent supervisor with long-lived identity, memory,
system awareness, and cross-app continuity. It supervises and starts scoped
agent runs; it is not an ever-growing agent session.
_Avoid_: root agent session, agent kernel, global chat

**Agent Workspace**:
The user-visible workspace, usually Alan Agent, where users inspect, steer, and
organize agent sessions, agent runs, supervisor-raised tasks, memory, evidence,
and cross-app work.
_Avoid_: System Agent Supervisor, invisible agent service, root session

**Agent Capability Migration**:
The migration of existing Alan Agent capabilities into Alan OS by preserving,
adapting, or rewriting them according to the new Alan OS boundaries instead of
discarding the working runtime model.
_Avoid_: greenfield replacement, copying Alan Agent internals into Kernel

**Agent Capability Migration Classes**:
The classification rule for existing Alan Agent capabilities: each capability
must become an OS Primitive, a Host Service Capability, an Alan Agent App
Feature, compatibility-only behavior, or a rewrite candidate.
_Avoid_: unclassified migration, all capabilities become Kernel

**Supervisor Authority**:
The authority model for the System Agent Supervisor: broad system awareness and
suggestion power, with app-private reads and side effects mediated through
permission, command, and audit paths.
_Avoid_: root automation permission, unrestricted agent access

**Primary Shell Window**:
The single main Alan shell window used by the macOS app. Short-term product
work assumes there is only one shell window, and summon behavior targets this
window.
_Avoid_: recent shell window, per-Space shell window, Quick Terminal window

**Primary Window Summon**:
The user action that brings Alan's primary shell window to the user's current
macOS Space and display. It targets the main Alan window, not a detached
terminal panel or separate terminal runtime, and it preserves the current Alan
workspace Space, Tab, and Pane selection. Alan comes to the user's current
desktop context rather than moving the user to Alan's previous desktop context.
It replaces the former Quick Terminal shortcut without keeping Quick Terminal
compatibility aliases. It is an app/window command, not a shell workspace
action.
_Avoid_: Quick Terminal summon, Peak summon, global terminal toggle, quick-terminal alias
