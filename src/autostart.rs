//! Start at login.
//!  Windows: HKCU\Software\Microsoft\Windows\CurrentVersion\Run
//!  macOS:   a LaunchAgent in ~/Library/LaunchAgents
pub use imp::*;

#[cfg(windows)]
mod imp {
    use std::io;
    use winreg::{enums::*, RegKey};

    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const NAME: &str = "Retypr";

    fn exe_value() -> io::Result<String> {
        Ok(format!("\"{}\"", std::env::current_exe()?.display()))
    }

    fn stored() -> Option<String> {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(KEY)
            .and_then(|k| k.get_value::<String, _>(NAME))
            .ok()
    }

    /// Is autostart enabled for THIS copy of the program?
    pub fn is_enabled() -> bool {
        match (exe_value(), stored()) {
            (Ok(want), Some(v)) => v == want,
            _ => false,
        }
    }

    pub fn set(on: bool) -> io::Result<()> {
        let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(KEY)?;
        if on {
            k.set_value(NAME, &exe_value()?)
        } else {
            match k.delete_value(NAME) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                r => r,
            }
        }
    }

    /// Portable folders get moved: if autostart is enabled but points to an old location, repoint it to this copy.
    pub fn refresh_if_enabled() {
        if let (Ok(want), Some(v)) = (exe_value(), stored()) {
            if v != want {
                let _ = set(true);
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use crate::macplat;
    use std::{fs, io, path::PathBuf};

    const LABEL: &str = "com.retypr.app";

    fn plist_path() -> io::Result<PathBuf> {
        let home = std::env::var_os("HOME").ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
        Ok(PathBuf::from(home).join("Library/LaunchAgents").join(format!("{LABEL}.plist")))
    }

    fn exe() -> io::Result<String> {
        Ok(std::env::current_exe()?.to_string_lossy().to_string())
    }

    /// Is the login item enabled for THIS copy of the app?
    pub fn is_enabled() -> bool {
        match (plist_path(), exe()) {
            (Ok(p), Ok(e)) => fs::read_to_string(p).map(|s| s.contains(&macplat::xml_escape(&e))).unwrap_or(false),
            _ => false,
        }
    }

    pub fn set(on: bool) -> io::Result<()> {
        let p = plist_path()?;
        if on {
            if let Some(dir) = p.parent() {
                fs::create_dir_all(dir)?;
            }
            fs::write(&p, macplat::launch_agent_plist(LABEL, &exe()?))
        } else {
            match fs::remove_file(&p) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                r => r,
            }
        }
    }

    /// The app may have been moved: keep the login item pointing at this copy.
    pub fn refresh_if_enabled() {
        if let Ok(p) = plist_path() {
            if p.exists() && !is_enabled() {
                let _ = set(true);
            }
        }
    }
}
