# Agent Processes Are Request-Owned By Default

Agent Processes should be owned by the app, shell, user, or parent Agent
Process that spawned them. Root Agent Process provides continuity for the agent
process tree, but it does not own every AI-mediated activity in Alan OS. This
keeps UPDF reading assistance, Groove Master practice help, Alan Shell agent
commands, and Alan Agent workspace tasks inside their product or process
semantics instead of making every agent action a root-owned global task.
