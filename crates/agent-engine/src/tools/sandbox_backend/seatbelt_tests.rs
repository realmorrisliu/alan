use super::*;

#[test]
fn seatbelt_profile_confines_writes_and_denies_network() {
    let writable_roots = vec![PathBuf::from("/work/space")];
    let profile = seatbelt_profile(&writable_roots, &[], false);
    assert!(profile.contains("(deny network*)"));
    assert!(profile.contains("(deny file-write*)"));
    assert!(profile.contains("(allow file-write* (subpath \"/work/space\"))"));
}

#[test]
fn seatbelt_profile_permits_network_when_approved() {
    // An approved network call runs with network allowed (still fs-confined).
    let writable_roots = vec![PathBuf::from("/work/space")];
    let approved = seatbelt_profile(&writable_roots, &[], true);
    assert!(!approved.contains("(deny network*)"));
    assert!(approved.contains("(deny file-write*)"));
}

#[test]
fn seatbelt_profile_single_root_matches_pre_refactor_profile() {
    let host_mount_root = PathBuf::from("/work/space");
    let writable_roots = vec![host_mount_root.clone()];
    let profile = seatbelt_profile(&writable_roots, &[], false);
    assert_eq!(
        profile,
        pre_refactor_single_host_mount_profile(&host_mount_root, false)
    );
    assert!(!profile.contains("(deny file-read*"));
}

#[test]
fn seatbelt_profile_emits_read_denies_when_configured() {
    let writable_roots = vec![PathBuf::from("/work/space")];
    let read_denylist = vec![PathBuf::from("/secret"), PathBuf::from("/home/me/.netrc")];
    let profile = seatbelt_profile(&writable_roots, &read_denylist, false);
    assert!(profile.contains("(deny file-read* (literal \"/secret\") (subpath \"/secret\"))"));
    assert!(
        profile.contains(
            "(deny file-read* (literal \"/home/me/.netrc\") (subpath \"/home/me/.netrc\"))"
        )
    );
}

#[test]
fn seatbelt_profile_omits_exact_writable_root_read_denies() {
    let writable_roots = vec![PathBuf::from("/Users/alice/.alan")];
    let read_denylist = vec![
        PathBuf::from("/Users/alice"),
        PathBuf::from("/Users/alice/.alan"),
        PathBuf::from("/Users/alice/.alan-dev"),
        PathBuf::from("/Users/alice/.ssh"),
    ];
    let profile = seatbelt_profile(&writable_roots, &read_denylist, false);

    assert!(profile.contains("(deny file-read* (literal \"/Users/alice\")"));
    assert!(!profile.contains("(deny file-read* (literal \"/Users/alice/.alan\")"));
    assert!(profile.contains("(deny file-read* (literal \"/Users/alice/.alan-dev\")"));
    assert!(profile.contains("(deny file-read* (literal \"/Users/alice/.ssh\")"));
}

#[test]
fn seatbelt_profile_preserves_parent_read_denies_for_nested_writable_roots() {
    let writable_roots = vec![PathBuf::from("/Users/alice/.ssh/project")];
    let read_denylist = vec![PathBuf::from("/Users/alice/.ssh")];
    let profile = seatbelt_profile(&writable_roots, &read_denylist, false);

    assert!(profile.contains("(deny file-read* (literal \"/Users/alice/.ssh\")"));
}

#[cfg(target_os = "macos")]
#[test]
fn seatbelt_enforces_host_mount_write_boundary_on_macos() {
    if !super::super::seatbelt_available() {
        return; // sandbox-exec not present; nothing to enforce
    }
    let host_mount = tempfile::tempdir().unwrap();
    let writable_roots = vec![host_mount.path().to_path_buf()];
    let profile = seatbelt_profile(&writable_roots, &[], false);
    let canonical_host_mount = std::fs::canonicalize(host_mount.path()).unwrap();

    let run = |script: String| {
        std::process::Command::new("/usr/bin/sandbox-exec")
            .arg("-p")
            .arg(&profile)
            .arg("sh")
            .arg("-c")
            .arg(script)
            .status()
            .unwrap()
    };

    // In-host_mount write succeeds. If it fails, this environment cannot run
    // `sandbox-exec` (e.g. a restricted CI runner) — skip rather than fail.
    let inside_file = canonical_host_mount.join("inside.txt");
    let ok = run(format!("echo hi > {}", inside_file.display()));
    if !ok.success() || !inside_file.exists() {
        return;
    }

    // Out-of-host_mount write (under HOME, outside host_mount and temp roots)
    // is blocked by the kernel regardless of command syntax.
    let escape_file =
        PathBuf::from(std::env::var("HOME").unwrap()).join(".alan_seatbelt_escape_test");
    let _ = std::fs::remove_file(&escape_file);
    let blocked = run(format!("echo hi > {}", escape_file.display()));
    assert!(
        !blocked.success(),
        "out-of-host_mount write should be denied"
    );
    assert!(
        !escape_file.exists(),
        "kernel must prevent the out-of-host_mount file from being created"
    );
}

fn pre_refactor_single_host_mount_profile(host_mount_root: &Path, allow_network: bool) -> String {
    let canonical_root = canonical_string(host_mount_root);
    let root = sbpl_quote(&canonical_root);
    let tmpdir = std::env::var("TMPDIR").ok();
    let mut writable = vec![
        root,
        sbpl_quote("/tmp"),
        sbpl_quote("/private/tmp"),
        sbpl_quote("/private/var/folders"),
    ];
    if let Some(tmpdir) = tmpdir.as_deref().filter(|value| !value.is_empty()) {
        writable.push(sbpl_quote(
            canonical_string(Path::new(tmpdir.trim_end_matches('/'))).as_str(),
        ));
    }
    let write_allows = writable
        .iter()
        .map(|path| format!("(allow file-write* (subpath {path}))"))
        .collect::<Vec<_>>()
        .join("\n");
    let network_rule = if allow_network {
        ""
    } else {
        "(deny network*)\n"
    };
    format!(
        "(version 1)\n\
         (allow default)\n\
         {network_rule}\
         (deny file-write*)\n\
         {write_allows}\n\
         (allow file-write-data (literal \"/dev/null\") (literal \"/dev/stdout\") (literal \"/dev/stderr\") (literal \"/dev/tty\") (literal \"/dev/dtracehelper\"))\n"
    )
}
