//! Startup classification never confuses missing managed metadata with injection.
use super::*;
use crate::connection::process_binding::ProcessConnection;
use alan_agent_engine::runtime::model_binding::ConnectionAuthority;

#[tokio::test]
async fn process_initial_binding_classifies_injection_and_preserves_managed_validation() {
    let service = ConnectionService::ephemeral("test");
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    service
        .attach_callable_registry(
            llmfs.clone(),
            Arc::new(TestLlmClientFactory::default()),
            Config::default(),
            Some(("injected".into(), LlmClient::new(MockLlmProvider::new()))),
        )
        .await
        .unwrap();
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(Arc::new(
            service.capture_connection("injected").await.unwrap(),
        )),
        alan_kernel::Access::ReadWrite,
    );
    let ns = alan_kernel::LiveNamespace::new(ns);
    let injected = ProcessConnection {
        service: service.clone(),
        profile: "injected".into(),
        namespace: ns.clone(),
    };
    let captured = injected.capture_initial().await.unwrap().unwrap();
    assert_eq!(captured.identity.revision, "injected-namespace-callable");
    assert_eq!(
        injected.restore(&captured.identity).await.unwrap().identity,
        captured.identity
    );
    assert!(injected.capture(Some("unauthorized-model")).await.is_err());
    let catalog = injected.catalog().await.unwrap();
    assert_eq!(catalog["profile"], "injected");
    assert_eq!(
        catalog["models"].as_array().unwrap().len(),
        1,
        "injected fallback only authorizes its original callable"
    );
    assert_eq!(catalog["models"][0]["model"], captured.identity.model);
    assert!(catalog.get("credential_ref").is_none());
    assert!(catalog.get("revision").is_none());
    let missing = ProcessConnection {
        service: service.clone(),
        profile: "missing".into(),
        namespace: ns.clone(),
    };
    assert!(missing.capture_initial().await.unwrap().is_none());
    assert!(missing.capture(None).await.is_err());
    assert!(missing.restore(&captured.identity).await.is_err());
    let mut connections = service.metadata();
    connections.credentials.insert(
        "openai-main".into(),
        crate::connection_profile::ConnectionCredential {
            kind: crate::connection_profile::CredentialKind::SecretString,
            provider_family: ProviderId::OpenAiResponses,
            label: "test reference".into(),
            backend: crate::connection_profile::default_credential_backend(
                crate::connection_profile::CredentialKind::SecretString,
            )
            .into(),
        },
    );
    service
        .apply(ConnectionCommand::ReplaceMetadata {
            expected: service.metadata().fingerprint().unwrap(),
            connections,
        })
        .await
        .unwrap();
    service
        .apply(ConnectionCommand::AddProfile {
            profile_id: "main".into(),
            profile: profile(),
        })
        .await
        .unwrap();
    let managed = ProcessConnection {
        service: service.clone(),
        profile: "main".into(),
        namespace: ns,
    };
    let captured = managed.capture_initial().await.unwrap().unwrap();
    assert_ne!(captured.identity.revision, "injected-namespace-callable");
    assert_eq!(
        managed.restore(&captured.identity).await.unwrap().identity,
        captured.identity
    );
    let mut wrong = captured.identity;
    wrong.revision.push_str("changed");
    assert!(managed.restore(&wrong).await.is_err());
}
