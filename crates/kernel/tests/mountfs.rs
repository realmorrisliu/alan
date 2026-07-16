//! `MountFs` — the kernel namespace presented as one aP [`FileServer`], so a
//! single client (the shell, the engine) reaches a whole assembled namespace
//! (`/proc`, `/agent`, `/mnt/llm`) through one transport. Paths that cross a
//! mount are delegated to the backing tree (through `Resolved::call`, so the
//! mount's access is enforced); paths above the mounts are synthetic directories
//! that list their child mount points.

include!("mountfs/base_contract.inc.rs");
include!("mountfs/stream_and_union_contract.inc.rs");
