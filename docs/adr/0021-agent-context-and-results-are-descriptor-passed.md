# Agent Context And Results Are Descriptor-Passed

Agent context and results should be descriptor-passed rather than API-contract
passed. An app, shell, user, or parent Agent Process opens bounded files,
directories, streams, Memory Stores, Skills, policy files, or app service
trees, then spawns an Agent Executable with those descriptors. Agent Runtime
Service projects the running work through AgentFS request, action, io, and
machine files; results are conveyed via `io/output` and per-action
`actions/<id>/result`, not a top-level `result` file. Kernel remains responsible
only for files, descriptors, access rights, credentials, namespaces, mounts, and
a single `Process` identity.
