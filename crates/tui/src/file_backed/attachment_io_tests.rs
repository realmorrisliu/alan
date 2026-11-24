use super::*;

const MINIMAL_PNG: &[u8] = b"\x89PNG\r\n\x1a\n1234";
const MINIMAL_JPEG: &[u8] = b"\xff\xd8\xff\xe0\x00\x10JFIF";
const MINIMAL_GIF: &[u8] = b"GIF89a\x01\x00\x01\x00";
const MINIMAL_WEBP: &[u8] = b"RIFF\x18\x00\x00\x00WEBP";

#[test]
fn encodes_base64_and_resolves_project_scoped_at_image_only() {
    assert_eq!(encode_base64(b"hi!"), "aGkh");
    assert_eq!(encode_base64(b"a"), "YQ==");

    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("shot.png"), MINIMAL_PNG).unwrap();
    std::fs::write(outside.path().join("secret.png"), b"\x89PNG\r\n\x1a\n5678").unwrap();

    let prompt = format!(
        "inspect @shot.png and ignore @../{}/secret.png",
        outside.path().file_name().unwrap().to_str().unwrap()
    );
    let parts = extract_at_image_attachments(&prompt, Some(dir.path()));
    assert_eq!(parts.len(), 1);
    match &parts[0] {
        ContentPart::Attachment {
            mime_type,
            metadata,
            ..
        } => {
            assert_eq!(mime_type, "image/png");
            assert_eq!(metadata["filename"], "shot.png");
            assert_eq!(metadata["byte_len"], MINIMAL_PNG.len());
            assert!(
                metadata["image_url"]
                    .as_str()
                    .unwrap()
                    .starts_with("data:image/png;base64,")
            );
        }
        other => panic!("unexpected part: {other:?}"),
    }

    let prompt_parts = extract_prompt_image_attachments(&prompt, Some(dir.path()));
    assert_eq!(prompt_parts, parts);
}

#[test]
fn magic_bytes_validate_all_supported_formats_and_reject_spoofed_files() {
    let dir = tempfile::tempdir().unwrap();
    for (name, bytes, expected_mime) in [
        ("a.png", MINIMAL_PNG, "image/png"),
        ("b.jpg", MINIMAL_JPEG, "image/jpeg"),
        ("c.jpeg", MINIMAL_JPEG, "image/jpeg"),
        ("d.gif", MINIMAL_GIF, "image/gif"),
        ("e.webp", MINIMAL_WEBP, "image/webp"),
    ] {
        let p = dir.path().join(name);
        std::fs::write(&p, bytes).unwrap();
        let PasteImageOutcome::Attached(parts) =
            try_extract_pasted_image_attachments(&p.display().to_string())
        else {
            panic!("expected attached for {name}");
        };
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            ContentPart::Attachment {
                mime_type,
                metadata,
                ..
            } => {
                assert_eq!(mime_type, expected_mime);
                assert_eq!(metadata["filename"], name);
                assert_eq!(metadata["byte_len"], bytes.len());
            }
            other => panic!("unexpected part: {other:?}"),
        }
    }

    let bad = dir.path().join("spoof.png");
    std::fs::write(&bad, b"not actually a png").unwrap();
    match try_extract_pasted_image_attachments(&bad.display().to_string()) {
        PasteImageOutcome::Warning(msg) => assert!(msg.contains("spoof.png"), "{msg}"),
        other => panic!("expected warning, got {other:?}"),
    }

    let empty = dir.path().join("empty.png");
    std::fs::write(&empty, b"").unwrap();
    match try_extract_pasted_image_attachments(&empty.display().to_string()) {
        PasteImageOutcome::Warning(msg) => assert!(msg.contains("empty"), "{msg}"),
        other => panic!("expected warning, got {other:?}"),
    }

    let oversized = dir.path().join("huge.png");
    let file = File::create(&oversized).unwrap();
    file.set_len(MAX_IMAGE_BYTES + 1).unwrap();
    match try_extract_pasted_image_attachments(&oversized.display().to_string()) {
        PasteImageOutcome::Warning(msg) => assert!(msg.contains("20 MiB"), "{msg}"),
        other => panic!("expected 20 MiB warning, got {other:?}"),
    }
}

#[test]
fn pure_paste_parses_abs_tilde_file_uri_quoted_escaped_and_multiline_paths() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("rex-clipboard-1.png");
    let spaced = dir.path().join("Screenshot 1.png");
    let quoted = dir.path().join("My Photo.jpg");
    std::fs::write(&plain, MINIMAL_PNG).unwrap();
    std::fs::write(&spaced, b"\x89PNG\r\n\x1a\nABCD").unwrap();
    std::fs::write(&quoted, MINIMAL_JPEG).unwrap();

    // Bare absolute path with trailing newline.
    let PasteImageOutcome::Attached(parts) =
        try_extract_pasted_image_attachments(&format!("{}\n", plain.display()))
    else {
        panic!("bare absolute path");
    };
    assert_eq!(parts.len(), 1);

    // Backslash-escaped spaces.
    let escaped_str = spaced.display().to_string().replace(' ', "\\ ");
    let PasteImageOutcome::Attached(parts) = try_extract_pasted_image_attachments(&escaped_str)
    else {
        panic!("backslash-escaped path");
    };
    assert_eq!(attachment_filename(&parts[0]), "Screenshot 1.png");

    // Unquoted single-line path with spaces.
    let PasteImageOutcome::Attached(parts) =
        try_extract_pasted_image_attachments(&spaced.display().to_string())
    else {
        panic!("unquoted path with spaces");
    };
    assert_eq!(attachment_filename(&parts[0]), "Screenshot 1.png");

    // Quoted + multi-path newline/space paste (deduped if identical bytes).
    let multi = format!(
        "'{}'\n\"{}\"\n{}",
        spaced.display(),
        quoted.display(),
        plain.display()
    );
    let PasteImageOutcome::Attached(parts) = try_extract_pasted_image_attachments(&multi) else {
        panic!("multi-path paste");
    };
    assert_eq!(parts.len(), 3);

    // file:// URI with %20 percent-decoding.
    let uri = format!(
        "file://{}",
        spaced.display().to_string().replace(' ', "%20")
    );
    let PasteImageOutcome::Attached(parts) = try_extract_pasted_image_attachments(&uri) else {
        panic!("file:// URI");
    };
    assert_eq!(attachment_filename(&parts[0]), "Screenshot 1.png");

    // ~/ expansion via HOME.
    let tilde_outcome = try_extract_pasted_image_attachments_with_home(
        "~/rex-clipboard-1.png",
        Some(dir.path().as_os_str()),
    );
    assert!(matches!(tilde_outcome, PasteImageOutcome::Attached(p) if p.len() == 1));
    assert_eq!(
        try_extract_pasted_image_attachments_with_home("~/rex-clipboard-1.png", None),
        PasteImageOutcome::NotPureImagePaste
    );
}

#[test]
fn non_pure_or_missing_or_non_image_pastes_return_not_pure_image_paste() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("shot.png");
    let txt = dir.path().join("notes.txt");
    let subdir = dir.path().join("folder.png");
    std::fs::write(&png, MINIMAL_PNG).unwrap();
    std::fs::write(&txt, b"hello").unwrap();
    std::fs::create_dir(&subdir).unwrap();

    for input in [
        "".to_string(),
        "   \n  ".to_string(),
        format!("inspect {}", png.display()),
        format!("{} describe this", png.display()),
        dir.path().join("missing.png").display().to_string(),
        txt.display().to_string(),
        subdir.display().to_string(),
        "@shot.png".to_string(),
        "https://example.com/a.png".to_string(),
        "file://remote-host/tmp/a.png".to_string(),
    ] {
        assert_eq!(
            try_extract_pasted_image_attachments(&input),
            PasteImageOutcome::NotPureImagePaste,
            "unexpected outcome for `{input}`"
        );
    }
}

#[test]
fn submit_extractor_combines_project_at_and_explicit_paths_and_deduplicates() {
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let rel_png = project.path().join("shot.png");
    let abs_png = outside.path().join("Screenshot 1.png");
    let abs_dup = outside.path().join("dup.png");
    let abs_secret = outside.path().join("secret.png");
    std::fs::write(&rel_png, MINIMAL_PNG).unwrap();
    std::fs::write(&abs_png, b"\x89PNG\r\n\x1a\nWXYZ").unwrap();
    std::fs::write(&abs_dup, b"\x89PNG\r\n\x1a\nWXYZ").unwrap();
    std::fs::write(&abs_secret, b"\x89PNG\r\n\x1a\nSECRET").unwrap();

    let escaped = abs_png.display().to_string().replace(' ', "\\ ");
    let uri = format!(
        "file://{}",
        abs_dup.display().to_string().replace(' ', "%20")
    );
    let prompt = format!(
        "it's @shot.png, {}, {}, ~/home.png, and ignore @../{}/secret.png.",
        escaped,
        uri,
        outside.path().file_name().unwrap().to_str().unwrap()
    );
    std::fs::write(outside.path().join("home.png"), b"\x89PNG\r\n\x1a\nHOME").unwrap();
    let parts = extract_prompt_image_attachments_with_home(
        &prompt,
        Some(project.path()),
        Some(outside.path().as_os_str()),
    );
    assert_eq!(parts.len(), 3, "{parts:?}");
    assert_eq!(attachment_filename(&parts[0]), "shot.png");
    assert_eq!(attachment_filename(&parts[1]), "Screenshot 1.png");
    assert_eq!(attachment_filename(&parts[2]), "home.png");
}
