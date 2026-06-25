# Agent Context And Results Use Files And Descriptors

Agent context and results should be passed through files, directories,
descriptors, stream files, and app/service-owned file trees rather than through
RPC request/response contracts. Kernel provides paths, descriptors, access
rights, credentials, namespaces, mounts, Processes, and Agent Processes; Agent
Runtime Service and apps define the concrete request, action, context, result,
evidence, and audit files above that substrate.
