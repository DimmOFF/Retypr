//! Single-instance guard.
//!  Windows: a named mutex (through the `single-instance` crate).
//!  macOS / Linux: an exclusive `flock` on a lock file in the per-user temp folder. The lock is released by the OS
//!  when the process ends, even if it crashes. (The `single-instance` crate used a relative path on macOS, which
//!  fails for apps started from Finder because their working directory is the read-only root "/".)

#[cfg(windows)]
pub struct Instance(single_instance::SingleInstance);

#[cfg(windows)]
impl Instance {
    pub fn new(name: &str) -> anyhow::Result<Self> {
        single_instance::SingleInstance::new(name)
            .map(Instance)
            .map_err(|e| anyhow::anyhow!("single-instance: {e:?}"))
    }

    pub fn is_single(&self) -> bool {
        self.0.is_single()
    }
}

#[cfg(unix)]
pub struct Instance {
    single: bool,
    _file: std::fs::File, // keeping it open keeps the lock
}

#[cfg(unix)]
impl Instance {
    pub fn new(name: &str) -> anyhow::Result<Self> {
        use std::os::unix::io::AsRawFd;
        // one lock file per user: /tmp is shared on Linux
        let uid = unsafe { libc::getuid() };
        let path = std::env::temp_dir().join(format!("{name}-{uid}.lock"));
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| anyhow::anyhow!("cannot open the lock file {}: {e}", path.display()))?;
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        let single = if rc == 0 {
            true
        } else {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::WouldBlock {
                false // another instance holds the lock
            } else {
                return Err(anyhow::anyhow!("cannot lock {}: {err}", path.display()));
            }
        };
        Ok(Instance { single, _file: file })
    }

    pub fn is_single(&self) -> bool {
        self.single
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_second_instance_is_detected_and_the_lock_is_released_on_drop() {
        let name = format!("retypr-test-{}", std::process::id());
        let a = Instance::new(&name).unwrap();
        assert!(a.is_single());
        let b = Instance::new(&name).unwrap();
        assert!(!b.is_single());
        drop(a);
        let c = Instance::new(&name).unwrap();
        assert!(c.is_single());
        let uid = unsafe { libc::getuid() };
        let _ = std::fs::remove_file(std::env::temp_dir().join(format!("{name}-{uid}.lock")));
    }
}
