//! `/proc` synthetic device (substrate §7.1) and spawn via clone-via-open
//! (§7.1a). `/proc` renders the process table as files: a `clone` file plus a
//! directory per pid (`status`, `parent`, `credentials`, `exit`, `ctl`, `io/`).
//! Process creation is pure aP — open `/proc/clone` (a pending pid, not yet
//! public), write the exec spec, and `clunk` to commit — so an aP-only client
//! needs no side API to launch a process.

include!("procfs/lifecycle_and_io_contract.inc.rs");
include!("procfs/observation_and_namespace_contract.inc.rs");
include!("procfs/namespace_generation_contract.inc.rs");
