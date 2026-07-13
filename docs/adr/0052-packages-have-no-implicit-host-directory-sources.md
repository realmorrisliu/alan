# Packages have no implicit Host directory sources

Status: accepted

Package Service and its install-channel System Store are the only package
authority. Alan OS does not scan workspace, AgentRoot, `~/.agents`, or other
Host directories as implicit local-source providers and preserves no overlay
precedence for them. Host content must first enter through an explicit Host
Mount and then be installed by an Alan OS command; resolved packages project at
`/lib/pkg`, while Agent Definitions and Skills are passed by descriptor. The
existing package-management change remains blocked until rewritten after the
workspace-runtime, system Host, and Service Manager changes.
