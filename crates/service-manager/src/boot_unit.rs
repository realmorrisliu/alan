use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

const MAX_TIMEOUT_MS: u64 = 300_000;
const MAX_RESTART_ATTEMPTS: u32 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    Never,
    OnFailure,
    Always,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootDescriptor {
    pub number: u32,
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MountAccess {
    Read,
    Write,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootMount {
    pub path: String,
    pub source: String,
    pub access: MountAccess,
}

/// A deliberately bounded system Boot Unit. Unknown fields are rejected.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootUnit {
    pub name: String,
    pub executable: String,
    #[serde(default)]
    pub descriptors: Vec<BootDescriptor>,
    #[serde(default)]
    pub mounts: Vec<BootMount>,
    #[serde(default)]
    pub after: Vec<String>,
    pub required: bool,
    pub timeout_ms: u64,
    pub restart: RestartPolicy,
    pub restart_limit: u32,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub stable_reset_ms: u64,
    #[serde(default)]
    pub published_handles: Vec<String>,
}

impl BootUnit {
    pub fn parse(document: &str) -> Result<Self> {
        let unit: Self = toml::from_str(document)?;
        unit.validate()?;
        Ok(unit)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            valid_name(&self.name),
            "invalid Boot Unit name `{}`",
            self.name
        );
        ensure!(
            valid_absolute_path(&self.executable),
            "Boot Unit `{}` executable must be an absolute Alan OS path",
            self.name
        );
        ensure!(
            self.timeout_ms > 0 && self.timeout_ms <= MAX_TIMEOUT_MS,
            "Boot Unit `{}` timeout is outside the supported range",
            self.name
        );
        ensure!(
            self.restart_limit <= MAX_RESTART_ATTEMPTS,
            "Boot Unit `{}` restart budget is too large",
            self.name
        );
        ensure!(
            self.initial_backoff_ms > 0
                && self.initial_backoff_ms <= self.max_backoff_ms
                && self.max_backoff_ms <= self.timeout_ms,
            "Boot Unit `{}` backoff is invalid",
            self.name
        );
        ensure!(
            self.stable_reset_ms >= self.initial_backoff_ms,
            "Boot Unit `{}` stable reset window is invalid",
            self.name
        );
        unique_valid_names(&self.name, "dependency", &self.after)?;
        ensure!(
            !self.after.iter().any(|dependency| dependency == &self.name),
            "Boot Unit `{}` depends on itself",
            self.name
        );
        unique_valid_names(&self.name, "published handle", &self.published_handles)?;
        let mut descriptor_numbers = BTreeSet::new();
        let mut descriptor_paths = BTreeSet::new();
        for descriptor in &self.descriptors {
            ensure!(
                valid_absolute_path(&descriptor.path),
                "Boot Unit `{}` descriptor path is invalid",
                self.name
            );
            ensure!(
                descriptor.number >= 3,
                "Boot Unit `{}` descriptor number is reserved",
                self.name
            );
            ensure!(
                descriptor_numbers.insert(descriptor.number),
                "Boot Unit `{}` repeats descriptor number {}",
                self.name,
                descriptor.number
            );
            ensure!(
                descriptor_paths.insert(descriptor.path.as_str()),
                "Boot Unit `{}` repeats descriptor path `{}`",
                self.name,
                descriptor.path
            );
        }
        let mut mount_paths = BTreeSet::new();
        for mount in &self.mounts {
            ensure!(
                valid_absolute_path(&mount.path) && valid_absolute_path(&mount.source),
                "Boot Unit `{}` mount is invalid",
                self.name
            );
            ensure!(
                mount_paths.insert(mount.path.as_str()),
                "Boot Unit `{}` repeats mount path `{}`",
                self.name,
                mount.path
            );
        }
        Ok(())
    }

    pub fn backoff_ms(&self, failed_attempts: u32) -> u64 {
        let shift = failed_attempts.saturating_sub(1).min(62);
        self.initial_backoff_ms
            .saturating_mul(1_u64 << shift)
            .min(self.max_backoff_ms)
    }

    pub fn should_restart(&self, exit_code: i32) -> bool {
        match self.restart {
            RestartPolicy::Never => false,
            RestartPolicy::OnFailure => exit_code != 0,
            RestartPolicy::Always => true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BootManifest {
    units: BTreeMap<String, BootUnit>,
    order: Vec<String>,
}

impl BootManifest {
    pub fn parse(documents: impl IntoIterator<Item = &'static str>) -> Result<Self> {
        let mut units = BTreeMap::new();
        for document in documents {
            let unit = BootUnit::parse(document)?;
            ensure!(
                units.insert(unit.name.clone(), unit).is_none(),
                "duplicate Boot Unit name"
            );
        }
        ensure!(!units.is_empty(), "Boot Unit tree is empty");
        for unit in units.values() {
            for dependency in &unit.after {
                ensure!(
                    units.contains_key(dependency),
                    "Boot Unit `{}` references unknown dependency `{dependency}`",
                    unit.name
                );
            }
        }
        let order = dependency_order(&units)?;
        Ok(Self { units, order })
    }

    pub fn system() -> Result<Self> {
        Self::parse([
            include_str!("../system/boot/route.toml"),
            include_str!("../system/boot/connection.toml"),
            include_str!("../system/boot/package.toml"),
            include_str!("../system/boot/agent-runtime.toml"),
            include_str!("../system/boot/host-mount.toml"),
            include_str!("../system/boot/local-entry.toml"),
            include_str!("../system/boot/root-agent.toml"),
        ])
    }

    pub fn ordered(&self) -> impl Iterator<Item = &BootUnit> {
        self.order.iter().map(|name| &self.units[name])
    }

    pub fn get(&self, name: &str) -> Option<&BootUnit> {
        self.units.get(name)
    }

    pub fn documents() -> [(&'static str, &'static str); 7] {
        [
            ("route.toml", include_str!("../system/boot/route.toml")),
            (
                "connection.toml",
                include_str!("../system/boot/connection.toml"),
            ),
            ("package.toml", include_str!("../system/boot/package.toml")),
            (
                "agent-runtime.toml",
                include_str!("../system/boot/agent-runtime.toml"),
            ),
            (
                "host-mount.toml",
                include_str!("../system/boot/host-mount.toml"),
            ),
            (
                "local-entry.toml",
                include_str!("../system/boot/local-entry.toml"),
            ),
            (
                "root-agent.toml",
                include_str!("../system/boot/root-agent.toml"),
            ),
        ]
    }
}

fn dependency_order(units: &BTreeMap<String, BootUnit>) -> Result<Vec<String>> {
    let mut pending = units.keys().cloned().collect::<BTreeSet<_>>();
    let mut ready = BTreeSet::new();
    let mut order = Vec::with_capacity(units.len());
    while !pending.is_empty() {
        let next = pending
            .iter()
            .find(|name| {
                units[*name]
                    .after
                    .iter()
                    .all(|dependency| ready.contains(dependency))
            })
            .cloned();
        let Some(next) = next else {
            bail!("Boot Unit dependency cycle detected");
        };
        pending.remove(&next);
        ready.insert(next.clone());
        order.push(next);
    }
    Ok(order)
}

fn unique_valid_names(owner: &str, label: &str, names: &[String]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for name in names {
        ensure!(
            valid_name(name),
            "Boot Unit `{owner}` has invalid {label} `{name}`"
        );
        ensure!(
            seen.insert(name),
            "Boot Unit `{owner}` repeats {label} `{name}`"
        );
    }
    Ok(())
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_absolute_path(path: &str) -> bool {
    path.starts_with('/')
        && path != "/"
        && !path
            .split('/')
            .any(|component| matches!(component, "." | ".."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(name: &str, after: &[&str]) -> String {
        format!(
            r#"name = "{name}"
executable = "/bin/{name}"
after = [{}]
required = true
timeout_ms = 1000
restart = "on-failure"
restart_limit = 3
initial_backoff_ms = 10
max_backoff_ms = 100
stable_reset_ms = 1000
published_handles = []
"#,
            after
                .iter()
                .map(|name| format!("\"{name}\""))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    #[test]
    fn rejects_unknown_fields_and_cycles() {
        let unknown = format!("{}script = \"oops\"\n", unit("a", &[]));
        assert!(BootUnit::parse(&unknown).is_err());

        let a = Box::leak(unit("a", &["b"]).into_boxed_str());
        let b = Box::leak(unit("b", &["a"]).into_boxed_str());
        assert!(BootManifest::parse([a as &'static str, b as &'static str]).is_err());
    }

    #[test]
    fn system_manifest_is_ordered_and_bounded() {
        let manifest = BootManifest::system().unwrap();
        let names = manifest
            .ordered()
            .map(|unit| unit.name.as_str())
            .collect::<Vec<_>>();
        assert!(
            names.iter().position(|name| *name == "connection").unwrap()
                < names.iter().position(|name| *name == "root-agent").unwrap()
        );
        assert_eq!(
            manifest.get("root-agent").unwrap().restart,
            RestartPolicy::Always
        );
        assert_eq!(manifest.get("root-agent").unwrap().backoff_ms(20), 1_000);
        assert!(manifest.ordered().all(|unit| !unit.mounts.is_empty()));
        assert_eq!(
            manifest.get("root-agent").unwrap().descriptors,
            vec![
                BootDescriptor {
                    number: 3,
                    path: "/lib/agents/root".to_string(),
                },
                BootDescriptor {
                    number: 4,
                    path: "/memory".to_string(),
                },
            ]
        );
    }
}
