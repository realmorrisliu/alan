//! OS-held revision references shared by independent Package Service instances.
use super::super::{fs_safety::ensure_owned_directory, validate_package_id};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};

pub(in super::super) struct PackageLease {
    _file: tempfile::NamedTempFile,
}

impl PackageLease {
    pub(in super::super) fn create(root: &Path, package: &str, revision: &str) -> Result<Self> {
        let directory = root.join("leases");
        ensure_owned_directory(&directory, "package leases path is not an owned directory")?;
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        lock(file.as_file(), false)?;
        serde_json::to_writer(&mut file, &(package, revision))?;
        file.flush()?;
        Ok(Self { _file: file })
    }
}

// The store transaction excludes new leases; dropping a lease may race a scan.
pub(super) fn active(root: &Path) -> Result<BTreeMap<u64, (String, String)>> {
    let directory = root.join("leases");
    ensure_owned_directory(&directory, "package leases path is not an owned directory")?;
    let mut active = BTreeMap::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let mut options = OpenOptions::new();
        options.read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let mut file = match options.open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error).context("open package lease"),
        };
        ensure!(
            file.metadata()?.is_file(),
            "package lease is not a regular file"
        );
        if lock(&file, true)? {
            // No process retains this lease after a crash or interrupted cleanup.
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            continue;
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file).take(1025).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 1024, "package lease is oversized");
        let record: (String, String) = serde_json::from_slice(&bytes)?;
        validate_package_id(&record.0)?;
        super::validate_revision_id(&record.1)?;
        active.insert(active.len() as u64, record);
    }
    Ok(active)
}

fn lock(file: &std::fs::File, probe: bool) -> Result<bool> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: file owns a live descriptor throughout this call.
        let result = unsafe {
            libc::flock(
                file.as_raw_fd(),
                libc::LOCK_EX | if probe { libc::LOCK_NB } else { 0 },
            )
        };
        if result == 0 {
            return Ok(true);
        }
        let error = std::io::Error::last_os_error();
        if probe
            && error
                .raw_os_error()
                .is_some_and(|code| code == libc::EWOULDBLOCK || code == libc::EAGAIN)
        {
            return Ok(false);
        }
        Err(error).context("lock package lease")
    }
    #[cfg(not(unix))]
    {
        let _ = (file, probe);
        anyhow::bail!("package lease locking is unsupported on this platform")
    }
}
