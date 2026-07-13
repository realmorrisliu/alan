# Host files enter only through explicit Host Mounts

Status: accepted

Alan OS sees no Host OS files by default. A host must authorize a directory,
export it through hostfs, and mount that file tree explicitly into a Process
namespace; starting Alan from a directory may request such a mount but does not
create workspace identity. Kernel and Agent Execution Engine logic receive Alan
OS paths rather than raw host roots, child visibility follows namespace
inheritance, and native sandbox roots are derived from the same mount grants so
file visibility and host enforcement cannot drift apart.
