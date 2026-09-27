//! Root Agent continuity selection in the Agent Runtime Service System Store.

use alan_agent_engine::AgentProcessConfig;
use anyhow::{Context, Result, bail, ensure};
use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

const CURRENT: &str = "root-rollout";

#[derive(Clone, Debug, Default)]
pub(super) enum RootRolloutState {
    #[default]
    NotStarted,
    NoDurableRollout,
    Durable(PathBuf),
}

pub(super) fn select(
    config: &mut AgentProcessConfig,
    resume_previous: bool,
    current_rollout: &mut RootRolloutState,
) -> Result<()> {
    if config.recovery_rollout_path.is_some() {
        return Ok(());
    }
    let Some(stores) = config.store_bindings.as_ref() else {
        return Ok(());
    };
    let pin_instance_state = matches!(current_rollout, RootRolloutState::NotStarted);
    let path = match current_rollout {
        RootRolloutState::Durable(path) => path.clone(),
        RootRolloutState::NoDurableRollout => return Ok(()),
        RootRolloutState::NotStarted if !resume_previous => return Ok(()),
        RootRolloutState::NotStarted => {
            let name = match fs::read_to_string(stores.metadata.join(CURRENT)) {
                Ok(name) => name,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    bail!("no selected Root Agent rollout is available to resume")
                }
                Err(error) => return Err(error).context("read Root Agent recovery selection"),
            };
            validate_filename(Path::new(&name))?;
            stores.rollouts.join(name)
        }
    };
    let name = path
        .strip_prefix(&stores.rollouts)
        .context("Root Agent recovery rollout store mismatch")?;
    validate_filename(name)?;
    ensure!(
        fs::metadata(&path)
            .context("selected Root Agent rollout is unavailable")?
            .is_file(),
        "selected Root Agent rollout is not a file"
    );
    config.recovery_rollout_path = Some(path.clone());
    if pin_instance_state {
        *current_rollout = RootRolloutState::Durable(path);
    }
    Ok(())
}

pub(super) fn publish(config: &AgentProcessConfig, rollout: Option<&Path>) -> Result<()> {
    let (Some(stores), Some(rollout)) = (config.store_bindings.as_ref(), rollout) else {
        return Ok(());
    };
    let name = rollout
        .strip_prefix(&stores.rollouts)
        .context("Root Agent rollout store mismatch")?;
    validate_filename(name)?;
    let name = name
        .to_str()
        .context("Root Agent rollout filename is not UTF-8")?;
    fs::File::open(rollout)?
        .sync_all()
        .context("sync selected Root Agent rollout")?;
    sync_directory_ancestors(&stores.rollouts)?;
    fs::create_dir_all(&stores.metadata)?;
    sync_directory_ancestors(&stores.metadata)?;
    let mut temporary = tempfile::NamedTempFile::new_in(&stores.metadata)?;
    temporary.write_all(name.as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(stores.metadata.join(CURRENT))
        .context("publish Root Agent recovery selection")?;
    fs::File::open(&stores.metadata)?.sync_all()?;
    Ok(())
}

// The Host may have just created any ancestor of these store directories.
fn sync_directory_ancestors(path: &Path) -> Result<()> {
    for directory in fs::canonicalize(path)?.ancestors() {
        fs::File::open(directory)?
            .sync_all()
            .with_context(|| format!("sync Root Agent store directory {}", directory.display()))?;
    }
    Ok(())
}

fn validate_filename(name: &Path) -> Result<()> {
    let mut components = name.components();
    ensure!(
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none(),
        "Root Agent recovery selection must be one rollout filename"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_selection_is_opt_in_atomic_and_rejects_missing_records() {
        let temp = tempfile::tempdir().unwrap();
        let stores = alan_agent_engine::AgentRuntimeStoreBindings {
            rollouts: temp.path().join("rollouts"),
            metadata: temp.path().join("metadata"),
            checkpoints: temp.path().join("checkpoints"),
            cache: temp.path().join("cache"),
            tmp: temp.path().join("tmp"),
        };
        let mut config = AgentProcessConfig {
            store_bindings: Some(stores.clone()),
            ..Default::default()
        };
        let mut not_started = RootRolloutState::NotStarted;
        select(&mut config, false, &mut not_started).unwrap();
        assert!(config.recovery_rollout_path.is_none());
        assert!(select(&mut config, true, &mut not_started).is_err());
        fs::create_dir_all(&stores.rollouts).unwrap();
        for name in ["first.jsonl", "second.jsonl"] {
            let path = stores.rollouts.join(name);
            fs::write(&path, "test evidence").unwrap();
            publish(&config, Some(&path)).unwrap();
            config.recovery_rollout_path = None;
            let mut new_instance = RootRolloutState::NotStarted;
            select(&mut config, false, &mut new_instance).unwrap();
            assert!(config.recovery_rollout_path.is_none());
            select(&mut config, true, &mut new_instance).unwrap();
            assert_eq!(config.recovery_rollout_path.as_ref(), Some(&path));
        }
        assert!(publish(&config, Some(&stores.rollouts.join("unavailable.jsonl"))).is_err());
        assert_eq!(
            fs::read_to_string(stores.metadata.join(CURRENT)).unwrap(),
            "second.jsonl"
        );
        config.recovery_rollout_path = Some(stores.rollouts.join("explicit.jsonl"));
        select(&mut config, false, &mut not_started).unwrap();
        assert_eq!(
            config.recovery_rollout_path,
            Some(stores.rollouts.join("explicit.jsonl"))
        );
        config.recovery_rollout_path = None;
        fs::remove_file(stores.rollouts.join("second.jsonl")).unwrap();
        assert!(select(&mut config, true, &mut not_started).is_err());
        for invalid in ["", "../first.jsonl", "/outside.jsonl"] {
            fs::write(stores.metadata.join(CURRENT), invalid).unwrap();
            assert!(select(&mut config, true, &mut not_started).is_err());
        }
        let mut ephemeral = AgentProcessConfig::default();
        select(&mut ephemeral, true, &mut not_started).unwrap();
        assert!(ephemeral.recovery_rollout_path.is_none());
    }

    #[test]
    fn current_instance_rollout_precedes_and_new_instance_pins_channel_selector() {
        let temp = tempfile::tempdir().unwrap();
        let stores = alan_agent_engine::AgentRuntimeStoreBindings {
            rollouts: temp.path().join("rollouts"),
            metadata: temp.path().join("metadata"),
            checkpoints: temp.path().join("checkpoints"),
            cache: temp.path().join("cache"),
            tmp: temp.path().join("tmp"),
        };
        fs::create_dir_all(&stores.rollouts).unwrap();
        fs::create_dir_all(&stores.metadata).unwrap();
        let current = stores.rollouts.join("session-a.jsonl");
        let selected = stores.rollouts.join("session-b.jsonl");
        fs::write(&current, "current instance").unwrap();
        fs::write(&selected, "channel selector").unwrap();
        fs::write(stores.metadata.join(CURRENT), "session-b.jsonl").unwrap();
        let mut config = AgentProcessConfig {
            store_bindings: Some(stores.clone()),
            ..Default::default()
        };

        let mut current_instance = RootRolloutState::Durable(current.clone());
        select(&mut config, true, &mut current_instance).unwrap();

        assert_eq!(config.recovery_rollout_path, Some(current));

        config.recovery_rollout_path = None;
        let mut new_instance = RootRolloutState::NotStarted;
        select(&mut config, true, &mut new_instance).unwrap();
        assert_eq!(config.recovery_rollout_path, Some(selected.clone()));
        fs::write(stores.metadata.join(CURRENT), "session-a.jsonl").unwrap();
        config.recovery_rollout_path = None;
        select(&mut config, true, &mut new_instance).unwrap();
        assert_eq!(config.recovery_rollout_path, Some(selected));
    }

    #[test]
    fn started_instance_without_durable_rollout_never_uses_channel_selector() {
        let temp = tempfile::tempdir().unwrap();
        let stores = alan_agent_engine::AgentRuntimeStoreBindings {
            rollouts: temp.path().join("rollouts"),
            metadata: temp.path().join("metadata"),
            checkpoints: temp.path().join("checkpoints"),
            cache: temp.path().join("cache"),
            tmp: temp.path().join("tmp"),
        };
        fs::create_dir_all(&stores.rollouts).unwrap();
        fs::create_dir_all(&stores.metadata).unwrap();
        fs::write(stores.rollouts.join("other-session.jsonl"), "other session").unwrap();
        fs::write(stores.metadata.join(CURRENT), "other-session.jsonl").unwrap();
        let mut config = AgentProcessConfig {
            store_bindings: Some(stores),
            ..Default::default()
        };

        select(&mut config, true, &mut RootRolloutState::NoDurableRollout).unwrap();

        assert!(config.recovery_rollout_path.is_none());
    }

    #[tokio::test]
    async fn explicit_resume_requires_durable_store_bindings() {
        let mut config = crate::ServiceManagerConfig::ephemeral(
            "test",
            AgentProcessConfig::default(),
            crate::ProcessLaunchContext::root(),
            alan_agent_engine::LlmClient::new(alan_llm::MockLlmProvider::new()),
            alan_agent_engine::tools::ToolRegistry::new(),
        );
        config.resume_root = true;
        let error = match crate::ServiceManager::boot(config).await {
            Ok(manager) => {
                manager.shutdown().await.unwrap();
                panic!("explicit resume without durable stores must fail");
            }
            Err(error) => error,
        };
        assert!(format!("{error:#}").contains("recovery requires durable store bindings"));
    }

    #[tokio::test]
    async fn overlapping_root_sessions_restart_from_their_own_rollouts() {
        use alan_agent_engine::{RolloutItem, RolloutRecorder};
        let temp = tempfile::tempdir().unwrap();
        let stores = alan_agent_engine::AgentRuntimeStoreBindings {
            rollouts: temp.path().join("rollouts"),
            metadata: temp.path().join("metadata"),
            checkpoints: temp.path().join("checkpoints"),
            cache: temp.path().join("cache"),
            tmp: temp.path().join("tmp"),
        };
        fs::create_dir_all(&stores.rollouts).unwrap();
        let config = AgentProcessConfig {
            store_bindings: Some(stores.clone()),
            ..Default::default()
        };
        let previous = RolloutRecorder::new_in_dir("/proc/old", "mock", &stores.rollouts)
            .await
            .unwrap();
        previous
            .record_tape_message(&alan_agent_engine::tape::Message::user(
                "previous invocation",
            ))
            .await
            .unwrap();
        publish(&config, Some(previous.path())).unwrap();

        let provider = alan_llm::MockLlmProvider::new();
        let probe = provider.clone();
        let manager = crate::ServiceManager::boot(crate::ServiceManagerConfig::ephemeral(
            "test",
            config.clone(),
            crate::ProcessLaunchContext::root(),
            alan_agent_engine::LlmClient::new(provider),
            alan_agent_engine::tools::ToolRegistry::new(),
        ))
        .await
        .unwrap();
        let selected = fs::read_to_string(stores.metadata.join(CURRENT)).unwrap();
        assert_ne!(stores.rollouts.join(&selected), *previous.path());
        let items = RolloutRecorder::load_history(&stores.rollouts.join(selected))
            .await
            .unwrap();
        assert!(!items.iter().any(|item| matches!(item, RolloutItem::Message(record)
            if record.message.as_ref().is_some_and(|message| message.text_content() == "previous invocation"))));

        let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
        let shell = alan_shell::Shell::new(alan_ap::InProcessTransport::new(namespace));
        shell
            .write("/agent/root/io/input", b"work in fresh invocation")
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let activity: serde_json::Value = serde_json::from_slice(
                    &shell.cat("/agent/root/machine/ui/activity").await.unwrap(),
                )
                .unwrap();
                if !probe.recorded_requests().is_empty() && activity["state"] == "idle" {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        let provider_b = alan_llm::MockLlmProvider::new();
        let probe_b = provider_b.clone();
        let manager_b = crate::ServiceManager::boot(crate::ServiceManagerConfig::ephemeral(
            "test",
            config,
            crate::ProcessLaunchContext::root(),
            alan_agent_engine::LlmClient::new(provider_b),
            alan_agent_engine::tools::ToolRegistry::new(),
        ))
        .await
        .unwrap();
        let (_, _, namespace_b) = manager_b.local_entry().create_and_handoff().await.unwrap();
        let shell_b = alan_shell::Shell::new(alan_ap::InProcessTransport::new(namespace_b));
        shell_b
            .write("/agent/root/io/input", b"work in second invocation")
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let activity: serde_json::Value = serde_json::from_slice(
                    &shell_b
                        .cat("/agent/root/machine/ui/activity")
                        .await
                        .unwrap(),
                )
                .unwrap();
                if !probe_b.recorded_requests().is_empty() && activity["state"] == "idle" {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        let old_pid = manager.root_pid();
        manager.terminate_unit("root-agent", 0).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let pid = manager.root_pid();
                if pid.0 != 0 && pid != old_pid {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let resumed = fs::read_to_string(stores.metadata.join(CURRENT)).unwrap();
        let items = RolloutRecorder::load_history(&stores.rollouts.join(resumed))
            .await
            .unwrap();
        assert!(items.iter().any(|item| matches!(item, RolloutItem::Message(record)
            if record.message.as_ref().is_some_and(|message| message.text_content() == "work in fresh invocation"))));
        assert!(!items.iter().any(|item| matches!(item, RolloutItem::Message(record)
            if record.message.as_ref().is_some_and(|message| message.text_content() == "previous invocation"))));

        let old_pid_b = manager_b.root_pid();
        manager_b.terminate_unit("root-agent", 0).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let pid = manager_b.root_pid();
                if pid.0 != 0 && pid != old_pid_b {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let resumed_b = fs::read_to_string(stores.metadata.join(CURRENT)).unwrap();
        let items_b = RolloutRecorder::load_history(&stores.rollouts.join(resumed_b))
            .await
            .unwrap();
        assert!(items_b.iter().any(|item| matches!(item, RolloutItem::Message(record)
            if record.message.as_ref().is_some_and(|message| message.text_content() == "work in second invocation"))));
        assert!(!items_b.iter().any(|item| matches!(item, RolloutItem::Message(record)
            if record.message.as_ref().is_some_and(|message| message.text_content() == "work in fresh invocation"))));

        manager_b.shutdown().await.unwrap();
        manager.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn explicit_resume_and_supervised_restart_select_latest_durable_history() {
        use alan_agent_engine::{LlmClient, RolloutItem, RolloutRecorder, tools::ToolRegistry};
        use alan_ap::InProcessTransport;
        use std::time::Duration;
        let temp = tempfile::tempdir().unwrap();
        let stores = alan_agent_engine::AgentRuntimeStoreBindings {
            rollouts: temp.path().join("rollouts"),
            metadata: temp.path().join("metadata"),
            checkpoints: temp.path().join("checkpoints"),
            cache: temp.path().join("cache"),
            tmp: temp.path().join("tmp"),
        };
        let config = AgentProcessConfig {
            store_bindings: Some(stores.clone()),
            ..Default::default()
        };
        let seed = RolloutRecorder::new_in_dir("/proc/old", "mock", &stores.rollouts)
            .await
            .unwrap();
        seed.record_tape_message(&alan_agent_engine::tape::Message::user("previous lifetime"))
            .await
            .unwrap();
        publish(&config, Some(seed.path())).unwrap();
        let provider = alan_llm::MockLlmProvider::new();
        let probe = provider.clone();
        let mut manager_config = crate::ServiceManagerConfig::ephemeral(
            "test",
            config,
            crate::ProcessLaunchContext::root(),
            LlmClient::new(provider),
            ToolRegistry::new(),
        );
        manager_config.resume_root = true;
        let manager = crate::ServiceManager::boot(manager_config).await.unwrap();
        let first = fs::read_to_string(stores.metadata.join(CURRENT)).unwrap();
        assert_ne!(stores.rollouts.join(&first), *seed.path());
        let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
        let shell = alan_shell::Shell::new(InProcessTransport::new(namespace));
        shell
            .write("/agent/root/io/input", b"work since recovery")
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let activity: serde_json::Value = serde_json::from_slice(
                    &shell.cat("/agent/root/machine/ui/activity").await.unwrap(),
                )
                .unwrap();
                if !probe.recorded_requests().is_empty() && activity["state"] == "idle" {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let old_pid = manager.root_pid();
        manager.terminate_unit("root-agent", 0).await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let pid = manager.root_pid();
                if pid.0 != 0 && pid != old_pid {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let second = fs::read_to_string(stores.metadata.join(CURRENT)).unwrap();
        assert_ne!(first, second);
        let items = RolloutRecorder::load_history(&stores.rollouts.join(second))
            .await
            .unwrap();
        for expected in ["previous lifetime", "work since recovery"] {
            assert!(items.iter().any(|item| matches!(item, RolloutItem::Message(record)
                if record.message.as_ref().is_some_and(|message| message.text_content() == expected))));
        }
        manager.shutdown().await.unwrap();
    }
    #[tokio::test]
    async fn failed_root_recovery_preserves_the_selected_source() {
        let temp = tempfile::tempdir().unwrap();
        let stores = alan_agent_engine::AgentRuntimeStoreBindings {
            rollouts: temp.path().join("rollouts"),
            metadata: temp.path().join("metadata"),
            checkpoints: temp.path().join("checkpoints"),
            cache: temp.path().join("cache"),
            tmp: temp.path().join("tmp"),
        };
        let config = AgentProcessConfig {
            store_bindings: Some(stores.clone()),
            ..Default::default()
        };
        fs::create_dir_all(&stores.rollouts).unwrap();
        let source = stores.rollouts.join("selected.jsonl");
        fs::write(&source, "invalid rollout\n").unwrap();
        publish(&config, Some(&source)).unwrap();
        let mut manager_config = crate::ServiceManagerConfig::ephemeral(
            "test",
            config,
            crate::ProcessLaunchContext::root(),
            alan_agent_engine::LlmClient::new(alan_llm::MockLlmProvider::new()),
            alan_agent_engine::tools::ToolRegistry::new(),
        );
        manager_config.resume_root = true;
        let result = crate::ServiceManager::boot(manager_config).await;
        let error = match result {
            Ok(manager) => {
                manager.shutdown().await.unwrap();
                panic!("invalid selected recovery must fail startup");
            }
            Err(error) => error,
        };
        assert!(
            format!("{error:#}").contains("Failed to recover selected"),
            "{error:#}"
        );
        assert_eq!(
            fs::read_to_string(stores.metadata.join(CURRENT)).unwrap(),
            "selected.jsonl"
        );
        assert_eq!(fs::read_to_string(source).unwrap(), "invalid rollout\n");
        assert_eq!(fs::read_dir(stores.rollouts).unwrap().count(), 1);
    }
}
