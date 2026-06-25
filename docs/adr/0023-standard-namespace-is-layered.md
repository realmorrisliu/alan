# Standard Namespace Is Layered

Alan OS should keep top-level namespace roots small and UNIX/Plan 9-like:
`/proc`, `/agent`, `/srv`, `/bin`, `/lib`, `/man`, and `/mnt`. `/proc`,
`/agent`, and `/srv` are live Kernel/service views; `/bin`, `/lib`, and `/man`
are command, package, and documentation roots; `/mnt` is where mounted service,
app, and data trees appear. Alan-specific package trees such as skills, tool
metadata, policy packages, and memory mounts should live under `/lib` or `/mnt`
instead of becoming new default top-level roots.
