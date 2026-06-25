# Agent Runtime Service Is A File-Server Service

Agent Runtime Service should be a Plan 9-style file-server Process managed by
Service Manager. It posts a service handle under `/srv`, serves AgentFS at
`/agent`, and executes Agent Processes. Starting, inspecting, steering,
scheduling, streaming, yielding, and completing agent work should be expressed
through AgentFS files, descriptors, and process state. The current HTTP/WS
session server remains compatibility transport while clients migrate; it is not
the target OS boundary.
