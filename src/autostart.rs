//! Start with Windows: HKCU\Software\Microsoft\Windows\CurrentVersion\Run
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
