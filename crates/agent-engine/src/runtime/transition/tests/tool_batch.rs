use crate::runtime::{
    loop_guard::ToolLoopGuard,
    tool_effect_lifecycle::{EffectCategory, build_effect_identity},
    tool_execution::{execute_tool_effect, tool_payload_for_tape},
};
use serde_json::Value;

include!("tool_batch/support_and_namespace_contract.inc.rs");
include!("tool_batch/namespace_and_batch_contract.inc.rs");
include!("tool_batch/replay_and_effect_contract.inc.rs");
