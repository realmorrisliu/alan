use super::*;

#[tokio::test]
async fn boot_rejects_ambient_package_namespace_mounts() {
    let mut config = ServiceManagerConfig::ephemeral(
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    );
    config.launch_context.namespace.mount(
        "/lib/pkg/ambient",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        Access::ReadOnly,
    );

    let error = ServiceManager::boot(config).await.err().unwrap();

    assert!(
        error
            .to_string()
            .contains("namespace mounts overlapping /lib/pkg are not accepted")
    );
}

#[tokio::test]
async fn boot_rejects_root_namespace_mount_covering_package_namespace() {
    let mut config = ServiceManagerConfig::ephemeral(
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    );
    config.launch_context.namespace.mount(
        "/",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        Access::ReadOnly,
    );

    let error = ServiceManager::boot(config).await.err().unwrap();

    assert!(
        error
            .to_string()
            .contains("namespace mounts overlapping /lib/pkg are not accepted")
    );
}
