# No Kernel Journal Primitive

Alan Kernel should not model a Kernel-owned semantic journal. Services and apps
may expose named stream Files for activity, audit, recovery, replay, and
projection rebuilds, but those stream Files stay owned by the service or app
that understands the events. Kernel provides namespace/mounts, paths, Files,
Descriptors, Access Rights, Credentials, Processes, and the Process Table; it
does not become the system audit database or projection replay authority.
