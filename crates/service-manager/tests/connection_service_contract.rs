use std::collections::BTreeMap;

use alan_agent_engine::LlmProvider;
use alan_ap::InProcessTransport;
use alan_service_manager::{ConnectionProfile, ConnectionService};
use alan_shell::Shell;
use chrono::Utc;

fn profile() -> ConnectionProfile {
    ConnectionProfile {
        provider: LlmProvider::OpenAiResponses,
        label: None,
        credential_id: Some("openai-main".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        source: "managed".to_string(),
        settings: BTreeMap::from([
            (
                "base_url".to_string(),
                "https://api.openai.com/v1".to_string(),
            ),
            ("model".to_string(), "gpt-5.4".to_string()),
        ]),
    }
}

async fn add_profile(shell: &Shell, profile_id: &str) {
    shell
        .write(
            "/ctl",
            &serde_json::to_vec(&serde_json::json!({
                "op": "add_profile",
                "profile_id": profile_id,
                "profile": profile(),
            }))
            .unwrap(),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn process_selection_overrides_and_then_falls_back_to_service_default() {
    let service = ConnectionService::ephemeral("test");
    let shell = Shell::new(InProcessTransport::new(service.file_server()));
    add_profile(&shell, "default-profile").await;
    add_profile(&shell, "process-profile").await;
    shell
        .write(
            "/ctl",
            br#"{"op":"set_default","profile_id":"default-profile"}"#,
        )
        .await
        .unwrap();

    assert_eq!(
        service.selected_profile(42).as_deref(),
        Some("default-profile")
    );
    service.select(42, "process-profile").unwrap();
    assert_eq!(
        service.selected_profile(42).as_deref(),
        Some("process-profile")
    );

    service.release_process(42);
    assert_eq!(
        service.selected_profile(42).as_deref(),
        Some("default-profile")
    );
}
