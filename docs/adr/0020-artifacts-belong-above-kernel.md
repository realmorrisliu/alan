# Artifacts Belong Above Kernel

Alan Kernel should not model Artifact as a primitive. Kernel provides output
Files created or written by Processes, stream Files emitted by Processes,
service-owned stream Files, and native selectors; Alan Agent, Agent Runtime
Service, or domain apps may interpret those outputs as artifacts for
presentation, sharing, review, compatibility, or evidence.
