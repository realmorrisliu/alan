//! Descriptor-bound permission metadata used when adopting Host-owned files.

use std::{
    collections::BTreeMap,
    ffi::CString,
    fs::File,
    os::{fd::AsRawFd, unix::fs::MetadataExt},
};

use anyhow::{Context, Result, ensure};
use serde::Serialize;

const MAX_METADATA: usize = 16 * 1024 * 1024;

/// Extended attributes include Linux POSIX ACLs; macOS ACLs have a separate native API.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct ExtendedMetadata {
    owner: u32,
    group: u32,
    attributes: BTreeMap<String, Vec<u8>>,
    #[cfg(target_os = "macos")]
    acl: Option<String>,
}

impl ExtendedMetadata {
    pub fn read(file: &File) -> Result<Self> {
        let fd = file.as_raw_fd();
        let names = read_buffer(|buffer, size| unsafe {
            #[cfg(target_os = "macos")]
            {
                libc::flistxattr(fd, buffer.cast(), size, 0)
            }
            #[cfg(target_os = "linux")]
            {
                libc::flistxattr(fd, buffer.cast(), size)
            }
        })?;
        let mut attributes = BTreeMap::new();
        let mut total = names.len();
        for bytes in names
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
        {
            let name = CString::new(bytes)?;
            let value = read_buffer(|buffer, size| unsafe {
                #[cfg(target_os = "macos")]
                {
                    libc::fgetxattr(fd, name.as_ptr(), buffer.cast(), size, 0, 0)
                }
                #[cfg(target_os = "linux")]
                {
                    libc::fgetxattr(fd, name.as_ptr(), buffer.cast(), size)
                }
            })?;
            total += value.len();
            ensure!(
                total <= MAX_METADATA,
                "extended metadata exceeds supported size"
            );
            attributes.insert(String::from_utf8(bytes.to_vec())?, value);
        }
        Ok(Self {
            owner: file.metadata()?.uid(),
            group: file.metadata()?.gid(),
            attributes,
            #[cfg(target_os = "macos")]
            acl: macos::read(fd)?,
        })
    }

    /// Apply only to a newly staged file, replacing inherited metadata before publication.
    pub fn apply(&self, file: &File) -> Result<()> {
        let fd = file.as_raw_fd();
        let current = file.metadata()?;
        if current.uid() != self.owner || current.gid() != self.group {
            syscall(unsafe { libc::fchown(fd, self.owner, self.group) } as isize)
                .context("preserve migration file ownership")?;
        }
        for name in Self::read(file)?.attributes.keys() {
            if !self.attributes.contains_key(name) {
                let name = CString::new(name.as_bytes())?;
                let result = unsafe {
                    #[cfg(target_os = "macos")]
                    {
                        libc::fremovexattr(fd, name.as_ptr(), 0)
                    }
                    #[cfg(target_os = "linux")]
                    {
                        libc::fremovexattr(fd, name.as_ptr())
                    }
                };
                syscall(result as isize).context("remove inherited extended attribute")?;
            }
        }
        for (name, value) in &self.attributes {
            let name = CString::new(name.as_bytes())?;
            let result = unsafe {
                #[cfg(target_os = "macos")]
                {
                    libc::fsetxattr(fd, name.as_ptr(), value.as_ptr().cast(), value.len(), 0, 0)
                }
                #[cfg(target_os = "linux")]
                {
                    libc::fsetxattr(fd, name.as_ptr(), value.as_ptr().cast(), value.len(), 0)
                }
            };
            syscall(result as isize).context("preserve extended attribute")?;
        }
        #[cfg(target_os = "macos")]
        macos::apply(fd, self.acl.as_deref())?;
        Ok(())
    }
}

fn syscall(result: isize) -> Result<usize> {
    if result < 0 {
        Err(std::io::Error::last_os_error().into())
    } else {
        Ok(result as usize)
    }
}

fn read_buffer(call: impl Fn(*mut u8, usize) -> isize) -> Result<Vec<u8>> {
    let size = syscall(call(std::ptr::null_mut(), 0)).context("measure extended metadata")?;
    ensure!(
        size <= MAX_METADATA,
        "extended metadata exceeds supported size"
    );
    let mut bytes = vec![0; size];
    let length = syscall(call(bytes.as_mut_ptr(), size)).context("read extended metadata")?;
    ensure!(
        length == size,
        "extended metadata changed during inspection"
    );
    Ok(bytes)
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::ffi::{CStr, c_char, c_int, c_void};

    const ACL_TYPE_EXTENDED: c_int = 0x100;
    unsafe extern "C" {
        fn acl_init(count: c_int) -> *mut c_void;
        fn acl_get_fd_np(fd: c_int, kind: c_int) -> *mut c_void;
        fn acl_set_fd_np(fd: c_int, acl: *mut c_void, kind: c_int) -> c_int;
        fn acl_to_text(acl: *mut c_void, length: *mut isize) -> *mut c_char;
        fn acl_from_text(text: *const c_char) -> *mut c_void;
        fn acl_free(value: *mut c_void) -> c_int;
    }

    struct Allocation(*mut c_void);
    impl Allocation {
        fn new(value: *mut c_void) -> Result<Self> {
            if value.is_null() {
                Err(std::io::Error::last_os_error().into())
            } else {
                Ok(Self(value))
            }
        }
    }
    impl Drop for Allocation {
        fn drop(&mut self) {
            unsafe {
                acl_free(self.0);
            }
        }
    }

    pub(super) fn read(fd: c_int) -> Result<Option<String>> {
        let raw = unsafe { acl_get_fd_np(fd, ACL_TYPE_EXTENDED) };
        if raw.is_null() && std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound {
            return Ok(None);
        }
        let acl = Allocation::new(raw)?;
        let mut length = 0;
        let text = Allocation::new(unsafe { acl_to_text(acl.0, &mut length) }.cast())?;
        ensure!(
            (0..=MAX_METADATA as isize).contains(&length),
            "ACL exceeds supported size"
        );
        Ok(Some(
            unsafe { CStr::from_ptr(text.0.cast()) }
                .to_str()?
                .to_owned(),
        ))
    }

    pub(super) fn apply(fd: c_int, text: Option<&str>) -> Result<()> {
        let acl = if let Some(text) = text {
            let text = CString::new(text)?;
            Allocation::new(unsafe { acl_from_text(text.as_ptr()) })?
        } else {
            Allocation::new(unsafe { acl_init(0) })?
        };
        syscall(unsafe { acl_set_fd_np(fd, acl.0, ACL_TYPE_EXTENDED) } as isize)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_preserves_an_alternate_group_when_the_user_has_one() {
        let count = syscall(unsafe { libc::getgroups(0, std::ptr::null_mut()) } as isize).unwrap();
        let mut groups = vec![0; count];
        syscall(unsafe { libc::getgroups(count as i32, groups.as_mut_ptr()) } as isize).unwrap();
        let file = tempfile::tempfile().unwrap();
        let mut expected = ExtendedMetadata::read(&file).unwrap();
        if let Some(group) = groups.into_iter().find(|group| *group != expected.group) {
            expected.group = group;
            expected.apply(&file).unwrap();
            assert!(ExtendedMetadata::read(&file).unwrap() == expected);
        }
    }

    #[test]
    fn attributes_round_trip_and_inherited_attributes_are_removed() {
        let file = tempfile::tempfile().unwrap();
        let mut expected = ExtendedMetadata::read(&file).unwrap();
        expected
            .attributes
            .insert("user.alan-migration-test".into(), vec![0, 1, 255]);
        expected.apply(&file).unwrap();
        assert!(ExtendedMetadata::read(&file).unwrap() == expected);
        let mut empty = expected.clone();
        empty.attributes.remove("user.alan-migration-test");
        empty.apply(&file).unwrap();
        let actual = ExtendedMetadata::read(&file).unwrap();
        assert!(!actual.attributes.contains_key("user.alan-migration-test"));
        assert!(actual == empty);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_acl_round_trips_on_another_descriptor() {
        let source = tempfile::NamedTempFile::new().unwrap();
        assert!(
            std::process::Command::new("chmod")
                .args(["+a", "everyone allow read"])
                .arg(source.path())
                .status()
                .unwrap()
                .success()
        );
        let expected = ExtendedMetadata::read(source.as_file()).unwrap();
        let target = tempfile::tempfile().unwrap();
        assert!(ExtendedMetadata::read(&target).unwrap() != expected);
        expected.apply(&target).unwrap();
        assert!(ExtendedMetadata::read(&target).unwrap() == expected);
    }
}
