//! Task-oriented Agent work commands over the invoking Process's namespace.
use alan_agent_engine::{InputIntent, InputMode, UserInputRecord};
use alan_ap::InProcessTransport;
use alan_kernel::{MountFs, ProcessInvocation, ProcessOutcome, ProcessRunner};
use alan_shell::Shell;
use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

pub(crate) const EXECUTABLE: &str = "/bin/agent_work";
const HELP: &str = "agent_work status TARGET | submit TARGET TEXT | cancel TARGET INPUT_ID | continue TARGET | discard TARGET\nTARGET is root or an Agent PID visible to you. Submit returns an input ID, not a completed answer. Cancel/continue/discard request a queue change; inspect status to observe it. No command retries an uncertain write. JSON arguments use action, target, and text or submission_id.";

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
    serde_json::to_vec(&json!({
        "version":1, "name":"agent_work", "description":HELP,
        "parameters":{"type":"object", "required":["action","target"], "additionalProperties":false,
            "properties":{"action":{"type":"string","enum":["status","submit","cancel","continue","discard"]},
                "target":{"type":"string","description":"root or a visible Agent PID"},
                "text":{"type":"string"}, "submission_id":{"type":"string"}}},
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
        other => {
            let (target, control) = match other {
                Action::Cancel {
                    target,
                    submission_id,
                } => {
                    uuid::Uuid::parse_str(&submission_id).context("invalid input ID")?;
                    (target, format!("queue-v1 interrupt {submission_id}"))
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
mod tests {
    use super::*;
    use alan_kernel::{Access, Credentials, ExecSpec, Namespace, Pid};
    use std::collections::BTreeMap;

    fn invocation(namespace: Namespace, args: &[&str]) -> ProcessInvocation {
        ProcessInvocation {
            pid: Pid(2),
            parent: Some(Pid(1)),
            credentials: Credentials::user("test"),
            namespace,
            exec: ExecSpec {
                executable: EXECUTABLE.into(),
                args: args.iter().map(|s| (*s).into()).collect(),
                namespace: Default::default(),
                descriptors: BTreeMap::new(),
            },
        }
    }

    #[tokio::test]
    async fn work_commands_use_only_visible_agent_files_and_preserve_receipt_semantics() {
        let manifest: alan_agent_engine::runtime::ToolPackageManifest =
            serde_json::from_slice(&manifest()).unwrap();
        manifest.validate_for_name("agent_work").unwrap();
        let fs = Arc::new(alan_agentfs::AgentFs::new());
        let mut namespace = Namespace::new();
        namespace.mount("/agent/7", InProcessTransport::new(fs), Access::ReadWrite);
        let shell = Shell::new(InProcessTransport::new(Arc::new(MountFs::new(
            namespace.clone(),
        ))));
        let runner = AgentWorkProcessRunner;
        let status = runner
            .run(invocation(namespace.clone(), &["status", "7"]))
            .await;
        assert_eq!(status.exit_code, 0);
        assert!(serde_json::from_slice::<Value>(&status.output).unwrap()["activity"].is_object());
        let submitted = runner
            .run(invocation(
                namespace.clone(),
                &[r#"{"action":"submit","target":"7","text":"do work"}"#],
            ))
            .await;
        assert_eq!(submitted.exit_code, 0);
        let receipt: Value = serde_json::from_slice(&submitted.output).unwrap();
        assert_eq!(receipt["status"], "submitted");
        let id = receipt["submission_id"].as_str().unwrap();
        uuid::Uuid::parse_str(id).unwrap();
        let input = shell.cat("/agent/7/io/input").await.unwrap();
        let input = String::from_utf8_lossy(&input);
        assert!(
            input.contains(id)
                && input.contains("do work")
                && input.contains("\"intent\":\"agent\"")
        );
        for args in [
            vec!["cancel", "7", id],
            vec!["continue", "7"],
            vec!["discard", "7"],
        ] {
            let outcome = runner.run(invocation(namespace.clone(), &args)).await;
            assert_eq!(outcome.exit_code, 0);
            assert_eq!(
                serde_json::from_slice::<Value>(&outcome.output).unwrap()["status"],
                "requested"
            );
        }
        for args in [
            vec!["status", "root"],
            vec!["submit", "7/../8", "escape"],
            vec!["cancel", "7", "bad-id"],
        ] {
            assert_ne!(
                runner
                    .run(invocation(namespace.clone(), &args))
                    .await
                    .exit_code,
                0
            );
        }
        let mut read_only = Namespace::new();
        read_only.mount(
            "/agent/7",
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            Access::ReadOnly,
        );
        let denied = runner
            .run(invocation(read_only, &["submit", "7", "forbidden"]))
            .await;
        assert_eq!(denied.exit_code, 1);
        assert!(
            String::from_utf8_lossy(&denied.output).contains("access"),
            "{}",
            String::from_utf8_lossy(&denied.output)
        );
        let missing = crate::process_runner::SystemProcessRunner::new(None, None)
            .run(invocation(namespace, &["status", "7"]))
            .await;
        assert_eq!(missing.exit_code, 127);
    }
}
