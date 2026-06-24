# Agent Memory Is Layered By Owner

Agent memory in Alan OS should be layered into User Memory, System Memory, and
App Memory. This gives the System Agent Supervisor useful cross-app continuity
without turning every app's private history into global agent memory by default.
