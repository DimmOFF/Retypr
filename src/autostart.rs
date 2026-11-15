//! Автозапуск с Windows: HKCU\Software\Microsoft\Windows\CurrentVersion\Run
use std::io;
use winreg::{enums::*, RegKey};

const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const NAME: &str = "Retypr";

fn exe_value() -> io::Result<String> {
    Ok(format!("\"{}\"", std::env::current_exe()?.display()))
}

pub fn is_enabled() -> bool {
    let Ok(want) = exe_value() else { return false };
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(KEY)
        .and_then(|k| k.get_value::<String, _>(NAME))
        .map(|v| v == want)
        .unwrap_or(false)
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
