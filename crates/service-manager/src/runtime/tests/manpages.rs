use super::*;

#[tokio::test]
async fn standard_namespace_mounts_real_adr0061_manpages_at_man() {
    let manager = ServiceManager::boot(ServiceManagerConfig::ephemeral(
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    ))
    .await
    .unwrap();
    let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
    let shell = alan_shell::Shell::new(InProcessTransport::new(namespace));

    let sections = shell.ls("/man").await.unwrap();
    assert!(sections.contains(&"1".to_string()), "{sections:?}");
    assert!(sections.contains(&"4".to_string()), "{sections:?}");
    assert!(sections.contains(&"7".to_string()), "{sections:?}");

    let sec1 = shell.ls("/man/1").await.unwrap();
    assert!(sec1.contains(&"alan".to_string()), "{sec1:?}");
    assert!(sec1.contains(&"q".to_string()), "{sec1:?}");
    assert!(sec1.contains(&"connection".to_string()), "{sec1:?}");
    assert!(sec1.contains(&"update".to_string()), "{sec1:?}");

    let alan9 = String::from_utf8(shell.cat("/man/7/alan9").await.unwrap()).unwrap();
    assert!(alan9.contains("ALAN9(7)"), "{alan9}");
    assert!(alan9.contains("/proc") && alan9.contains("/mnt"), "{alan9}");
    assert!(alan9.contains("Seatbelt"), "{alan9}");
    assert!(!alan9.to_ascii_lowercase().contains("herdr"), "{alan9}");

    let alan1 = String::from_utf8(shell.cat("/man/1/alan").await.unwrap()).unwrap();
    assert!(alan1.contains("ALAN(1)"), "{alan1}");
    assert!(alan1.contains("ALAN_THEME"), "{alan1}");
    assert!(alan1.contains("alan update"), "{alan1}");
    assert!(alan1.contains("ALAN_NO_UPDATE_CHECK"), "{alan1}");
    assert!(!alan1.to_ascii_lowercase().contains("herdr"), "{alan1}");

    let update1 = String::from_utf8(shell.cat("/man/1/update").await.unwrap()).unwrap();
    assert!(update1.contains("UPDATE(1)"), "{update1}");
    assert!(update1.contains("alan update"), "{update1}");
    assert!(update1.contains(".alan-cli-manifest"), "{update1}");
    assert!(update1.contains("ALAN_NO_UPDATE_CHECK"), "{update1}");
    assert!(!update1.to_ascii_lowercase().contains("herdr"), "{update1}");

    let q1 = String::from_utf8(shell.cat("/man/1/q").await.unwrap()).unwrap();
    for kind in ["skill", "mcp", "tool", "provider"] {
        assert!(q1.contains(kind), "missing q package kind {kind}: {q1}");
    }

    let conn1 = String::from_utf8(shell.cat("/man/1/connection").await.unwrap()).unwrap();
    for provider in [
        "magpie",
        "openai_responses",
        "chatgpt",
        "vertex",
        "openrouter",
    ] {
        assert!(
            conn1.contains(provider),
            "missing provider {provider}: {conn1}"
        );
    }
    assert!(conn1.contains("alan login"), "{conn1}");
    assert!(alan1.contains("/login"), "{alan1}");

    let agentfs4 = String::from_utf8(shell.cat("/man/4/agentfs").await.unwrap()).unwrap();
    assert!(agentfs4.contains("AGENTFS(4)"), "{agentfs4}");
    assert!(agentfs4.contains("machine/tape"), "{agentfs4}");

    manager.shutdown().await.unwrap();
}
