//! Frozen qualification inputs through the shipped client adapters and AgentFS.
//! No Machine/provider/executor is installed; this proves admission, not routing quality.
use super::*;
use alan_agent_protocol::{InputIntent, UserInputRecord};
use alan_ap::{Fid, FileServer, OpenMode};
use std::collections::HashSet;

fn corpus() -> Vec<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../openspec/changes/qualify-agent-input-routing/shadow-corpus.v1.json"
    )))
    .unwrap()["cases"]
        .as_array()
        .unwrap()
        .clone()
}

#[tokio::test]
async fn frozen_ordinary_inputs_keep_source_and_identity_through_both_clients() {
    let (shell, _, _, _) = stdio_tests::live_root_agent().await;
    let attachment = open_stdio_tail_attachment(&shell, "/agent/root")
        .await
        .unwrap();
    let mut input_tail = shell.tail("/agent/root/io/input").await.unwrap();
    let mut ids = HashSet::new();
    let mut count = 0;
    for case in corpus()
        .iter()
        .filter(|case| case["pending_response"] == false)
    {
        let original = case["input"].as_str().unwrap().to_owned();
        for _repeat in 0..3 {
            for interactive in [true, false] {
                let expected = if interactive {
                    let mut app = FileBackedApp::new("/agent/root".into());
                    app.insert_input_text(&original);
                    let Some(FileBackedAction::Submit(record)) = app.handle_submit() else {
                        panic!("ordinary input was not admitted: {}", case["id"]);
                    };
                    write_agent_input(
                        &shell,
                        "/agent/root",
                        Some(attachment.root_agent_pid),
                        &record,
                    )
                    .await
                    .unwrap();
                    record
                } else {
                    let task = StdioTaskWaitContext::new(&original).unwrap();
                    submit_stdio_task(&shell, &task, &attachment).await.unwrap();
                    task.record
                };
                let bytes =
                    tokio::time::timeout(std::time::Duration::from_secs(2), input_tail.read(65536))
                        .await
                        .unwrap()
                        .unwrap();
                let split = bytes.iter().position(|byte| *byte == b'\n').unwrap();
                let length: usize = std::str::from_utf8(&bytes[..split])
                    .unwrap()
                    .parse()
                    .unwrap();
                assert_eq!(length, bytes.len() - split - 1);
                let observed = UserInputRecord::decode_payload(&bytes[split + 1..])
                    .unwrap()
                    .unwrap();
                assert_eq!(observed, expected, "wire identity/body for {}", case["id"]);
                assert!(ids.insert(observed.submission_id));
                let (intent, body) = match original.as_bytes()[0] {
                    b'!' => (InputIntent::Command, &original[1..]),
                    b':' => (InputIntent::ForceAgent, &original[1..]),
                    _ => (InputIntent::Agent, original.as_str()),
                };
                assert_eq!(observed.intent, intent);
                assert_eq!(observed.body.as_bytes(), body.as_bytes());
                assert_eq!(original, case["input"].as_str().unwrap(), "retained source");
                count += 1;
            }
        }
    }
    assert_eq!(count, 300);
    input_tail.close().await.unwrap();
    close_stdio_tails(attachment.tape_tail, attachment.ui_tail)
        .await
        .unwrap();
}

#[tokio::test]
async fn frozen_pending_responses_use_request_files_without_creating_input() {
    let (shell, root, _, _) = stdio_tests::live_root_agent().await;
    for case in corpus()
        .iter()
        .filter(|case| case["pending_response"] == true)
    {
        for _repeat in 0..3 {
            let original = case["input"].as_str().unwrap().to_owned();
            // The fixture acts as the request owner; the response uses the product client path.
            let fid = Fid(1_000_000);
            root.walk(
                Fid::ROOT,
                fid,
                &["root".into(), "requests".into(), "clone".into()],
            )
            .await
            .unwrap();
            root.open(fid, OpenMode::ReadWrite).await.unwrap();
            let request = String::from_utf8(root.read(fid, 0, 64).await.unwrap()).unwrap();
            root.clunk(fid).await.unwrap();
            let mut app = FileBackedApp::new("/agent/root".into());
            app.set_pending_yield(crate::history::PendingYieldCell {
                request_id: request.clone(),
                kind: alan_agent_protocol::YieldKind::Custom("text".into()),
                title: "Answer".into(),
                prompt: None,
                options: vec![],
                default_option: None,
                questions: vec![],
                capability: None,
                reason: None,
                presentation: None,
            });
            app.insert_input_text(&original);
            let Some(FileBackedAction::Resume {
                request_id,
                response,
                ..
            }) = app.handle_submit()
            else {
                panic!("pending response was parsed as ordinary input");
            };
            assert_eq!(request_id, request);
            file_surface::write_request_response(&shell, "/agent/root", &request_id, &response)
                .await
                .unwrap();
            assert_eq!(
                shell
                    .cat(&format!("/agent/root/requests/{request}/response"))
                    .await
                    .unwrap(),
                original.as_bytes()
            );
            assert_eq!(
                shell
                    .cat(&format!("/agent/root/requests/{request}/status"))
                    .await
                    .unwrap(),
                b"answered"
            );
            assert!(shell.cat("/agent/root/io/input").await.unwrap().is_empty());
        }
    }
    // The 12 redirected-response observations remain unsupported, not passing rows.
}

#[tokio::test]
async fn redirected_wait_requires_a_tty_instead_of_accepting_a_response() {
    let (shell, _, _, _) = stdio_tests::live_root_agent().await;
    let mut attachment = open_stdio_tail_attachment(&shell, "/agent/root")
        .await
        .unwrap();
    let task = StdioTaskWaitContext::new("ask a question").unwrap();
    submit_stdio_task(&shell, &task, &attachment).await.unwrap();
    let mut activity = alan_agent_protocol::UiActivitySnapshot::paused(None);
    activity
        .waiting_submission_ids
        .push(task.record.submission_id.clone());
    let mut event = serde_json::to_vec(&UiEvent::Activity { snapshot: activity }).unwrap();
    event.push(b'\n');
    shell
        .write("/agent/root/machine/ui/events", &event)
        .await
        .unwrap();
    let error = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        wait_for_stdio_answer_after_submit(&shell, task, &mut attachment, std::future::pending()),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(error.to_string().contains("needs interactive input"));
    close_stdio_tails(attachment.tape_tail, attachment.ui_tail)
        .await
        .unwrap();
}
