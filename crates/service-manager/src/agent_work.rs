//! Task-oriented Agent work commands over the invoking Process's namespace.
use alan_agent_engine::{
    InputIntent, InputMode, OwnerWorkControl, OwnerWorkRequest, UserInputRecord,
};
use alan_ap::InProcessTransport;
use alan_kernel::{MountFs, ProcessInvocation, ProcessOutcome, ProcessRunner};
use alan_shell::Shell;
use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

pub(crate) const EXECUTABLE: &str = "/bin/agent_work";
const HELP: &str = "agent_work status TARGET | result TARGET | submit TARGET TEXT | cancel TARGET INPUT_ID | continue TARGET | discard TARGET\nTARGET is root or an Agent PID visible to you. root is this invocation's Root Agent Process and may be the caller itself. Submit queues input for that target, including legitimate self-scheduling; it does not notify an external operator and is not an external handoff or report. Submit returns an input ID, not a completed answer. Cancel/continue/discard request a queue change; inspect status to observe it. No command retries an uncertain write. JSON arguments use action, target, and text or submission_id. The explicit select_owner JSON action accepts a versioned request with question, evaluator_profile and candidate source ranges; it queues bounded read-only Machine work. Result reads the latest acknowledged machine/work projection, which can belong to a different submission; compare work_id. It never waits for completion or retries work.";

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Status {
        target: String,
    },
    Submit {
        target: String,
        text: String,
    },
    SelectOwner {
        target: String,
        request: OwnerWorkRequest,
    },
    Result {
        target: String,
    },
    Cancel {
        target: String,
        submission_id: String,
    },
    Continue {
        target: String,
    },
    Discard {
        target: String,
    },
}

pub(crate) fn manifest() -> Vec<u8> {
    let source = json!({"type":"object", "additionalProperties":false,
        "required":["path","start_line","end_line"], "properties":{
            "path":{"type":"string","description":"Normalized /mnt/ namespace path"},
            "start_line":{"type":"integer","minimum":1},
            "end_line":{"type":"integer","minimum":1}}});
    let candidate = json!({"type":"object", "additionalProperties":false,
        "required":["id","sources"], "properties":{"id":{"type":"string","minLength":1},
            "sources":{"type":"array","minItems":1,"maxItems":8,"items":source}}});
    let request = json!({"type":"object", "additionalProperties":false,
        "required":["version","question","evaluator_profile","candidates"], "properties":{
            "version":{"const":1},"question":{"type":"string","minLength":1},
            "evaluator_profile":{"type":"string","minLength":1},
            "candidates":{"type":"array","minItems":1,"maxItems":16,"items":candidate}}});
    serde_json::to_vec(&json!({
        "version":1, "name":"agent_work", "description":HELP,
        "parameters":{"type":"object", "required":["action","target"], "additionalProperties":false,
            "properties":{"action":{"type":"string","enum":["status","submit","select_owner","result","cancel","continue","discard"]},
                "target":{"type":"string","description":"root (this invocation's Root Agent Process, which may be the caller itself) or a visible Agent PID"},
                "text":{"type":"string"}, "submission_id":{"type":"string"}, "request":request},
            "oneOf":[
                {"properties":{"action":{"const":"select_owner"}},"required":["request"],"not":{"anyOf":[{"required":["text"]},{"required":["submission_id"]}]}},
                {"properties":{"action":{"const":"submit"}},"required":["text"],"not":{"anyOf":[{"required":["submission_id"]},{"required":["request"]}]}},
                {"properties":{"action":{"const":"cancel"}},"required":["submission_id"],"not":{"anyOf":[{"required":["text"]},{"required":["request"]}]}},
                {"properties":{"action":{"enum":["status","result","continue","discard"]}},"not":{"anyOf":[{"required":["text"]},{"required":["submission_id"]},{"required":["request"]}]}}
            ]},
        "capability":"write", "timeout_secs":10,
        "execution":{"arguments":"json_first_arg","result":"stdout_json"}
    })).expect("static Agent work manifest")
}

pub(crate) struct AgentWorkProcessRunner;

#[async_trait]
impl ProcessRunner for AgentWorkProcessRunner {
    async fn run(&self, invocation: ProcessInvocation) -> ProcessOutcome {
        if invocation.exec.executable != EXECUTABLE {
            return output(127, json!({"success":false,"error":"executable mismatch"}));
        }
        let args = &invocation.exec.args;
        if args.is_empty() || matches!(args.as_slice(), [arg] if arg == "--help" || arg == "help") {
            return output(0, json!({"success":true,"help":HELP}));
        }
        let action = match parse(args) {
            Ok(action) => action,
            Err(error) => {
                return output(
                    2,
                    json!({"success":false,"error":error.to_string(),"help":HELP}),
                );
            }
        };
        // No service reference, Host store, or ambient connection is available here.
        let shell = Shell::new(InProcessTransport::new(Arc::new(MountFs::new(
            invocation.namespace,
        ))));
        match execute(&shell, action).await {
            Ok(result) => output(0, result),
            Err(error) => output(1, json!({"success":false,"error":format!("{error:#}")})),
        }
    }
}

fn output(code: i32, mut value: Value) -> ProcessOutcome {
    value["version"] = json!(1);
    let mut bytes = serde_json::to_vec(&value).expect("Agent work JSON result");
    bytes.push(b'\n');
    ProcessOutcome::exited(code, bytes)
}

fn parse(args: &[String]) -> Result<Action> {
    if let [json] = args
        && json.starts_with('{')
    {
        return serde_json::from_str(json).context("invalid Agent work request");
    }
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["status", target] => Ok(Action::Status {
            target: (*target).into(),
        }),
        ["result", target] => Ok(Action::Result {
            target: (*target).into(),
        }),
        ["submit", target, text] => Ok(Action::Submit {
            target: (*target).into(),
            text: (*text).into(),
        }),
        ["cancel", target, id] => Ok(Action::Cancel {
            target: (*target).into(),
            submission_id: (*id).into(),
        }),
        ["continue", target] => Ok(Action::Continue {
            target: (*target).into(),
        }),
        ["discard", target] => Ok(Action::Discard {
            target: (*target).into(),
        }),
        _ => bail!("invalid Agent work arguments"),
    }
}

async fn execute(shell: &Shell, action: Action) -> Result<Value> {
    let target = match &action {
        Action::Status { target }
        | Action::Submit { target, .. }
        | Action::SelectOwner { target, .. }
        | Action::Result { target }
        | Action::Cancel { target, .. }
        | Action::Continue { target }
        | Action::Discard { target } => target,
    };
    ensure!(
        target == "root"
            || (!target.is_empty()
                && target.bytes().all(|b| b.is_ascii_digit())
                && target.parse::<u64>().is_ok_and(|pid| pid > 0)),
        "target must be root or an Agent PID"
    );
    let base = format!("/agent/{target}");
    match action {
        Action::Status { target } => {
            let activity: Value = serde_json::from_slice(
                &shell
                    .cat(&format!("{base}/machine/ui/activity"))
                    .await
                    .context("read work status")?,
            )?;
            Ok(json!({"success":true,"target":target,"activity":activity}))
        }
        Action::Submit { target, text } => {
            ensure!(!text.trim().is_empty(), "work text must not be empty");
            let input = UserInputRecord::new(InputIntent::Agent, InputMode::FollowUp, text);
            shell.write(&format!("{base}/io/input"), &input.encode_payload()?).await
                .with_context(|| format!("submit {} failed; delivery may be unknown, inspect this ID before resubmitting", input.submission_id))?;
            Ok(
                json!({"success":true,"target":target,"status":"submitted","submission_id":input.submission_id}),
            )
        }
        Action::SelectOwner { target, request } => {
            let control = OwnerWorkControl {
                id: uuid::Uuid::new_v4(),
                request,
            };
            let bytes = control.encode()?;
            shell.write(&format!("{base}/machine/ctl"), bytes.as_bytes()).await
                .with_context(|| format!("owner work {} delivery may be unknown; inspect this ID before resubmitting", control.id))?;
            Ok(
                json!({"success":true,"target":target,"status":"submitted","submission_id":control.id}),
            )
        }
        Action::Result { target } => {
            let projection: Value =
                serde_json::from_slice(&shell.cat(&format!("{base}/machine/work")).await?)?;
            Ok(json!({"success":true,"target":target,"projection":projection}))
        }
        other => {
            let (target, control) = match other {
                Action::Cancel {
                    target,
                    submission_id,
                } => {
                    let id = uuid::Uuid::parse_str(&submission_id).context("invalid input ID")?;
                    (target, format!("queue-v1 interrupt {id}"))
                }
                Action::Continue { target } => (target, "queue-v1 continue".into()),
                Action::Discard { target } => (target, "queue-v1 discard".into()),
                _ => unreachable!(),
            };
            shell
                .write(&format!("{base}/machine/ctl"), control.as_bytes())
                .await
                .context(
                    "queue request failed; delivery may be unknown, inspect status before retrying",
                )?;
            Ok(json!({"success":true,"target":target,"status":"requested"}))
        }
    }
}

#[cfg(test)]
#[path = "agent_work/tests.rs"]
mod tests;
