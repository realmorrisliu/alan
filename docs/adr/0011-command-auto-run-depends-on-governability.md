# Command Auto-Run Depends On Governability

Alan OS should decide automatic command execution from governability, not
from a coarse read/write split alone. A command may run automatically only when
policy, effect class, target scope, reversibility, execution guard strength, and
auditability support it; high-risk effects such as delete, publish, irreversible
modify, privilege escalation, cross-app writes, and opaque shell/process work
without strong confinement must require approval or denial.
