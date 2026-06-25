# Agent Action Auto-Run Depends On Governability

Agent Runtime Service should decide automatic execution of agent-proposed or
autonomous actions from governability, not from a coarse read/write split alone.
An Agent Process action may run automatically only when policy, effect class,
target scope, reversibility, execution guard strength, and auditability support
it; high-risk effects such as delete, publish, irreversible modify, privilege
escalation, cross-app writes, and opaque shell/process work without strong
confinement must require approval or denial.
