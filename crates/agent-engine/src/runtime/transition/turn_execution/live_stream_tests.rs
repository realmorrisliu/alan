use std::sync::Arc;

use alan_agent_protocol::{Event, UiEvent, UiThinkingState};
use alan_agentfs::AgentFs;
use alan_ap::InProcessTransport;
use alan_kernel::{Access, MountFs, Namespace};
use alan_shell::Shell;

use super::*;
use crate::runtime::transition::NamespaceRuntimeEnvironment;

async fn test_agent_files() -> (NamespaceAgentFiles, Shell) {
    let agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let env = NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default");
    (env.agent_files(), Shell::new(root))
}

#[tokio::test]
async fn text_deltas_stream_to_io_output_without_guardrail_hold() {
    let (files, shell) = test_agent_files().await;
    let stream = LiveTurnStream::new(files, None);

    let mut p1 = stream.plan_event(Event::TextDelta {
        chunk: "Hello ".to_string(),
        is_final: false,
    });
    assert_eq!(p1.take_forward().len(), 1);
    stream.apply(p1).await;
    assert!(stream.text_visible());
    assert_eq!(
        String::from_utf8(shell.cat("/agent/1/io/output").await.unwrap()).unwrap(),
        "Hello "
    );

    let mut p2 = stream.plan_event(Event::TextDelta {
        chunk: "world!".to_string(),
        is_final: false,
    });
    assert_eq!(p2.take_forward().len(), 1);
    stream.apply(p2).await;
    assert_eq!(
        String::from_utf8(shell.cat("/agent/1/io/output").await.unwrap()).unwrap(),
        "Hello world!"
    );
    assert_eq!(stream.unstreamed_remainder("Hello world!"), None);
    stream.check().unwrap();
}

#[tokio::test]
async fn thinking_deltas_publish_streaming_then_complete_on_first_text() {
    let (files, shell) = test_agent_files().await;
    let stream = LiveTurnStream::new(files, None);

    let mut p1 = stream.plan_event(Event::ThinkingDelta {
        chunk: "Checking files".to_string(),
        is_final: false,
    });
    assert_eq!(p1.take_forward().len(), 1);
    stream.apply(p1).await;
    assert!(stream.thinking_published());

    let snapshot: UiThinkingSnapshot =
        serde_json::from_slice(&shell.cat("/agent/1/machine/ui/thinking").await.unwrap()).unwrap();
    assert_eq!(snapshot.state, UiThinkingState::Streaming);
    assert_eq!(snapshot.text, "Checking files");

    let mut p2 = stream.plan_event(Event::TextDelta {
        chunk: "Done".to_string(),
        is_final: false,
    });
    let forwarded = p2.take_forward();
    assert!(matches!(
        forwarded.first(),
        Some(Event::ThinkingDelta { is_final: true, .. })
    ));
    assert!(matches!(
        forwarded.last(),
        Some(Event::TextDelta {
            chunk,
            is_final: false
        }) if chunk == "Done"
    ));
    stream.apply(p2).await;

    let snapshot: UiThinkingSnapshot =
        serde_json::from_slice(&shell.cat("/agent/1/machine/ui/thinking").await.unwrap()).unwrap();
    assert_eq!(snapshot.state, UiThinkingState::Complete);
    assert_eq!(snapshot.text, "Checking files");

    let ui_events =
        String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
    let thinking_states: Vec<UiThinkingState> = ui_events
        .lines()
        .filter_map(|line| serde_json::from_str::<UiEvent>(line).ok())
        .filter_map(|event| match event {
            UiEvent::Thinking { snapshot } => Some(snapshot.state),
            _ => None,
        })
        .collect();
    assert_eq!(
        thinking_states,
        vec![UiThinkingState::Streaming, UiThinkingState::Complete]
    );
}

#[tokio::test]
async fn guardrail_hold_blocks_rejected_prefix_and_releases_clean_prefix() {
    let (files, shell) = test_agent_files().await;
    let probe: GuardrailProbe =
        Arc::new(|prefix: &str| prefix.contains("don't have access to tools"));
    let stream = LiveTurnStream::new(files.clone(), Some(Arc::clone(&probe)));

    // A short contradictory draft stays held until discarded.
    let mut p1 = stream.plan_event(Event::TextDelta {
        chunk: "Sorry, I don't have access to tools right now.".to_string(),
        is_final: false,
    });
    assert!(p1.take_forward().is_empty());
    stream.apply(p1).await;
    assert!(!stream.text_visible());
    assert!(shell.cat("/agent/1/io/output").await.unwrap().is_empty());
    stream.apply(stream.plan_discard()).await;
    assert!(!stream.text_visible());

    // A clean draft exceeding GUARDRAIL_HOLD_CHARS releases live before completion.
    let stream = LiveTurnStream::new(files, Some(probe));
    let long_clean = "a".repeat(GUARDRAIL_HOLD_CHARS + 5);
    let mut p2 = stream.plan_event(Event::TextDelta {
        chunk: long_clean.clone(),
        is_final: false,
    });
    assert_eq!(p2.take_forward().len(), 1);
    stream.apply(p2).await;
    assert!(stream.text_visible());
    assert_eq!(
        String::from_utf8(shell.cat("/agent/1/io/output").await.unwrap()).unwrap(),
        long_clean
    );
}
