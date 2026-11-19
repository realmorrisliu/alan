use std::process::Command;

use alan::cli::attachments::{
    collect_cli_image_attachments, load_image_attachment, validate_cli_scripting_flags,
};
use alan_agent_protocol::ContentPart;

const MINIMAL_PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15c4";

#[test]
fn cli_flag_validation_covers_print_json_and_image() {
    let dir = tempfile::tempdir().expect("tempdir");
    let img = dir.path().join("sample.png");
    std::fs::write(&img, MINIMAL_PNG).expect("write png");

    // Valid combinations pass.
    validate_cli_scripting_flags(Some("hello"), false, &[], false).expect("valid -p");
    validate_cli_scripting_flags(Some("hello"), true, &[], false).expect("valid -p --json");
    validate_cli_scripting_flags(Some(""), false, std::slice::from_ref(&img), false)
        .expect("empty -p allowed when --image provided");
    validate_cli_scripting_flags(None, true, std::slice::from_ref(&img), false)
        .expect("--json with --image");

    // Empty -p without --image fails.
    assert!(validate_cli_scripting_flags(Some("   "), false, &[], false).is_err());
    // Scripting flags rejected with subcommand.
    assert!(validate_cli_scripting_flags(Some("hi"), false, &[], true).is_err());
    assert!(validate_cli_scripting_flags(None, true, &[], true).is_err());
    assert!(validate_cli_scripting_flags(None, false, std::slice::from_ref(&img), true).is_err());
    // Missing or directory --image fails.
    assert!(validate_cli_scripting_flags(None, false, &[dir.path().to_path_buf()], false).is_err());
    assert!(
        validate_cli_scripting_flags(None, false, &[dir.path().join("nope.png")], false).is_err()
    );
}

#[test]
fn image_attachments_load_from_explicit_flag_and_inline_at_reference() {
    let dir = tempfile::tempdir().expect("tempdir");
    let explicit = dir.path().join("flag.png");
    let referenced = dir.path().join("inline.jpg");
    std::fs::write(&explicit, MINIMAL_PNG).expect("write png");
    std::fs::write(&referenced, b"\xff\xd8\xff\xe0\x00\x10JFIF").expect("write jpg");

    let single = load_image_attachment(&explicit).expect("load png attachment");
    match &single {
        ContentPart::Attachment {
            hash,
            mime_type,
            metadata,
        } => {
            assert!(hash.starts_with("sha256:"), "{hash}");
            assert_eq!(mime_type, "image/png");
            assert_eq!(metadata["filename"], "flag.png");
            let image_url = metadata["image_url"].as_str().expect("image_url string");
            assert!(
                image_url.starts_with("data:image/png;base64,"),
                "{image_url}"
            );
        }
        other => panic!("unexpected part: {other:?}"),
    }

    // Collect deduplicates duplicate paths and resolves @inline.jpg inside base_dir.
    let prompt = format!("Compare @{} and @inline.jpg.", explicit.display());
    let parts = collect_cli_image_attachments(
        Some(&prompt),
        Some(dir.path()),
        std::slice::from_ref(&explicit),
    )
    .expect("collect image attachments");
    assert_eq!(
        parts.len(),
        2,
        "expected deduplicated flag + inline jpg: {parts:?}"
    );
    let mimes = parts
        .iter()
        .filter_map(|part| match part {
            ContentPart::Attachment { mime_type, .. } => Some(mime_type.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(mimes, vec!["image/png", "image/jpeg"]);
}

#[test]
fn cli_help_exposes_print_json_image_and_skills_list_runs_cleanly() {
    let bin = env!("CARGO_BIN_EXE_alan");
    let help = Command::new(bin)
        .arg("--help")
        .output()
        .expect("run alan --help");
    assert!(help.status.success());
    let stdout = String::from_utf8_lossy(&help.stdout);
    assert!(
        stdout.contains("--print") && stdout.contains("-p"),
        "{stdout}"
    );
    assert!(stdout.contains("--json"), "{stdout}");
    assert!(stdout.contains("--image"), "{stdout}");

    let home = tempfile::tempdir().expect("isolated home");
    let list_json = Command::new(bin)
        .env("HOME", home.path())
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("XDG_STATE_HOME", home.path().join("state"))
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .args(["skills", "list", "--json"])
        .output()
        .expect("run alan skills list --json");
    assert!(
        list_json.status.success(),
        "skills list --json failed: {}",
        String::from_utf8_lossy(&list_json.stderr)
    );
    let parsed: serde_json::Value =
        serde_json::from_slice(&list_json.stdout).expect("valid JSON output");
    let packages = parsed["packages"].as_array().expect("packages array");
    assert!(
        !packages.is_empty(),
        "expected preinstalled skill packages in JSON output: {parsed}"
    );
    assert!(
        packages.iter().any(|pkg| pkg["kind"] == "preinstalled"),
        "expected preinstalled kind entry: {parsed}"
    );

    let list_table = Command::new(bin)
        .env("HOME", home.path())
        .env("XDG_DATA_HOME", home.path().join("data"))
        .args(["skills", "list"])
        .output()
        .expect("run alan skills list");
    assert!(list_table.status.success());
    let table_out = String::from_utf8_lossy(&list_table.stdout);
    assert!(
        table_out.contains("PACKAGE") && table_out.contains("preinstalled"),
        "{table_out}"
    );
}

#[test]
fn cli_binary_rejects_empty_print_and_missing_image_fast() {
    let bin = env!("CARGO_BIN_EXE_alan");
    let empty_print = Command::new(bin)
        .args(["-p", "   "])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run alan -p empty");
    assert!(!empty_print.status.success());
    let stderr = String::from_utf8_lossy(&empty_print.stderr);
    assert!(stderr.contains("--print"), "{stderr}");

    let missing_img = Command::new(bin)
        .args([
            "--image",
            "/nonexistent/alan-m2-missing-image.png",
            "-p",
            "hi",
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run alan --image missing");
    assert!(!missing_img.status.success());
    let stderr_img = String::from_utf8_lossy(&missing_img.stderr);
    assert!(stderr_img.contains("--image"), "{stderr_img}");
}

#[test]
fn cli_about_and_man_subcommands_cover_adr0061_and_no_color_fallback() {
    let bin = env!("CARGO_BIN_EXE_alan");

    // Default `alan about` emits Klein Blue Braille mascot + 7 standard namespace dirs without Herdr.
    let about_color = Command::new(bin)
        .env_remove("NO_COLOR")
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .arg("about")
        .output()
        .expect("run alan about");
    assert!(about_color.status.success());
    let stdout = String::from_utf8_lossy(&about_color.stdout);
    assert!(
        stdout.contains("\u{1b}["),
        "expected ANSI sequences: {stdout:?}"
    );
    assert!(stdout.contains("ALAN(1)"), "{stdout}");
    for dir in ["/proc", "/agent", "/srv", "/bin", "/lib", "/man", "/mnt"] {
        assert!(stdout.contains(dir), "missing {dir}: {stdout}");
    }
    assert!(!stdout.to_lowercase().contains("herdr"), "{stdout}");

    // `NO_COLOR=1` and `TERM=dumb` both fall back to 4-line ASCII rabbit + uncolored text.
    for (key, val) in [("NO_COLOR", "1"), ("TERM", "dumb")] {
        let out = Command::new(bin)
            .env_remove("NO_COLOR")
            .env("TERM", "xterm-256color")
            .env(key, val)
            .arg("about")
            .output()
            .expect("run alan about dumb");
        assert!(out.status.success());
        let plain = String::from_utf8_lossy(&out.stdout);
        assert!(
            !plain.contains('\u{1b}'),
            "{key}={val} emitted ANSI: {plain:?}"
        );
        assert!(plain.contains("(\\_/)"), "missing ASCII rabbit: {plain}");
        assert!(plain.contains("/proc"), "{plain}");
        assert!(!plain.to_lowercase().contains("herdr"), "{plain}");
    }

    // `alan man` lists the `/man` index, and `alan man alan9` / `alan man 1/q` read real ADR-0061 pages.
    let index = Command::new(bin).arg("man").output().expect("run alan man");
    assert!(index.status.success());
    let index_out = String::from_utf8_lossy(&index.stdout);
    for expected in [
        "/man/1/alan",
        "/man/1/q",
        "/man/1/connection",
        "/man/4/agentfs",
        "/man/7/alan9",
    ] {
        assert!(index_out.contains(expected), "{index_out}");
    }

    let alan9 = Command::new(bin)
        .args(["man", "alan9"])
        .output()
        .expect("run alan man alan9");
    assert!(alan9.status.success());
    let alan9_out = String::from_utf8_lossy(&alan9.stdout);
    assert!(alan9_out.contains("ALAN9(7)"), "{alan9_out}");
    assert!(
        alan9_out.contains("/proc") && alan9_out.contains("/mnt"),
        "{alan9_out}"
    );
    assert!(!alan9_out.to_lowercase().contains("herdr"), "{alan9_out}");

    let q_man = Command::new(bin)
        .args(["man", "1/q"])
        .output()
        .expect("run alan man 1/q");
    assert!(q_man.status.success());
    let q_out = String::from_utf8_lossy(&q_man.stdout);
    for kind in ["skill", "mcp", "tool", "provider"] {
        assert!(q_out.contains(kind), "missing {kind} in q(1): {q_out}");
    }

    let conn_man = Command::new(bin)
        .args(["man", "connection"])
        .output()
        .expect("run alan man connection");
    assert!(conn_man.status.success());
    let conn_out = String::from_utf8_lossy(&conn_man.stdout);
    for provider in [
        "magpie",
        "openai_responses",
        "chatgpt",
        "vertex_gemini",
        "openrouter",
    ] {
        assert!(
            conn_out.contains(provider),
            "missing {provider}: {conn_out}"
        );
    }

    let missing_man = Command::new(bin)
        .args(["man", "does-not-exist"])
        .output()
        .expect("run alan man nonexistent");
    assert!(!missing_man.status.success());
}
