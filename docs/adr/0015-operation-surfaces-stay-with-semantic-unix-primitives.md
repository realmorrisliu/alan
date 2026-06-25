# Operation Surfaces Stay With Semantic UNIX Primitives

Commands, queries, and subscriptions should remain typed operation surfaces over
paths, files, descriptors, access rights, processes, and namespaces rather than
becoming independent Alan Kernel primitives. V1 registries may index command,
query, and subscription descriptors for discovery and compatibility, but
durable semantics should stay close to executable files, read-only file
inspection, process spawning, and watched stream files so Alan Kernel stays
OS-shaped instead of app-framework-shaped.
