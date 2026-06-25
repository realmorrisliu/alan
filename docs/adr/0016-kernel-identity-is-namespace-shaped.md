# Kernel Identity Is Namespace-Shaped

Alan Kernel should use namespace-qualified Paths, Process Table entries, and
mounted file trees as canonical semantic identity, while typed opaque ids remain
runtime references for projections, caches, compatibility surfaces, and
in-flight state. This keeps Alan OS close to a filesystem, process table, and
mount tree, and prevents the Kernel from becoming an object database whose UUIDs
are treated as authority.
