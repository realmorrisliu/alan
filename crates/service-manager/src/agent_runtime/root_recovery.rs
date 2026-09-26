//! Root lineage points to execution evidence in the Agent Runtime System Store.

use std::{io::Write, path::Path};

use alan_agent_engine::AgentProcessConfig;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

const RECORD: &str = "root-rollout.json";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RootRollout {
    version: u8,
    rollout: String,
}

pub(super) fn restore_source(process: &mut AgentProcessConfig) -> Result<()> {
    let Some(stores) = process.store_bindings.as_ref() else {
        return Ok(());
    };
    let bytes = match std::fs::read(stores.metadata.join(RECORD)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if process.recovery_rollout_path.is_none() {
                let has_evidence = match std::fs::read_dir(&stores.rollouts) {
                    Ok(mut entries) => entries.next().transpose()?.is_some(),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                    Err(error) => return Err(error).context("inspect Root recovery store"),
                };
                ensure!(
                    !has_evidence,
                    "Root rollout reference is missing at {}; select the prior Root rollout explicitly before restarting. Existing execution evidence was not replayed",
                    stores.metadata.join(RECORD).display()
                );
            }
            return Ok(());
        }
        Err(error) => return Err(error).context("read Root rollout reference"),
    };
    let record: RootRollout =
        serde_json::from_slice(&bytes).context("decode Root rollout reference")?;
    ensure!(
        record.version == 1,
        "unsupported Root rollout reference version"
    );
    let name = Path::new(&record.rollout);
    ensure!(
        name.file_name()
            .is_some_and(|file| file == name.as_os_str()),
        "Root rollout reference must name one file"
    );
    let path = stores.rollouts.join(name);
    // A missing rollout is passed to engine recovery so its established missing-
    // evidence behavior applies. Never follow a reference outside this store.
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        ensure!(
            metadata.is_file(),
            "Root rollout reference is not a regular file"
        );
    }
    process.recovery_rollout_path = Some(path);
    Ok(())
}

pub(super) fn record_source(process: &AgentProcessConfig, rollout: Option<&Path>) -> Result<()> {
    let Some(stores) = process.store_bindings.as_ref() else {
        return Ok(());
    };
    let rollout = rollout.context("Root Agent has no durable rollout to retain")?;
    ensure!(
        rollout.parent() == Some(stores.rollouts.as_path()),
        "Root rollout is outside its Agent Runtime store"
    );
    let record = RootRollout {
        version: 1,
        rollout: rollout
            .file_name()
            .and_then(|name| name.to_str())
            .context("Root rollout filename is invalid")?
            .to_string(),
    };
    std::fs::create_dir_all(&stores.metadata).context("create Agent Runtime metadata directory")?;
    let mut file = tempfile::NamedTempFile::new_in(&stores.metadata)?;
    file.write_all(&serde_json::to_vec(&record)?)?;
    file.as_file().sync_all()?;
    file.persist(stores.metadata.join(RECORD))
        .context("replace Root rollout reference")?;
    std::fs::File::open(&stores.metadata)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_engine::AgentRuntimeStoreBindings;

    #[test]
    fn root_reference_retains_only_its_own_rollout_and_rejects_invalid_records() {
        let dir = tempfile::tempdir().unwrap();
        let stores = AgentRuntimeStoreBindings {
            rollouts: dir.path().join("rollouts"),
            checkpoints: dir.path().join("checkpoints"),
            cache: dir.path().join("cache"),
            tmp: dir.path().join("tmp"),
            metadata: dir.path().join("metadata"),
        };
        std::fs::create_dir_all(&stores.rollouts).unwrap();
        let mut process = AgentProcessConfig {
            store_bindings: Some(stores.clone()),
            ..AgentProcessConfig::default()
        };
        restore_source(&mut process).unwrap();
        assert!(process.recovery_rollout_path.is_none());
        let first = stores.rollouts.join("first.jsonl");
        std::fs::write(&first, b"execution evidence").unwrap();
        assert!(restore_source(&mut process).is_err());
        process.recovery_rollout_path = Some(first.clone());
        restore_source(&mut process).unwrap();
        record_source(&process, Some(&first)).unwrap();
        restore_source(&mut process).unwrap();
        assert_eq!(process.recovery_rollout_path.as_ref(), Some(&first));
        let second = stores.rollouts.join("second.jsonl");
        record_source(&process, Some(&second)).unwrap();
        restore_source(&mut process).unwrap();
        assert_eq!(process.recovery_rollout_path.as_ref(), Some(&second));
        assert!(record_source(&process, Some(&dir.path().join("outside.jsonl"))).is_err());
        assert!(record_source(&process, None).is_err());
        for record in [
            r#"{"version":2,"rollout":"first.jsonl"}"#,
            r#"{"version":1,"rollout":"../outside.jsonl"}"#,
            r#"{"version":1,"rollout":"/tmp/outside.jsonl"}"#,
            r#"{"version":1,"rollout":"."}"#,
            r#"{"version":1,"rollout":""}"#,
            r#"{"version":1,"rollout":"first.jsonl","unexpected":true}"#,
            "broken",
        ] {
            std::fs::write(stores.metadata.join(RECORD), record).unwrap();
            assert!(restore_source(&mut process).is_err(), "{record}");
        }
        #[cfg(unix)]
        {
            let link = stores.rollouts.join("symlink.jsonl");
            std::os::unix::fs::symlink(&first, &link).unwrap();
            record_source(&process, Some(&link)).unwrap();
            assert!(restore_source(&mut process).is_err());
        }
    }
}
