# Agent Memory Kind And Authority Are Separate Axes

Agent memory in Alan OS should not be modeled as User Memory, System Memory, and
App Memory as if those were memory kinds. Working, episodic, semantic, and
procedural describe how an agent uses memory; personal, system-continuity, app,
and workspace Memory Stores describe who owns and authorizes the memory file
tree. Agent Processes receive Memory Stores through descriptors, so Root Agent
Process can have continuity without turning app-private history into a global
agent brain.
