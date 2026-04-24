//! Early-boot filesystem mounts — `/proc`, `/sys`, `/dev`, `/run`, `/tmp`.
//!
//! Linux initramfs starts with just the cpio contents on a tmpfs — no
//! virtual filesystems mounted. Userspace tools that open `/dev/null`,
//! `/proc/self`, `/sys/class/*` etc. fail without these. tatara-init calls
//! `mount_early_filesystems()` before spawning any supervised service.
//!
//! Non-Linux builds are no-ops so the crate compiles on macOS for dev.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EarlyMountError {
    #[error("mount({target}): {reason}")]
    Mount { target: String, reason: String },
}

/// One declarative mount description.
#[derive(Debug, Clone)]
pub struct EarlyMount {
    pub source: &'static str,
    pub target: &'static str,
    pub fstype: &'static str,
    pub flags: u64,
    pub data: &'static str,
}

/// The canonical set a Linux guest needs before any service starts.
pub const CANONICAL_MOUNTS: &[EarlyMount] = &[
    EarlyMount {
        source: "proc",
        target: "/proc",
        fstype: "proc",
        flags: 0,
        data: "",
    },
    EarlyMount {
        source: "sysfs",
        target: "/sys",
        fstype: "sysfs",
        flags: 0,
        data: "",
    },
    EarlyMount {
        source: "devtmpfs",
        target: "/dev",
        fstype: "devtmpfs",
        flags: 0,
        data: "mode=0755",
    },
    EarlyMount {
        source: "tmpfs",
        target: "/run",
        fstype: "tmpfs",
        flags: 0,
        data: "mode=0755",
    },
    EarlyMount {
        source: "tmpfs",
        target: "/tmp",
        fstype: "tmpfs",
        flags: 0,
        data: "mode=1777",
    },
];

/// Mount each entry from `CANONICAL_MOUNTS`. Failures are returned as errors
/// but don't abort — the caller logs + continues so a missing kernel module
/// (say `devtmpfs`) doesn't wedge the whole boot.
pub fn mount_early_filesystems() -> Vec<Result<EarlyMount, EarlyMountError>> {
    CANONICAL_MOUNTS
        .iter()
        .map(|m| mount_one(m).map(|()| m.clone()))
        .collect()
}

/// Mount an extra filesystem declared via `(definit :mounts (…))`. Unlike
/// CANONICAL_MOUNTS the fields here are owned strings (the lisp form is
/// dynamic), so we take `&str` values and build C strings on the spot.
///
/// Typical use: virtiofs shares from the host.
///
/// ```no_run
/// # use tatara_init::mounts::mount_extra;
/// mount_extra("nixstore", "/nix/store", "virtiofs", Some("ro"));
/// ```
pub fn mount_extra(
    source: &str,
    target: &str,
    fstype: &str,
    options: Option<&str>,
) -> Result<(), EarlyMountError> {
    mount_extra_impl(source, target, fstype, options)
}

#[cfg(target_os = "linux")]
fn mount_extra_impl(
    source: &str,
    target: &str,
    fstype: &str,
    options: Option<&str>,
) -> Result<(), EarlyMountError> {
    use std::ffi::CString;
    let src = CString::new(source).map_err(|e| err(target, e))?;
    let tgt = CString::new(target).map_err(|e| err(target, e))?;
    let fst = CString::new(fstype).map_err(|e| err(target, e))?;
    let opts_raw = options.unwrap_or("");
    let opts = CString::new(opts_raw).map_err(|e| err(target, e))?;
    // mount(2) flags come encoded in the options string — we pass 0 to
    // `flags` and let virtiofs/ext4/etc. consume `data` as the options
    // string. Keeps the lisp surface simple (one string).
    let _ = std::fs::create_dir_all(target);
    let r = unsafe {
        libc::mount(
            src.as_ptr(),
            tgt.as_ptr(),
            fst.as_ptr(),
            0,
            opts.as_ptr() as *const libc::c_void,
        )
    };
    if r == 0 {
        return Ok(());
    }
    let e = std::io::Error::last_os_error();
    if e.raw_os_error() == Some(libc::EBUSY) {
        return Ok(());
    }
    Err(EarlyMountError::Mount {
        target: target.into(),
        reason: e.to_string(),
    })
}

#[cfg(not(target_os = "linux"))]
fn mount_extra_impl(
    _source: &str,
    _target: &str,
    _fstype: &str,
    _options: Option<&str>,
) -> Result<(), EarlyMountError> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn mount_one(m: &EarlyMount) -> Result<(), EarlyMountError> {
    use std::ffi::CString;
    // Idempotent: if the target already has something mounted on it,
    // mount(2) returns EBUSY — we treat that as success (boot-loop safe).
    // SAFETY: single-threaded at PID-1 bringup; pointers are valid C strings.
    let source = CString::new(m.source).map_err(|e| err(m.target, e))?;
    let target = CString::new(m.target).map_err(|e| err(m.target, e))?;
    let fstype = CString::new(m.fstype).map_err(|e| err(m.target, e))?;
    let data = CString::new(m.data).map_err(|e| err(m.target, e))?;
    // Ensure the mount point exists.
    let _ = std::fs::create_dir_all(m.target);
    let r = unsafe {
        libc::mount(
            source.as_ptr(),
            target.as_ptr(),
            fstype.as_ptr(),
            m.flags,
            data.as_ptr() as *const libc::c_void,
        )
    };
    if r == 0 {
        return Ok(());
    }
    let e = std::io::Error::last_os_error();
    if e.raw_os_error() == Some(libc::EBUSY) {
        return Ok(());
    }
    Err(EarlyMountError::Mount {
        target: m.target.into(),
        reason: e.to_string(),
    })
}

#[cfg(not(target_os = "linux"))]
fn mount_one(_m: &EarlyMount) -> Result<(), EarlyMountError> {
    // No-op: tatara-init compiles on macOS for dev but there's no
    // procfs/sysfs/devtmpfs to mount here. The real mount happens at
    // PID 1 inside the Linux guest.
    Ok(())
}

#[cfg(target_os = "linux")]
fn err<E: std::fmt::Display>(target: &str, e: E) -> EarlyMountError {
    EarlyMountError::Mount {
        target: target.into(),
        reason: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_set_is_in_boot_order() {
        let targets: Vec<_> = CANONICAL_MOUNTS.iter().map(|m| m.target).collect();
        assert_eq!(targets, ["/proc", "/sys", "/dev", "/run", "/tmp"]);
    }

    #[test]
    fn every_canonical_mount_has_nonempty_fstype() {
        for m in CANONICAL_MOUNTS {
            assert!(!m.fstype.is_empty(), "{} has empty fstype", m.target);
            assert!(m.target.starts_with('/'), "{} not absolute", m.target);
        }
    }

    #[test]
    fn mount_one_is_a_no_op_on_non_linux() {
        // Darwin dev path: every call returns Ok.
        for m in CANONICAL_MOUNTS {
            #[cfg(not(target_os = "linux"))]
            assert!(mount_one(m).is_ok());
            #[cfg(target_os = "linux")]
            {
                // On Linux we can't exercise real mounts in unit tests — they
                // need CAP_SYS_ADMIN. Just verify the function is callable.
                let _ = m;
            }
        }
    }
}
