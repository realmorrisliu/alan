use super::*;

#[tokio::test]
async fn empty_pair_validation_does_not_create_stores() {
    let temp = tempfile::tempdir().unwrap();
    validate_store_pair(&temp.path().join("system"), &temp.path().join("host"))
        .await
        .unwrap();
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn durable_validation_rejects_unknown_data_and_escaping_selection_without_replay() {
    let temp = tempfile::tempdir().unwrap();
    let system = temp.path().join("system");
    let host = temp.path().join("host");
    let runtime = system.join("services/agent-runtime");
    fs::create_dir_all(runtime.join("metadata")).unwrap();
    fs::write(runtime.join("metadata/root-rollout"), "../escape.jsonl").unwrap();
    assert!(validate_store_pair(&system, &host).await.is_err());
    assert_eq!(
        fs::read_to_string(runtime.join("metadata/root-rollout")).unwrap(),
        "../escape.jsonl"
    );
    fs::remove_file(runtime.join("metadata/root-rollout")).unwrap();
    fs::create_dir_all(runtime.join("rollouts")).unwrap();
    fs::write(
        runtime.join("rollouts/unknown.jsonl"),
        "{\"type\":\"future_schema\"}\n",
    )
    .unwrap();
    assert!(validate_store_pair(&system, &host).await.is_err());
    fs::remove_file(runtime.join("rollouts/unknown.jsonl")).unwrap();
    validate_store_pair(&system, &host).await.unwrap();
}

#[test]
fn missing_secret_is_distinct_from_explicit_logout() {
    let temp = tempfile::tempdir().unwrap();
    let store = SecretStore::from_directory(temp.path()).unwrap();
    assert!(store.validate_credential_for_migration("main").is_err());
    fs::write(temp.path().join("secrets.toml"), "revoked = ['main']\n").unwrap();
    store.validate_credential_for_migration("main").unwrap();
    assert_eq!(
        fs::read_to_string(temp.path().join("secrets.toml")).unwrap(),
        "revoked = ['main']\n"
    );
}

#[tokio::test]
async fn connection_backing_validation_preserves_logout_and_rejects_missing_material() {
    let temp = tempfile::tempdir().unwrap();
    let system = temp.path().join("system");
    let host = temp.path().join("host");
    let metadata = system.join("services/connections");
    fs::create_dir_all(&metadata).unwrap();
    let connections = r#"
version = 1
default_profile = "main"
[credentials.key]
kind = "secret_string"
provider_family = "openai_responses"
label = "Key"
backend = "host_credential_store"
[profiles.main]
provider = "openai_responses"
credential_id = "key"
[profiles.main.settings]
model = "gpt-5.4"
"#;
    fs::write(metadata.join("connections.toml"), connections).unwrap();
    let error = validate_store_pair(&system, &host).await.unwrap_err();
    assert!(
        error.to_string().contains("credential backing is missing"),
        "{error:#}"
    );
    fs::create_dir_all(host.join("credentials")).unwrap();
    let secret_path = host.join("credentials/secrets.toml");
    fs::write(&secret_path, "revoked = ['key']\n").unwrap();
    validate_store_pair(&system, &host).await.unwrap();
    fs::write(&secret_path, "[secrets]\nkey = 'fixture-secret'\n").unwrap();
    validate_store_pair(&system, &host).await.unwrap();
    assert_eq!(
        fs::read_to_string(metadata.join("connections.toml")).unwrap(),
        connections
    );
    assert_eq!(fs::read_dir(metadata).unwrap().count(), 1);
    fs::write(&secret_path, "[secrets]\nkey = fixture-secret\n").unwrap();
    let error = validate_store_pair(&system, &host).await.unwrap_err();
    assert!(!format!("{error:#}").contains("fixture-secret"));
}

#[tokio::test]
async fn authored_data_and_history_are_preserved_and_unknown_layouts_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let system = temp.path().join("system");
    let host = temp.path().join("host");
    let services = system.join("services");
    let memory = services.join("memory/stores/personal");
    fs::create_dir_all(&memory).unwrap();
    fs::write(memory.join("image.bin"), [0, 255, 4]).unwrap();
    let history = services.join("shell-ui");
    fs::create_dir_all(&history).unwrap();
    fs::write(history.join("composer-history.v1.jsonl"), "\"old input\"\n").unwrap();
    fs::write(
        history.join("composer-history.v2.jsonl"),
        serde_json::to_string(&alan_tui::composer::HistoryEntry {
            body: "new input".into(),
            intent: alan_agent_protocol::InputIntent::Agent,
        })
        .unwrap(),
    )
    .unwrap();
    validate_store_pair(&system, &host).await.unwrap();
    assert_eq!(fs::read(memory.join("image.bin")).unwrap(), [0, 255, 4]);
    fs::write(
        history.join("composer-history.v2.jsonl"),
        "{\"future\": true}\n",
    )
    .unwrap();
    assert!(validate_store_pair(&system, &host).await.is_err());
    fs::remove_file(history.join("composer-history.v2.jsonl")).unwrap();
    fs::create_dir(services.join("future-service")).unwrap();
    assert!(validate_store_pair(&system, &host).await.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn authored_symlink_is_not_followed() {
    let temp = tempfile::tempdir().unwrap();
    let system = temp.path().join("system");
    let memory = system.join("services/memory/stores");
    fs::create_dir_all(&memory).unwrap();
    std::os::unix::fs::symlink(temp.path().join("missing"), memory.join("link")).unwrap();
    assert!(
        validate_store_pair(&system, &temp.path().join("host"))
            .await
            .is_err()
    );
}
