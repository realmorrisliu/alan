//! Measure the shipped prefix parser without a model, Machine or execution path.
//! Pending responses belong to the native response collector, not this parser.
use std::{path::PathBuf, time::Instant};

use alan_agent_protocol::{InputIntent, parse_input_prefix};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 4,
        "usage: routing_prefix_baseline CORPUS BUDGETS NEW_REPORT"
    );
    let raw = std::fs::read(&args[1])?;
    let corpus: Value = serde_json::from_slice(&raw)?;
    let budget_raw = std::fs::read(&args[2])?;
    let budgets: Value = serde_json::from_slice(&budget_raw)?;
    ensure!(
        budgets["version"] == 1
            && budgets["min_repeats"] == 3
            && budgets["surfaces"] == json!(["interactive", "redirected"]),
        "unsupported v1 budget matrix"
    );
    ensure!(corpus["version"] == 1, "unsupported corpus version");
    let cases = corpus["cases"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing cases"))?;
    let mut records = Vec::new();
    for repeat in 0..3 {
        for case in cases {
            // A pending response must never pass through the ordinary prefix parser.
            if case["pending_response"] == true {
                continue;
            }
            let input = case["input"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing input"))?;
            for surface in ["interactive", "redirected"] {
                let started = Instant::now();
                let (intent, body) = parse_input_prefix(input);
                let elapsed_ns = started.elapsed().as_nanos();
                let route = match intent {
                    InputIntent::Command => "command",
                    InputIntent::Agent | InputIntent::ForceAgent => "agent",
                };
                records.push(json!({
                    "case_id":case["id"],"repeat":repeat,"surface_stratum":surface,
                    "input_sha256":hash(input.as_bytes()),"body":body,"intent":intent,
                    "route":route,"elapsed_ns":elapsed_ns,"evaluator_calls":0,
                    "routing_cost_microusd":0
                }));
            }
        }
    }
    let document = json!({
        "version":1,"kind":"production_prefix_parser_component_baseline",
        "corpus_sha256":hash(&raw),"budgets_sha256":hash(&budget_raw),"fixture_source_sha256":hash(include_bytes!("routing_prefix_baseline.rs")),
        "binary_sha256":hash(&std::fs::read(std::env::current_exe()?)?),
        "client_parity_verified":false,"timing_scope":"parse_input_prefix only",
        "pending_responses":"excluded from parser; require native request evidence",
        "records":records
    });
    let path = PathBuf::from(&args[3]);
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    serde_json::to_writer_pretty(file, &document)?;
    Ok(())
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_parser_preserves_one_shot_intent_and_body() {
        for (input, intent, body) in [
            ("! echo hi ", InputIntent::Command, " echo hi "),
            (":!rm output", InputIntent::ForceAgent, "!rm output"),
            ("run pwd", InputIntent::Agent, "run pwd"),
            (" !pwd", InputIntent::Agent, " !pwd"),
        ] {
            assert_eq!(parse_input_prefix(input), (intent, body));
        }
    }
}
