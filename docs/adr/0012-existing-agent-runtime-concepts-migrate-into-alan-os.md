# Existing Agent Runtime Concepts Migrate Into Alan OS

Existing Alan Agent runtime concepts should be migrated into Alan OS where they
fit the new file/process model, not discarded in favor of a greenfield agent
system. Tape, turn execution, policy, sandbox ideas, memory, skills, child
agent orchestration, rollout persistence, and session compatibility should be
preserved or adapted when they map cleanly to Agent Runtime Service, AgentFS,
Agent Processes, Tools, Skills, descriptors, Memory Stores, or compatibility
transport. Concepts that conflict with Process/Agent Process, Service Manager,
file-server services, or descriptor-passed context should be reshaped.
