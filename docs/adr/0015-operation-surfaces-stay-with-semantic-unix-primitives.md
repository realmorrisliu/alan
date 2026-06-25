# Operation Surfaces Stay With Semantic UNIX Primitives

Commands and queries should remain typed operation surfaces over paths, files,
descriptors, access rights, processes, and namespaces rather than becoming
independent Alan Kernel primitives: a command spawns a process or writes a file,
a query reads files or snapshots. Watching is a blocking read on a stream file;
Subscription is retired (ADR-0024 D8), not an operation surface, and there is no
subscription registry. V1 registries may index command and query descriptors for
discovery and compatibility, but durable semantics stay close to executable
files, read-only file inspection, process spawning, and stream-file reads so Alan
Kernel stays OS-shaped instead of app-framework-shaped.
