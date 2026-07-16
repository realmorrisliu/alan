use super::*;
use alan_agent_engine::LlmProvider;
use alan_service_manager::{
    ConnectionCredential, ConnectionProfile, ConnectionsFile, CredentialKind,
    default_credential_backend,
};
use chrono::Utc;
use tempfile::TempDir;
fn stores(root: &Path, channel: InstallChannel) -> (SystemStorePaths, HostStorePaths) {
    let data = root.join("data");
    fs::create_dir_all(&data).unwrap();
    (
        SystemStorePaths::from_data_dir(&data, channel.descriptor().id).unwrap(),
        HostStorePaths::from_data_dir(&data, channel.descriptor().id).unwrap(),
    )
}

fn connection_file(profile_id: &str) -> ConnectionsFile {
    let mut file = ConnectionsFile {
        default_profile: Some(profile_id.to_string()),
        ..ConnectionsFile::default()
    };
    file.profiles.insert(
        profile_id.to_string(),
        ConnectionProfile {
            provider: LlmProvider::OpenAiResponses,
            label: None,
            credential_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            source: "test".to_string(),
            settings: Default::default(),
        },
    );
    file
}

#[test]
fn connection_metadata_is_merged_verified_and_deleted() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let paths = LegacyStatePaths::from_home_dir(&home, InstallChannel::Stable).unwrap();
    fs::create_dir_all(&paths.alan_root).unwrap();
    let legacy = connection_file("legacy-main");
    legacy.save_to_path(&paths.connections_metadata()).unwrap();
    let (system, host) = stores(temp.path(), InstallChannel::Stable);

    let report = migrate_legacy_connections(&paths, &system, &host).unwrap();

    assert!(report.metadata_migrated);
    assert!(!paths.connections_metadata().exists());
    assert_eq!(
        ConnectionsFile::load_from_path(&system.connections_metadata().unwrap())
            .unwrap()
            .0,
        legacy
    );
}

#[test]
fn legacy_workspace_pins_are_dropped_during_connection_migration() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let paths = LegacyStatePaths::from_home_dir(&home, InstallChannel::Stable).unwrap();
    fs::create_dir_all(&paths.alan_root).unwrap();
    let mut legacy = connection_file("legacy-main");
    legacy.credentials.insert(
        "legacy-secret".to_string(),
        ConnectionCredential {
            kind: CredentialKind::SecretString,
            provider_family: LlmProvider::OpenAiResponses,
            label: "Legacy secret".to_string(),
            backend: "alan_home_secret_store".to_string(),
        },
    );
    let mut document = toml::Value::try_from(&legacy).unwrap();
    document.as_table_mut().unwrap().insert(
        "workspace_pins".to_string(),
        toml::Value::Table(toml::map::Map::from_iter([(
            "/legacy/project".to_string(),
            toml::Value::String("legacy-main".to_string()),
        )])),
    );
    fs::write(
        paths.connections_metadata(),
        toml::to_string_pretty(&document).unwrap(),
    )
    .unwrap();
    let (system, host) = stores(temp.path(), InstallChannel::Stable);

    let report = migrate_legacy_connections(&paths, &system, &host).unwrap();

    assert!(report.metadata_migrated);
    assert!(!paths.connections_metadata().exists());
    let target = system.connections_metadata().unwrap();
    let rendered = fs::read_to_string(&target).unwrap();
    assert!(!rendered.contains("workspace_pins"));
    let migrated = ConnectionsFile::load_from_path(&target).unwrap().0;
    assert_eq!(migrated.default_profile.as_deref(), Some("legacy-main"));
    assert_eq!(
        migrated.credentials["legacy-secret"].backend,
        default_credential_backend(CredentialKind::SecretString)
    );
}

#[test]
fn conflicting_connection_metadata_preserves_both_files() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let paths = LegacyStatePaths::from_home_dir(&home, InstallChannel::Stable).unwrap();
    fs::create_dir_all(&paths.alan_root).unwrap();
    connection_file("legacy-main")
        .save_to_path(&paths.connections_metadata())
        .unwrap();
    let (system, host) = stores(temp.path(), InstallChannel::Stable);
    connection_file("current-main")
        .save_to_path(&system.connections_metadata().unwrap())
        .unwrap();

    let error = migrate_legacy_connections(&paths, &system, &host).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("default connection profiles conflict")
    );
    assert!(paths.connections_metadata().is_file());
    assert!(system.connections_metadata().unwrap().is_file());
}

#[test]
fn secrets_move_only_between_host_owned_paths() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let paths = LegacyStatePaths::from_home_dir(&home, InstallChannel::Dev).unwrap();
    fs::create_dir_all(paths.credential_file().parent().unwrap()).unwrap();
    fs::write(paths.credential_file(), b"[secrets]\nmain = 'secret'\n").unwrap();
    fs::create_dir_all(&paths.alan_root).unwrap();
    fs::write(paths.managed_auth(), b"{\"token\":\"secret\"}").unwrap();
    let (system, host) = stores(temp.path(), InstallChannel::Dev);

    let report = migrate_legacy_connections(&paths, &system, &host).unwrap();

    assert!(report.credential_file_migrated);
    assert!(report.managed_auth_migrated);
    assert_eq!(
        fs::read(host.credentials.join(SECRET_STORE_FILE)).unwrap(),
        b"[secrets]\nmain = 'secret'\n"
    );
    assert_eq!(
        fs::read(host.managed_auth).unwrap(),
        b"{\"token\":\"secret\"}"
    );
    assert!(!system.root.join("secrets.toml").exists());
}

#[test]
fn cleanup_deletes_generated_state_and_preserves_authored_content() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let paths = LegacyStatePaths::from_home_dir(&home, InstallChannel::Stable).unwrap();
    fs::create_dir_all(paths.alan_root.join("runtime/stable/rollouts")).unwrap();
    fs::write(
        paths.alan_root.join("runtime/stable/rollouts/one.jsonl"),
        "x",
    )
    .unwrap();
    fs::create_dir_all(paths.alan_root.join("runtime/stable/memory")).unwrap();
    fs::write(
        paths.alan_root.join("runtime/stable/memory/MEMORY.md"),
        "mine",
    )
    .unwrap();
    fs::create_dir_all(paths.alan_root.join("agents/default/persona")).unwrap();
    fs::write(
        paths.alan_root.join("agents/default/persona/SOUL.md"),
        "mine",
    )
    .unwrap();
    fs::write(paths.alan_root.join("registry.json"), "{}").unwrap();
    let (system, host) = stores(temp.path(), InstallChannel::Stable);

    let report = cleanup_legacy_state(&paths, &system, &host, &[]).unwrap();

    assert!(!paths.alan_root.join("runtime/stable/rollouts").exists());
    assert!(!paths.alan_root.join("registry.json").exists());
    assert!(
        paths
            .alan_root
            .join("runtime/stable/memory/MEMORY.md")
            .is_file()
    );
    assert!(
        paths
            .alan_root
            .join("agents/default/persona/SOUL.md")
            .is_file()
    );
    assert!(
        report
            .authored_roots
            .iter()
            .any(|root| root.kind == AuthoredRootKind::MemoryStore)
    );
    assert!(
        report
            .authored_roots
            .iter()
            .any(|root| root.kind == AuthoredRootKind::Persona)
    );
}

#[cfg(unix)]
#[test]
fn cleanup_never_traverses_symlinked_runtime_parent() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let outside = temp.path().join("outside");
    fs::create_dir_all(outside.join("stable/rollouts")).unwrap();
    fs::write(outside.join("stable/rollouts/keep"), "safe").unwrap();
    let paths = LegacyStatePaths::from_home_dir(&home, InstallChannel::Stable).unwrap();
    fs::create_dir_all(&paths.alan_root).unwrap();
    symlink(&outside, paths.alan_root.join("runtime")).unwrap();
    let (system, host) = stores(temp.path(), InstallChannel::Stable);

    let error = cleanup_legacy_state(&paths, &system, &host, &[]).unwrap_err();

    assert!(error.to_string().contains("symlinked parent"));
    assert!(outside.join("stable/rollouts/keep").is_file());
}

#[cfg(unix)]
#[test]
fn cleanup_never_traverses_symlinked_legacy_root() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let outside = temp.path().join("outside");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(outside.join("runtime/stable/rollouts")).unwrap();
    fs::write(outside.join("runtime/stable/rollouts/keep"), "safe").unwrap();
    let paths = LegacyStatePaths::from_home_dir(&home, InstallChannel::Stable).unwrap();
    symlink(&outside, &paths.alan_root).unwrap();
    let (system, host) = stores(temp.path(), InstallChannel::Stable);

    let error = cleanup_legacy_state(&paths, &system, &host, &[]).unwrap_err();

    assert!(error.to_string().contains("symlinked legacy root"));
    assert!(outside.join("runtime/stable/rollouts/keep").is_file());
}

#[test]
fn changed_import_source_is_never_deleted() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("host-skill");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("SKILL.md"), "original").unwrap();
    let fingerprint = tree_fingerprint(&source).unwrap();
    fs::write(source.join("new-note.md"), "added after import").unwrap();

    let error = remove_import_source_if_unchanged(&source, &fingerprint).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("source changed after verification")
    );
    assert_eq!(
        fs::read_to_string(source.join("new-note.md")).unwrap(),
        "added after import"
    );
}

#[test]
fn overlapping_import_does_not_create_the_system_store() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("host-definition");
    fs::create_dir_all(source.join("persona")).unwrap();
    let system =
        SystemStorePaths::from_data_dir(&source, InstallChannel::Stable.descriptor().id).unwrap();

    let error = import_authored_content(
        AuthoredImportKind::AgentDefinition,
        &source,
        "default",
        false,
        &system,
    )
    .unwrap_err();

    assert!(error.to_string().contains("must not overlap"));
    assert!(!system.root.exists());
}

#[cfg(unix)]
#[test]
fn explicit_import_rejects_symlinks_without_installing() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let source = temp.path().join("host-definition");
    fs::create_dir_all(source.join("persona")).unwrap();
    fs::write(temp.path().join("outside"), "secret").unwrap();
    symlink(temp.path().join("outside"), source.join("persona/SOUL.md")).unwrap();
    let (system, _) = stores(temp.path(), InstallChannel::Stable);

    let error = import_authored_content(
        AuthoredImportKind::AgentDefinition,
        &source,
        "default",
        false,
        &system,
    )
    .unwrap_err();

    assert!(error.to_string().contains("contains a symlink"));
    let definitions = system.agent_definitions().unwrap();
    assert!(!definitions.join("default").exists());
    assert_eq!(fs::read_dir(definitions).unwrap().count(), 0);
}

#[test]
fn inspection_checks_only_fixed_and_explicit_roots() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let unrelated = temp.path().join("unrelated/repo/.alan/agents");
    fs::create_dir_all(&unrelated).unwrap();
    fs::create_dir_all(&home).unwrap();
    let paths = LegacyStatePaths::from_home_dir(&home, InstallChannel::Stable).unwrap();

    let implicit = inspect_legacy_state(&paths, &[]).unwrap();
    let explicit = inspect_legacy_state(&paths, &[temp.path().join("unrelated/repo")]).unwrap();

    assert!(implicit.authored_roots.is_empty());
    assert!(
        explicit
            .authored_roots
            .iter()
            .any(|root| root.path == unrelated)
    );
}

#[test]
fn dev_inspection_and_cleanup_use_dev_explicit_roots() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let project = temp.path().join("project");
    let dev_generated = project.join(".alan-dev/registry.json");
    let stable_generated = project.join(".alan/registry.json");
    let dev_skills = project.join(".agents-dev/skills");
    let stable_skills = project.join(".agents/skills");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(dev_generated.parent().unwrap()).unwrap();
    fs::create_dir_all(stable_generated.parent().unwrap()).unwrap();
    fs::create_dir_all(dev_skills.join("dev-skill")).unwrap();
    fs::create_dir_all(stable_skills.join("stable-skill")).unwrap();
    fs::write(&dev_generated, "{}").unwrap();
    fs::write(&stable_generated, "{}").unwrap();
    fs::write(dev_skills.join("dev-skill/SKILL.md"), "dev").unwrap();
    fs::write(stable_skills.join("stable-skill/SKILL.md"), "stable").unwrap();
    let paths = LegacyStatePaths::from_home_dir(&home, InstallChannel::Dev).unwrap();

    let inspection = inspect_legacy_state(&paths, std::slice::from_ref(&project)).unwrap();

    assert!(inspection.generated_paths.contains(&dev_generated));
    assert!(!inspection.generated_paths.contains(&stable_generated));
    assert!(
        inspection
            .authored_roots
            .iter()
            .any(|root| root.path == dev_skills)
    );
    assert!(
        !inspection
            .authored_roots
            .iter()
            .any(|root| root.path == stable_skills)
    );

    let (system, host) = stores(temp.path(), InstallChannel::Dev);
    let report =
        cleanup_legacy_state(&paths, &system, &host, std::slice::from_ref(&project)).unwrap();

    assert!(!dev_generated.exists());
    assert!(stable_generated.exists());
    assert!(report.removed_generated_paths.contains(&dev_generated));
    assert!(
        report
            .authored_roots
            .iter()
            .any(|root| root.path == dev_skills)
    );
}
