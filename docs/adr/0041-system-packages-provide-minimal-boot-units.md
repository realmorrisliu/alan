# System packages provide minimal Boot Units

Status: accepted

The first Service Manager reads a read-only `/lib/boot` tree installed by Alan
OS system packages. A Boot Unit names an executable, required descriptors and
mounts, dependency ordering, one of `never`, `on-failure`, or `always` restart,
and handles to publish. The Host starts only the Service Manager and never
parses units. This slice excludes arbitrary shell commands, environment
templating, user units, dynamic reload, and general target languages; package
transactions update the tree atomically for a later boot.
