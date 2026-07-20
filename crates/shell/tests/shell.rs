//! Alan Shell builtins over aP (introduce-alan-shell §3, §5.1). The shell is an
//! aP-only client: every builtin is generic file IO (walk/open/read/write/clunk
//! and clone-via-open for spawn) with no agent-specific command. These tests run
//! the builtins against an in-memory echo file server (the M1 milestone — input
//! echoed back through files, no LLM), against a partial-write server (short-write
//! handling), and against a real assembled namespace (`MountFs` over `/proc` and a
//! data mount) so path resolution crosses mounts as it will in production.

include!("shell/support_and_streams.inc.rs");
include!("shell/write_and_agent_contracts.inc.rs");
include!("shell/namespace_spawn_race.inc.rs");
