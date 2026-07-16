//! Generation as a clone-via-open connection directory (add-llm-file-server §4,
//! the minimal callable slice brought into the Plan 9 core). A caller opens
//! `connections/<conn>/clone` (allocating a Generation directory), writes one
//! request document to `data` (committed on clunk), and reads the streamed token
//! records from `events`. Backed by the mock provider — no real API key.

include!("generation/connection_and_request_contract.inc.rs");
include!("generation/lifecycle_and_failure_contract.inc.rs");
