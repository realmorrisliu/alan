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
    pub(super) over_hard_token_threshold: bool,
    pub(super) emergency_mid_turn_compaction: bool,
}

impl CompactionPressure {
    pub(super) fn requires_degraded_fallback(&self, context_window_tokens: u32) -> bool {
        self.over_hard_token_threshold
            || self.emergency_mid_turn_compaction
            || (context_window_tokens == 0 && self.over_message_threshold)
    }
}

fn derived_soft_trigger_ratio(hard_trigger_ratio: f32) -> f32 {
    hard_trigger_ratio * 0.9
}

fn effective_hard_trigger_ratio(settings: &super::CompactionSettings) -> f32 {
    settings.hard_trigger_ratio.clamp(0.0, 1.0)
}

fn effective_soft_trigger_ratio(
    settings: &super::CompactionSettings,
    hard_trigger_ratio: f32,
) -> f32 {
    let soft_trigger_ratio = settings.soft_trigger_ratio.clamp(0.0, 1.0);
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
    settings: &super::CompactionSettings,
    request: &CompactionRequest,
    message_count: usize,
    estimated_prompt_tokens: usize,
) -> CompactionPressure {
    let context_window_tokens = settings.context_window_tokens as usize;
    let hard_trigger_ratio = effective_hard_trigger_ratio(settings);
    let soft_trigger_ratio = effective_soft_trigger_ratio(settings, hard_trigger_ratio);
    let soft_token_trigger_threshold =
        token_trigger_threshold(context_window_tokens, soft_trigger_ratio);
    let hard_token_trigger_threshold =
        token_trigger_threshold(context_window_tokens, hard_trigger_ratio);
    let context_window_utilization = if context_window_tokens == 0 {
        0.0
    } else {
        estimated_prompt_tokens as f64 / context_window_tokens as f64
    };
    let over_message_threshold = message_count > settings.trigger_messages;
    let over_hard_token_threshold =
        context_window_tokens > 0 && estimated_prompt_tokens >= hard_token_trigger_threshold;
    let over_soft_token_threshold =
        context_window_tokens > 0 && estimated_prompt_tokens >= soft_token_trigger_threshold;
    let unknown_window_message_overflow = context_window_tokens == 0 && over_message_threshold;
    let emergency_mid_turn_compaction = matches!(request.mode(), CompactionMode::AutoMidTurn)
        && crate::agent_machine::is_auto_mid_turn_compaction_emergency(
            estimated_prompt_tokens,
            context_window_tokens,
        );
    let level = match request.mode() {
        CompactionMode::Manual => CompactionPressureLevel::Hard,
        CompactionMode::AutoMidTurn => {
            if emergency_mid_turn_compaction
                || over_hard_token_threshold
                || unknown_window_message_overflow
            {
                CompactionPressureLevel::Hard
            } else {
                CompactionPressureLevel::BelowSoft
            }
        }
        CompactionMode::AutoPreTurn => {
            if emergency_mid_turn_compaction
                || over_hard_token_threshold
                || unknown_window_message_overflow
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
        over_hard_token_threshold,
        emergency_mid_turn_compaction,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::compaction::CompactionSettings;

    #[test]
    fn known_window_ignores_message_count_when_token_utilization_is_low() {
        let settings = CompactionSettings::new(60, 12, 1_000_000, 0.70, 0.85);
        // ~4% of 1M context window with 80 messages (> trigger_messages=60)
        let estimated_prompt_tokens = 40_000;
        let message_count = 80;

        let pre_turn = evaluate_compaction_pressure(
            &settings,
            &CompactionRequest::automatic_pre_turn(),
            message_count,
            estimated_prompt_tokens,
        );
        assert_eq!(pre_turn.level, CompactionPressureLevel::BelowSoft);
        assert!(pre_turn.over_message_threshold);
        assert!(!pre_turn.over_hard_token_threshold);
        assert!(!pre_turn.requires_degraded_fallback(settings.context_window_tokens));

        let mid_turn = evaluate_compaction_pressure(
            &settings,
            &CompactionRequest::automatic_mid_turn(),
            message_count,
            estimated_prompt_tokens,
        );
        assert_eq!(mid_turn.level, CompactionPressureLevel::BelowSoft);
        assert!(mid_turn.over_message_threshold);
        assert!(!mid_turn.over_hard_token_threshold);
        assert!(!mid_turn.requires_degraded_fallback(settings.context_window_tokens));
    }

    #[test]
    fn unknown_window_falls_back_to_message_threshold() {
        let settings = CompactionSettings::new(60, 12, 0, 0.70, 0.85);
        let message_count = 61;

        let pre_turn = evaluate_compaction_pressure(
            &settings,
            &CompactionRequest::automatic_pre_turn(),
            message_count,
            500,
        );
        assert_eq!(pre_turn.level, CompactionPressureLevel::Hard);
        assert!(pre_turn.over_message_threshold);
        assert!(!pre_turn.over_hard_token_threshold);
        assert!(pre_turn.requires_degraded_fallback(settings.context_window_tokens));

        let mid_turn = evaluate_compaction_pressure(
            &settings,
            &CompactionRequest::automatic_mid_turn(),
            message_count,
            500,
        );
        assert_eq!(mid_turn.level, CompactionPressureLevel::Hard);
        assert!(mid_turn.requires_degraded_fallback(settings.context_window_tokens));
    }

    #[test]
    fn requires_degraded_fallback_distinguishes_soft_manual_and_hard_pressure() {
        let settings = CompactionSettings::new(60, 12, 10_000, 0.70, 0.85);

        let soft_pre_turn = evaluate_compaction_pressure(
            &settings,
            &CompactionRequest::automatic_pre_turn(),
            80,
            7_500,
        );
        assert_eq!(soft_pre_turn.level, CompactionPressureLevel::Soft);
        assert!(!soft_pre_turn.requires_degraded_fallback(settings.context_window_tokens));

        let manual_below_hard =
            evaluate_compaction_pressure(&settings, &CompactionRequest::manual(None), 80, 5_000);
        assert!(!manual_below_hard.requires_degraded_fallback(settings.context_window_tokens));

        let hard_pre_turn = evaluate_compaction_pressure(
            &settings,
            &CompactionRequest::automatic_pre_turn(),
            10,
            8_600,
        );
        assert_eq!(hard_pre_turn.level, CompactionPressureLevel::Hard);
        assert!(hard_pre_turn.over_hard_token_threshold);
        assert!(hard_pre_turn.requires_degraded_fallback(settings.context_window_tokens));
    }
}
