//! Compaction pressure classification for manual and automatic requests.

use alan_agent_protocol::{CompactionMode, CompactionPressureLevel};

use super::CompactionRequest;

#[derive(Debug, Clone, Copy)]
pub(super) struct CompactionPressure {
    pub(super) level: CompactionPressureLevel,
    pub(super) soft_trigger_ratio: f32,
    pub(super) hard_trigger_ratio: f32,
    pub(super) soft_token_trigger_threshold: usize,
    pub(super) hard_token_trigger_threshold: usize,
    pub(super) context_window_utilization: f64,
    pub(super) over_message_threshold: bool,
    pub(super) emergency_mid_turn_compaction: bool,
}

fn derived_soft_trigger_ratio(hard_trigger_ratio: f32) -> f32 {
    hard_trigger_ratio * 0.9
}

fn effective_hard_trigger_ratio(runtime_config: &super::super::RuntimeConfig) -> f32 {
    runtime_config.compaction_hard_trigger_ratio.clamp(0.0, 1.0)
}

fn effective_soft_trigger_ratio(
    runtime_config: &super::super::RuntimeConfig,
    hard_trigger_ratio: f32,
) -> f32 {
    let soft_trigger_ratio = runtime_config.compaction_soft_trigger_ratio.clamp(0.0, 1.0);
    if soft_trigger_ratio < hard_trigger_ratio {
        soft_trigger_ratio
    } else {
        derived_soft_trigger_ratio(hard_trigger_ratio)
    }
}

fn token_trigger_threshold(context_window_tokens: usize, ratio: f32) -> usize {
    if context_window_tokens == 0 {
        0
    } else {
        ((context_window_tokens as f64) * (ratio as f64)).ceil() as usize
    }
}

pub(super) fn evaluate_compaction_pressure(
    runtime_config: &super::super::RuntimeConfig,
    request: &CompactionRequest,
    message_count: usize,
    estimated_prompt_tokens: usize,
) -> CompactionPressure {
    let context_window_tokens = runtime_config.context_window_tokens as usize;
    let hard_trigger_ratio = effective_hard_trigger_ratio(runtime_config);
    let soft_trigger_ratio = effective_soft_trigger_ratio(runtime_config, hard_trigger_ratio);
    let soft_token_trigger_threshold =
        token_trigger_threshold(context_window_tokens, soft_trigger_ratio);
    let hard_token_trigger_threshold =
        token_trigger_threshold(context_window_tokens, hard_trigger_ratio);
    let context_window_utilization = if context_window_tokens == 0 {
        0.0
    } else {
        estimated_prompt_tokens as f64 / context_window_tokens as f64
    };
    let over_message_threshold = message_count > runtime_config.compaction_trigger_messages;
    let over_hard_token_threshold =
        context_window_tokens > 0 && estimated_prompt_tokens >= hard_token_trigger_threshold;
    let over_soft_token_threshold =
        context_window_tokens > 0 && estimated_prompt_tokens >= soft_token_trigger_threshold;
    let emergency_mid_turn_compaction = matches!(request.mode(), CompactionMode::AutoMidTurn)
        && super::super::turn_state::is_auto_mid_turn_compaction_emergency(
            estimated_prompt_tokens,
            context_window_tokens,
        );
    let level = match request.mode() {
        CompactionMode::Manual => CompactionPressureLevel::Hard,
        CompactionMode::AutoMidTurn => {
            if emergency_mid_turn_compaction || over_message_threshold || over_hard_token_threshold
            {
                CompactionPressureLevel::Hard
            } else {
                CompactionPressureLevel::BelowSoft
            }
        }
        CompactionMode::AutoPreTurn => {
            if emergency_mid_turn_compaction || over_message_threshold || over_hard_token_threshold
            {
                CompactionPressureLevel::Hard
            } else if over_soft_token_threshold {
                CompactionPressureLevel::Soft
            } else {
                CompactionPressureLevel::BelowSoft
            }
        }
    };

    CompactionPressure {
        level,
        soft_trigger_ratio,
        hard_trigger_ratio,
        soft_token_trigger_threshold,
        hard_token_trigger_threshold,
        context_window_utilization,
        over_message_threshold,
        emergency_mid_turn_compaction,
    }
}
