# Evidence Belongs Above Kernel

Alan Kernel should not model Evidence or ProvenanceRef as a primitive. Kernel
provides paths, Files, Descriptors, Access Rights, Credentials, Process Table
entries, stream offsets, service-owned stream Files, and native selectors; Alan
Agent, Agent Runtime Service, or domain apps may interpret those primitives as
evidence when supporting a claim, result, memory, command proposal, action, or
decision.
