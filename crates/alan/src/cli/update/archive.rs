//! Safe SHA-256 verification and `tar` inspection/extraction helpers.

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs::File;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// RAII temporary directory under `std::env::temp_dir()` removed on drop.
pub struct TempStagingDir {
    path: PathBuf,
}

impl TempStagingDir {
    /// Create a fresh UUID-scoped temporary staging directory (`alan-update-<uuid>`).
    pub fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!("alan-update-{}", uuid::Uuid::new_v4()));
        create_private_dir(&path)?;
        Ok(Self { path })
    }

    /// Return the staging directory path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempStagingDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn create_private_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .with_context(|| format!("create temporary directory `{}`", path.display()))
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(path)
            .with_context(|| format!("create temporary directory `{}`", path.display()))
    }
}

/// Return `true` when `value` is a valid 64-character hexadecimal SHA-256 digest.
pub fn is_valid_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Compute lowercase hex SHA-256 digest for an in-memory byte slice.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    encode_lower_hex(&digest)
}

/// Compute lowercase hex SHA-256 digest by streaming a file.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file =
        File::open(path).with_context(|| format!("open `{}` for SHA-256 check", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buf)
            .with_context(|| format!("read `{}` for SHA-256 check", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(encode_lower_hex(&hasher.finalize()))
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Validate `tar -tzf` entry names against path traversal and return the exact `alan` entry.
pub fn validate_archive_listing(listing: &str) -> Result<String> {
    let mut found_alan: Option<String> = None;
    let mut entry_count = 0usize;

    for raw_line in listing.lines() {
        let entry = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if entry.is_empty() {
            continue;
        }
        entry_count += 1;

        ensure!(
            !entry.chars().any(char::is_control),
            "release archive entry contains control characters"
        );
        ensure!(
            !entry.starts_with('/') && !entry.starts_with('\\'),
            "refusing unsafe absolute path in release archive: `{entry}`"
        );
        let bytes = entry.as_bytes();
        ensure!(
            !(bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'),
            "refusing unsafe drive-prefixed path in release archive: `{entry}`"
        );
        ensure!(
            !entry.split(['/', '\\']).any(|segment| segment == ".."),
            "refusing unsafe path traversal (`..`) in release archive: `{entry}`"
        );

        let is_dir = entry.ends_with('/');
        if !is_dir && matches!(entry, "alan" | "./alan") {
            if let Some(prev) = &found_alan
                && prev != entry
            {
                bail!(
                    "release archive contains conflicting `alan` entries (`{prev}` and `{entry}`)"
                );
            }
            found_alan = Some(entry.to_string());
        }
    }

    ensure!(entry_count > 0, "release archive is empty");
    found_alan.context("release archive does not contain an `alan` executable entry")
}

/// Inspect a verified `.tar.gz` archive with `tar -tzf`, extract only `alan`, and verify its
/// file type and executable permissions inside `staging_dir`.
pub fn inspect_and_extract_alan(archive_path: &Path, staging_dir: &Path) -> Result<PathBuf> {
    let list_output = Command::new("tar")
        .arg("-tzf")
        .arg(archive_path)
        .output()
        .context("invoke `tar -tzf` to inspect release archive")?;
    ensure!(
        list_output.status.success(),
        "`tar -tzf` failed ({}): {}",
        list_output.status,
        String::from_utf8_lossy(&list_output.stderr).trim()
    );
    let listing = String::from_utf8(list_output.stdout)
        .context("`tar -tzf` emitted non-UTF-8 archive entry paths")?;
    let alan_entry = validate_archive_listing(&listing)?;

    let extract_dir = staging_dir.join("extract");
    create_private_dir(&extract_dir)?;

    let extract_output = Command::new("tar")
        .arg("-xzf")
        .arg(archive_path)
        .arg("-C")
        .arg(&extract_dir)
        .arg(&alan_entry)
        .output()
        .context("invoke `tar -xzf` to extract `alan`")?;
    ensure!(
        extract_output.status.success(),
        "`tar -xzf` failed ({}): {}",
        extract_output.status,
        String::from_utf8_lossy(&extract_output.stderr).trim()
    );

    let extracted_bin = extract_dir.join("alan");
    let metadata = std::fs::symlink_metadata(&extracted_bin).with_context(|| {
        format!(
            "inspect extracted binary `{}` after `tar -xzf`",
            extracted_bin.display()
        )
    })?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "extracted `alan` entry at `{}` is not a regular file",
        extracted_bin.display()
    );
    ensure!(metadata.len() > 0, "extracted `alan` binary is empty");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&extracted_bin, std::fs::Permissions::from_mode(0o755))
            .with_context(|| {
                format!(
                    "set executable permissions on staged binary `{}`",
                    extracted_bin.display()
                )
            })?;
        let updated_meta = std::fs::symlink_metadata(&extracted_bin)?;
        ensure!(
            updated_meta.permissions().mode() & 0o111 != 0,
            "extracted `alan` binary at `{}` lacks executable permission bits",
            extracted_bin.display()
        );
    }

    Ok(extracted_bin)
}
