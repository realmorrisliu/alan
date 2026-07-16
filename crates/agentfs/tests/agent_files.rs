//! agentfs as the read-write file backing of the agent process's state
//! (`refactor-engine-namespace-native` §4): the agent writes io/output, the
//! tape, requests and actions as files; the shell writes io/input; consumers
//! read/tail. No `EventEnvelope` on the path — everything is aP file IO.

include!("agent_files/io_and_action_contracts.inc.rs");
include!("agent_files/machine_and_event_contracts.inc.rs");
