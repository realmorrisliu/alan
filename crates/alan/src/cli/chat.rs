//! `alan chat` — launch interactive TUI.

use alan_runtime::InstallChannel;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Launch the interactive TUI chat.
///
/// Spawns the TUI process, which manages the daemon lifecycle.
pub async fn run_chat(agent_name: Option<&str>) -> Result<()> {
    let channel = InstallChannel::detect_current();
    // Find TUI executable or bundle
    let tui_path = find_tui_bundle()?;

    let mut cmd = if should_run_via_bun(&tui_path) {
        let mut c = Command::new("bun");
        c.arg("run").arg(&tui_path);
        c
    } else {
        Command::new(&tui_path)
    };
    if let Some(agent_name) = agent_name {
        cmd.env("ALAN_AGENT_NAME", agent_name);
    }
    cmd.env(alan_runtime::INSTALL_CHANNEL_ENV, channel.descriptor().id);

    // Spawn TUI as the main process
    let status = cmd
        .status()
        .with_context(|| format!("Failed to launch TUI from {}", tui_path.display()))?;

    if !status.success() {
        anyhow::bail!("TUI exited with status: {}", status);
    }

    Ok(())
}

fn should_run_via_bun(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("js" | "mjs" | "cjs" | "tsx" | "ts")
    )
}

/// Find the TUI JS bundle.
fn find_tui_bundle() -> Result<PathBuf> {
    find_tui_bundle_for_channel(InstallChannel::detect_current())
}

/// Find the TUI JS bundle for an explicit channel.
fn find_tui_bundle_for_channel(channel: InstallChannel) -> Result<PathBuf> {
    find_tui_bundle_with_env(
        channel,
        std::env::var("ALAN_TUI_PATH").ok().as_deref(),
        std::env::current_exe().ok().as_deref(),
        dirs::home_dir().as_deref(),
    )
}

/// Find the TUI JS bundle with injectable dependencies (for testing).
fn find_tui_bundle_with_env(
    channel: InstallChannel,
    env_path: Option<&str>,
    current_exe: Option<&Path>,
    home_dir: Option<&Path>,
) -> Result<PathBuf> {
    let descriptor = channel.descriptor();
    let tui_name = descriptor.tui_name;
    // 1. ALAN_TUI_PATH env
    if let Some(path) = env_path {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }

    // 2. Adjacent to binary (production install)
    if let Some(exe) = current_exe
        && let Some(dir) = exe.parent()
    {
        let prod_bin = dir.join(tui_name);
        if prod_bin.exists() {
            return Ok(prod_bin);
        }
        let prod_path = dir.join(format!("{tui_name}.js"));
        if prod_path.exists() {
            return Ok(prod_path);
        }
    }

    // 3. Channel-local legacy bin path.
    if let Some(home) = home_dir {
        let home_bin = home
            .join(descriptor.alan_home_dir_name)
            .join("bin")
            .join(tui_name);
        if home_bin.exists() {
            return Ok(home_bin);
        }
        let home_path = home
            .join(descriptor.alan_home_dir_name)
            .join("bin")
            .join(format!("{tui_name}.js"));
        if home_path.exists() {
            return Ok(home_path);
        }
    }

    // 4. Development mode: relative to project root
    let dev_paths = [
        "clients/tui/src/index.tsx",
        "../clients/tui/src/index.tsx",
        "../../clients/tui/src/index.tsx",
    ];
    for p in &dev_paths {
        let path = PathBuf::from(p);
        if path.exists() {
            return Ok(path);
        }
    }

    anyhow::bail!("Cannot find TUI bundle. Set ALAN_TUI_PATH or run `just install`.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_find_tui_bundle_from_env() {
        let tmp = TempDir::new().unwrap();
        let tui_file = tmp.path().join("alan-tui.js");
        std::fs::write(&tui_file, "// test").unwrap();

        let result = find_tui_bundle_with_env(
            InstallChannel::Stable,
            Some(tui_file.to_str().unwrap()),
            None,
            None,
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), tui_file);
    }

    #[test]
    fn test_find_tui_bundle_from_exe_parent() {
        let tmp = TempDir::new().unwrap();
        let exe_dir = tmp.path().join("bin");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let tui_file = exe_dir.join("alan-tui.js");
        std::fs::write(&tui_file, "// test").unwrap();

        let exe_path = exe_dir.join("alan");

        let result = find_tui_bundle_with_env(InstallChannel::Stable, None, Some(&exe_path), None);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), tui_file);
    }

    #[test]
    fn test_find_tui_bundle_prefers_binary_from_exe_parent() {
        let tmp = TempDir::new().unwrap();
        let exe_dir = tmp.path().join("bin");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let tui_bin = exe_dir.join("alan-tui");
        let tui_js = exe_dir.join("alan-tui.js");
        std::fs::write(&tui_bin, "#!/bin/sh\necho test").unwrap();
        std::fs::write(&tui_js, "// test").unwrap();

        let exe_path = exe_dir.join("alan");
        let result = find_tui_bundle_with_env(InstallChannel::Stable, None, Some(&exe_path), None);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), tui_bin);
    }

    #[test]
    fn test_should_run_via_bun_for_source_and_js_only() {
        assert!(should_run_via_bun(Path::new("foo.tsx")));
        assert!(should_run_via_bun(Path::new("foo.js")));
        assert!(!should_run_via_bun(Path::new("alan-tui")));
    }

    #[test]
    fn test_find_tui_bundle_from_home() {
        let tmp = TempDir::new().unwrap();
        let home_dir = tmp.path();
        let bin_dir = home_dir.join(".alan/bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let tui_file = bin_dir.join("alan-tui.js");
        std::fs::write(&tui_file, "// test").unwrap();

        let result = find_tui_bundle_with_env(InstallChannel::Stable, None, None, Some(home_dir));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), tui_file);
    }

    #[test]
    fn test_find_tui_bundle_env_takes_precedence() {
        let tmp = TempDir::new().unwrap();

        // Create env path file
        let env_file = tmp.path().join("env-tui.js");
        std::fs::write(&env_file, "// env").unwrap();

        // Create exe parent file
        let exe_dir = tmp.path().join("bin");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let exe_file = exe_dir.join("alan-tui.js");
        std::fs::write(&exe_file, "// exe").unwrap();

        let exe_path = exe_dir.join("alan");

        // Env should take precedence
        let result = find_tui_bundle_with_env(
            InstallChannel::Stable,
            Some(env_file.to_str().unwrap()),
            Some(&exe_path),
            None,
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), env_file);
    }

    #[test]
    fn test_find_tui_bundle_env_not_existing_falls_through() {
        let tmp = TempDir::new().unwrap();

        // Create env path that doesn't exist
        let env_file = tmp.path().join("nonexistent.js");

        // Create exe parent file
        let exe_dir = tmp.path().join("bin");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let exe_file = exe_dir.join("alan-tui.js");
        std::fs::write(&exe_file, "// exe").unwrap();

        let exe_path = exe_dir.join("alan");

        // Should fall through to exe parent
        let result = find_tui_bundle_with_env(
            InstallChannel::Stable,
            Some(env_file.to_str().unwrap()),
            Some(&exe_path),
            None,
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), exe_file);
    }

    // Note: Testing "not found" case is unreliable in dev environment
    // because dev_paths (clients/tui/src/index.tsx) may exist.

    #[test]
    fn test_find_dev_tui_bundle_from_exe_parent() {
        let tmp = TempDir::new().unwrap();
        let exe_dir = tmp.path().join("bin");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let tui_file = exe_dir.join("alan-dev-tui.js");
        std::fs::write(&tui_file, "// test").unwrap();

        let exe_path = exe_dir.join("alan-dev");

        let result = find_tui_bundle_with_env(InstallChannel::Dev, None, Some(&exe_path), None);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), tui_file);
    }

    #[test]
    fn test_find_dev_tui_bundle_from_dev_home() {
        let tmp = TempDir::new().unwrap();
        let home_dir = tmp.path();
        let bin_dir = home_dir.join(".alan-dev/bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let tui_file = bin_dir.join("alan-dev-tui.js");
        std::fs::write(&tui_file, "// test").unwrap();

        let result = find_tui_bundle_with_env(InstallChannel::Dev, None, None, Some(home_dir));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), tui_file);
    }
}
