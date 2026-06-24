# Agent Capability Service Is A Host Service API

Alan Kernel should own the semantic model for Agent Capabilities, including
agent actors, context grants, agent runs, result contracts, tasks, permissions,
and audit records. Starting, scheduling, streaming, yielding, and completing
agent runs should be provided through an Agent Capability Service as a Host
Service API so provider, runtime, memory, and execution implementations do not
become Kernel dependencies.
