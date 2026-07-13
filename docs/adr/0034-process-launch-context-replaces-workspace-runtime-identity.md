# Process Launch Context replaces workspace runtime identity

Status: accepted

Alan OS does not assign workspace identity to the system, Agent Processes, or
Tools. A Process receives execution context through its parent namespace
snapshot plus explicit mounts, descriptors, credentials, and an initial
namespace current directory. Host directories are mounted file trees rather
than workspace roots; Tool reachability comes from `/bin` and policy rather
than global/workspace-local classification. Alan for macOS Space remains a host
presentation concept and is not an Alan OS execution identity.
