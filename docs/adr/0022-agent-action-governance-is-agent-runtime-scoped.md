# Agent Action Governance Is Agent Runtime Scoped

Effect classes, risk scoring, approval checkpoints, auto-execution policy, and
execution guard metadata should be scoped to Agent Runtime Service, AgentFS
action files, and Alan Agent compatibility/workspace paths. Kernel should only
expose Paths, Files, stream Files, Descriptors, Access Rights, Credentials, a
single `Process` category (agent-ness is an `/agent` file-layout convention above
the kernel; ADR-0024 D3), Access Checks, namespaces, and mounts. Other Alan
Apps may use Access Checks and host consent services, but they should not be
forced to model ordinary user actions through agent action risk.
