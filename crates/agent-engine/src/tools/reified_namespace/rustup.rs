//! Bounded Host inspection of standard Rustup proxy/runtime installations.
use super::plan::{canonicalize_existing_host_path, contains_parent_component};
use super::{
    ReifiedExecutionSubstrateMount, ReifiedMountSource, ReifiedNamespacePlanInput,
    ReifiedRustupEnvironment,
};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

// shortcut: standard Rustup ELF layouts only; extend for a recorded linked/custom runtime task.
const PROXIES: &[&str] = &[
    "rustup",
    "cargo",
    "rustc",
    "rustdoc",
    "rustfmt",
    "cargo-fmt",
    "clippy-driver",
    "cargo-clippy",
    "cargo-miri",
    "rls",
    "rust-analyzer",
    "rust-gdb",
    "rust-gdbgui",
    "rust-lldb",
];

pub(super) fn discover(
    path: &str,
    home: &Path,
    toolchain_override: Option<String>,
    input: Option<(&ReifiedNamespacePlanInput, &[Vec<String>])>,
) -> Result<Option<ReifiedRustupEnvironment>, String> {
    let mut proxies = Vec::new();
    let system = super::default_execution_substrate();
    for entry in std::env::split_paths(path) {
        if system
            .iter()
            .any(|mount| entry.starts_with(&mount.namespace_path))
        {
            continue;
        }
        if !entry.is_absolute() || contains_parent_component(&entry) {
            return Err(format!("unsafe PATH entry {}", entry.display()));
        }
        if !entry.join("rustup").exists() {
            continue;
        }
        let source = canonicalize_existing_host_path(&entry);
        validate_proxies(&source)?;
        if !proxies
            .iter()
            .any(|mount: &ReifiedExecutionSubstrateMount| mount.namespace_path == entry)
        {
            proxies.push(ReifiedExecutionSubstrateMount::new(entry, source));
        }
    }
    if proxies.is_empty() {
        return Ok(None);
    }
    if !home.is_absolute() || contains_parent_component(home) || home == Path::new("/") {
        return Err("unsafe Rustup home metadata path".into());
    }
    let home =
        std::fs::canonicalize(home).map_err(|error| format!("Rustup home unavailable: {error}"))?;
    let source = home.join("settings.toml");
    let bytes = read_metadata(&source)?;
    let value: toml::Value =
        toml::from_str(&bytes).map_err(|error| format!("invalid Rustup settings: {error}"))?;
    if value.get("version").and_then(toml::Value::as_str) != Some("12") {
        return Err("unsupported Rustup settings version".into());
    }
    let mut settings = toml::Table::new();
    for key in [
        "version",
        "default_toolchain",
        "default_host_triple",
        "profile",
    ] {
        if let Some(value) = value.get(key) {
            let text = value
                .as_str()
                .ok_or_else(|| format!("invalid Rustup {key}"))?;
            if !safe_name(text) {
                return Err(format!("unsafe Rustup {key}"));
            }
            settings.insert(key.into(), value.clone());
        }
    }
    let mut overrides = toml::Table::new();
    if let Some(input) = input
        && let Some(values) = value.get("overrides")
    {
        let values = values.as_table().ok_or("invalid Rustup overrides")?;
        for (path, selector) in values {
            let path = Path::new(path);
            if input.0.cwd.starts_with(path)
                || input
                    .0
                    .declarations
                    .iter()
                    .any(|mount| match &mount.source {
                        ReifiedMountSource::Host(root) => {
                            root.starts_with(path) || path.starts_with(root)
                        }
                        ReifiedMountSource::Virtual => false,
                    })
            {
                if !path.is_absolute() || contains_parent_component(path) {
                    return Err("unsafe Rustup directory override".into());
                }
                let selector = selector
                    .as_str()
                    .ok_or("invalid Rustup directory override")?;
                overrides.insert(
                    path.to_str().ok_or("non-UTF-8 Rustup override")?.into(),
                    selector.into(),
                );
            }
        }
    }
    settings.insert("overrides".into(), overrides.into());
    let runtime_parent = home.join("toolchains");
    if std::fs::symlink_metadata(&runtime_parent)
        .map_err(|error| format!("Rustup runtimes unavailable: {error}"))?
        .file_type()
        .is_symlink()
    {
        return Err("Rustup toolchains directory is an unsafe alias".into());
    }
    let mut toolchains = Vec::new();
    for entry in std::fs::read_dir(&runtime_parent).map_err(|error| error.to_string())? {
        if toolchains.len() >= 64 {
            return Err("Rustup runtime catalog exceeds inspection bound".into());
        }
        let root = entry.map_err(|error| error.to_string())?.path();
        validate_runtime(&root)?;
        toolchains.push(ReifiedExecutionSubstrateMount::new(&root, &root));
    }
    toolchains.sort_by(|left, right| left.host_path.cmp(&right.host_path));
    let mut environment = ReifiedRustupEnvironment {
        settings: toml::to_string(&settings).map_err(|error| error.to_string())?,
        toolchain_override,
        proxy_mounts: proxies,
        toolchain_mounts: toolchains,
        metadata_hashes: vec![(source, digest(&bytes))],
        executable_hashes: Vec::new(),
    };
    for mount in &environment.toolchain_mounts {
        let manifest = mount.host_path.join("lib/rustlib/multirust-config.toml");
        environment
            .metadata_hashes
            .push((manifest.clone(), digest(&read_metadata(&manifest)?)));
    }
    for path in environment
        .proxy_mounts
        .iter()
        .map(|mount| mount.host_path.join("rustup"))
        .chain(environment.toolchain_mounts.iter().flat_map(|mount| {
            ["cargo", "rustc", "rustdoc"].map(|name| mount.host_path.join("bin").join(name))
        }))
    {
        environment
            .executable_hashes
            .push((path.clone(), executable_digest(&path)?));
    }
    if let Some((input, commands)) = input {
        for words in commands {
            let Some(name) = words
                .first()
                .and_then(|word| Path::new(word).file_name())
                .and_then(|name| name.to_str())
            else {
                continue;
            };
            if PROXIES.contains(&name) && name != "rustup" {
                let selector =
                    if let Some(selector) = words.get(1).and_then(|word| word.strip_prefix('+')) {
                        selector.to_string()
                    } else {
                        active_selector(input, &mut environment)?
                    };
                let root = runtime_for_selector(&selector, &environment)?
                    .host_path
                    .clone();
                let subcommand = words
                    .get(if words.get(1).is_some_and(|word| word.starts_with('+')) {
                        2
                    } else {
                        1
                    })
                    .map(String::as_str);
                let required: &[&str] = match (name, subcommand) {
                    ("cargo", Some("fmt")) | ("cargo-fmt", _) => &["cargo-fmt", "rustfmt"],
                    ("cargo", Some("clippy")) | ("cargo-clippy", _) => {
                        &["cargo-clippy", "clippy-driver"]
                    }
                    _ => &[name],
                };
                for name in required {
                    let path = root.join("bin").join(name);
                    validate_executable(&path, &root)?;
                    if !environment
                        .executable_hashes
                        .iter()
                        .any(|(existing, _)| *existing == path)
                    {
                        environment
                            .executable_hashes
                            .push((path.clone(), executable_digest(&path)?));
                    }
                }
            } else if name == "rustup" && words.get(1).is_some_and(|word| word == "run") {
                validate_selector(
                    words.get(2).ok_or("missing Rustup run selector")?,
                    &environment,
                )?;
            }
        }
    }
    Ok(Some(environment))
}

fn active_selector(
    input: &ReifiedNamespacePlanInput,
    environment: &mut ReifiedRustupEnvironment,
) -> Result<String, String> {
    if let Some(selector) = &environment.toolchain_override {
        return Ok(selector.clone());
    }
    let settings: toml::Value = toml::from_str(&environment.settings)
        .map_err(|error| format!("invalid private Rustup settings: {error}"))?;
    for directory in input.cwd.ancestors() {
        if let Some(selector) = settings
            .get("overrides")
            .and_then(|values| values.get(directory.to_str()?))
            .and_then(toml::Value::as_str)
        {
            return Ok(selector.into());
        }
        for name in ["rust-toolchain", "rust-toolchain.toml"] {
            let file = directory.join(name);
            match std::fs::symlink_metadata(&file) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(format!("Rust toolchain file unavailable: {error}")),
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err("Rust toolchain file is an unsupported alias".into());
                }
                Ok(_) => {}
            }
            let canonical = std::fs::canonicalize(&file).map_err(|error| error.to_string())?;
            if !input.declarations.iter().any(|mount| match &mount.source {
                ReifiedMountSource::Host(root) => {
                    canonical.starts_with(canonicalize_existing_host_path(root))
                }
                ReifiedMountSource::Virtual => false,
            }) {
                return Err("Rust toolchain file is outside delegated project authority".into());
            }
            let bytes = read_metadata(&canonical)?;
            environment.metadata_hashes.push((file, digest(&bytes)));
            if name == "rust-toolchain" && !bytes.trim_start().starts_with('[') {
                if !bytes.is_ascii() {
                    return Err("non-ASCII legacy Rust toolchain file".into());
                }
                return Ok(bytes.trim().into());
            }
            let value: toml::Value = toml::from_str(&bytes)
                .map_err(|error| format!("invalid Rust toolchain file: {error}"))?;
            let toolchain = value
                .get("toolchain")
                .and_then(toml::Value::as_table)
                .ok_or("missing Rust toolchain table")?;
            if let Some(profile) = toolchain.get("profile")
                && !profile
                    .as_str()
                    .is_some_and(|profile| ["minimal", "default", "complete"].contains(&profile))
            {
                return Err("unsupported Rust toolchain profile".into());
            }
            if let Some(path) = toolchain.get("path") {
                let path = path.as_str().ok_or("invalid Rust toolchain path")?;
                if toolchain.contains_key("channel") {
                    return Err("Rust toolchain file has channel and path".into());
                }
                return Ok(path.into());
            }
            let selector = toolchain
                .get("channel")
                .or_else(|| settings.get("default_toolchain"))
                .and_then(toml::Value::as_str)
                .ok_or("Rust toolchain file does not select an installed channel")?;
            let root = &runtime_for_selector(selector, environment)?.host_path;
            let manifest: toml::Value = toml::from_str(&read_metadata(
                &root.join("lib/rustlib/multirust-config.toml"),
            )?)
            .map_err(|error| format!("invalid installed Rust component manifest: {error}"))?;
            for key in ["components", "targets"] {
                if let Some(requested) = toolchain.get(key) {
                    let requested = requested
                        .as_array()
                        .ok_or("invalid Rust component/target list")?;
                    for entry in requested {
                        let name = entry.as_str().ok_or("invalid Rust component/target name")?;
                        let found = manifest
                            .get("components")
                            .and_then(toml::Value::as_array)
                            .is_some_and(|components| {
                                components.iter().any(|component| {
                                    let package =
                                        component.get("pkg").and_then(toml::Value::as_str);
                                    if key == "targets" {
                                        package == Some("rust-std")
                                            && component.get("target").and_then(toml::Value::as_str)
                                                == Some(name)
                                    } else {
                                        package == Some(name)
                                            || package == Some(format!("{name}-preview").as_str())
                                    }
                                })
                            });
                        if !found {
                            return Err(format!("required Rust component/target absent: {name}"));
                        }
                    }
                }
            }
            return Ok(selector.into());
        }
    }
    settings
        .get("default_toolchain")
        .and_then(toml::Value::as_str)
        .map(String::from)
        .ok_or_else(|| "Rustup has no default toolchain".into())
}

fn validate_selector(selector: &str, environment: &ReifiedRustupEnvironment) -> Result<(), String> {
    runtime_for_selector(selector, environment).map(|_| ())
}

fn runtime_for_selector<'a>(
    selector: &str,
    environment: &'a ReifiedRustupEnvironment,
) -> Result<&'a ReifiedExecutionSubstrateMount, String> {
    let settings: toml::Value = toml::from_str(&environment.settings)
        .map_err(|error| format!("invalid private Rustup settings: {error}"))?;
    let host = settings
        .get("default_host_triple")
        .and_then(toml::Value::as_str)
        .map(String::from)
        .unwrap_or_else(|| {
            let abi = if cfg!(target_env = "musl") {
                "musl"
            } else {
                "gnu"
            };
            format!("{}-unknown-linux-{abi}", std::env::consts::ARCH)
        });
    let found = if Path::new(selector).is_absolute() {
        environment
            .toolchain_mounts
            .iter()
            .find(|mount| mount.namespace_path == Path::new(selector))
    } else {
        environment.toolchain_mounts.iter().find(|mount| {
            if !safe_name(selector) {
                return false;
            }
            let name = mount
                .namespace_path
                .file_name()
                .and_then(|name| name.to_str());
            name == Some(selector) || name == Some(format!("{selector}-{host}").as_str())
        })
    };
    found.ok_or_else(|| format!("selected Rust runtime absent or unsupported: {selector}"))
}

pub(super) fn revalidate(environment: &ReifiedRustupEnvironment) -> Result<(), String> {
    for mount in &environment.proxy_mounts {
        validate_proxies(&mount.host_path)?;
    }
    for mount in &environment.toolchain_mounts {
        if mount.namespace_path != mount.host_path {
            return Err("Rustup runtime has an unsupported namespace remap".into());
        }
        validate_runtime(&mount.host_path)?;
    }
    for (path, expected) in &environment.metadata_hashes {
        if digest(&read_metadata(path)?) != *expected {
            return Err("Rustup selection metadata changed before execution".into());
        }
    }
    for (path, expected) in &environment.executable_hashes {
        let root = environment
            .proxy_mounts
            .iter()
            .chain(&environment.toolchain_mounts)
            .find(|mount| path.starts_with(&mount.host_path))
            .ok_or("Rust executable is outside its inspected roots")?;
        validate_executable(path, &root.host_path)?;
        if executable_digest(path)? != *expected {
            return Err("Rust executable changed before execution".into());
        }
    }
    Ok(())
}

fn executable_digest(path: &Path) -> Result<String, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > 256 * 1024 * 1024 {
        return Err("Rust executable exceeds inspection bound".into());
    }
    let mut file = std::fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(256 * 1024 * 1024 + 1);
    let mut hasher = Sha256::new();
    let mut buffer = [0; 65536];
    let mut total = 0;
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        total += count;
        if total > 256 * 1024 * 1024 {
            return Err("Rust executable exceeds inspection bound".into());
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn validate_proxies(root: &Path) -> Result<(), String> {
    let rustup = root.join("rustup");
    let metadata = std::fs::symlink_metadata(&rustup).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("unsupported Rustup executable alias".into());
    }
    validate_executable(&rustup, root)?;
    let mut count = 0;
    for entry in std::fs::read_dir(root).map_err(|error| error.to_string())? {
        count += 1;
        if count > PROXIES.len() {
            return Err("unsupported extra file in Rustup proxy directory".into());
        }
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        if !name.to_str().is_some_and(|name| PROXIES.contains(&name)) {
            return Err("unsupported extra file in Rustup proxy directory".into());
        }
        let path = std::fs::canonicalize(entry.path()).map_err(|error| error.to_string())?;
        let proxy = std::fs::metadata(&path).map_err(|error| error.to_string())?;
        if !path.starts_with(root) || proxy.dev() != metadata.dev() || proxy.ino() != metadata.ino()
        {
            return Err("Rustup proxy escapes its validated executable".into());
        }
    }
    Ok(())
}

fn validate_runtime(root: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(root)
        .map_err(|error| format!("Rust runtime unavailable: {error}"))?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || std::fs::canonicalize(root).map_err(|error| error.to_string())? != root
    {
        return Err("Rust runtime root is an unsafe alias".into());
    }
    if !root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(safe_name)
    {
        return Err("unsafe Rust runtime name".into());
    }
    for entry in std::fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry
            .file_name()
            .to_str()
            .is_some_and(|name| ["bin", "lib", "libexec", "etc", "share"].contains(&name))
        {
            return Err("unsupported content in Rust runtime root".into());
        }
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            return Err("Rust runtime directory is an unsafe alias".into());
        }
    }
    for name in ["cargo", "rustc", "rustdoc"] {
        validate_executable(&root.join("bin").join(name), root)?;
    }
    let manifest = root.join("lib/rustlib/multirust-config.toml");
    if !std::fs::canonicalize(&manifest)
        .map_err(|error| error.to_string())?
        .starts_with(root)
    {
        return Err("Rust component metadata escapes its runtime".into());
    }
    read_metadata(&manifest)?;
    Ok(())
}

fn validate_executable(path: &Path, root: &Path) -> Result<(), String> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|error| format!("Rust executable unavailable: {error}"))?;
    let metadata = std::fs::metadata(&canonical).map_err(|error| error.to_string())?;
    if !canonical.starts_with(root) || !metadata.is_file() || metadata.mode() & 0o111 == 0 {
        return Err("Rust executable escapes its runtime or is not executable".into());
    }
    let mut magic = [0; 4];
    std::fs::File::open(canonical)
        .and_then(|mut file| file.read_exact(&mut magic))
        .map_err(|error| error.to_string())?;
    if magic != *b"\x7fELF" {
        return Err("unsupported Rust executable wrapper".into());
    }
    Ok(())
}

fn read_metadata(path: &Path) -> Result<String, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("Rust metadata unavailable: {error}"))?;
    if !metadata.is_file() || metadata.len() > 65536 {
        return Err("unsafe or oversized Rust metadata file".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 65536 {
        return Err("Rust metadata exceeds inspection bound".into());
    }
    String::from_utf8(bytes).map_err(|_| "Rust metadata is not UTF-8".into())
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
}

fn digest(bytes: &str) -> String {
    hex::encode(Sha256::digest(bytes.as_bytes()))
}

#[cfg(test)]
#[path = "rustup_tests.rs"]
mod tests;
