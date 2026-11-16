//! Configuration (config.toml) and the portable data folder.
//!
//! Retypr is portable by default: everything lives in a `data` folder next to the executable
//! (config.toml, dict/, sounds/), so the whole program folder can be moved anywhere.
//! Paths stored in the config may be relative; they are resolved against the data folder.
//!
//! Fallbacks: if the exe folder is not writable (e.g. Program Files) or the exe lives inside a macOS
//! `.app` bundle, the per-user config folder is used instead. `RETYPR_DATA_DIR` overrides everything.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use crate::lang::Lang;

const MANUAL_SWITCH_MP3: &[u8] = include_bytes!("../assets/sounds/manual-switch.mp3");
const AUTO_CORRECTION_MP3: &[u8] = include_bytes!("../assets/sounds/automatic-correction.mp3");

pub const DEFAULT_SOUND_MANUAL: &str = "sounds/manual-switch.mp3";
pub const DEFAULT_SOUND_AUTO: &str = "sounds/automatic-correction.mp3";

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum CaretSide {
    Left,
    #[default]
    Right,
}

/// Hotkeys: "Pause", "Shift+Pause", "Ctrl+Alt+F12", "Win+Scroll Lock"...
/// Modifiers: Ctrl, Alt, Shift, Win. Keys: Pause, Break, ScrollLock, CapsLock, Insert, Delete, Home, End,
/// PageUp, PageDown, PrintScreen, Menu, Space, Tab, Enter, Esc, arrows, F1-F24, A-Z, 0-9, Grave (`).
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct HotkeysConfig {
    pub convert_word: String,
    pub convert_selection: String,
    pub toggle_auto: String,
    pub toggle_app_auto: String,
}

impl Default for HotkeysConfig {
    fn default() -> Self {
        Self {
            convert_word: "Pause".into(),
            convert_selection: "Shift+Pause".into(),
            toggle_auto: "Alt+Pause".into(),
            toggle_app_auto: "Alt+Shift+Pause".into(),
        }
    }
}

/// Sound files. A path may be absolute or relative to the data folder; an empty string means "no sound".
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SoundConfig {
    pub enabled: bool,
    /// 0.0 - 1.0
    pub volume: f32,
    pub on_manual_switch: String,
    pub on_auto_switch: String,
    pub on_selection_convert: String,
    /// Played when auto-switching is toggled for the current app (Alt+Shift+Pause)
    pub on_rule_toggle: String,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 0.3,
            on_manual_switch: DEFAULT_SOUND_MANUAL.into(),
            on_auto_switch: DEFAULT_SOUND_AUTO.into(),
            on_selection_convert: DEFAULT_SOUND_MANUAL.into(),
            on_rule_toggle: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub auto_switch: bool,
    /// After converting selected text: remove the selection (default: keep it, so the hotkey can be pressed
    /// again to cycle to the next layout).
    pub deselect_after_convert: bool,
    /// After converting selected text: put the caret at the "left" or "right" end of the converted text.
    pub caret_after_convert: CaretSide,
    /// Settings window theme: "system", "light" or "dark"
    pub theme: String,
    pub min_word_len: usize,
    /// Language codes ("en", "ru", "de"...) that do NOT take part in auto-switching.
    /// By default every language of an installed layout takes part.
    pub disabled_languages: Vec<String>,
    /// Apps (exe name) where only auto-switching is off; the manual hotkeys still work.
    pub auto_exclude_apps: Vec<String>,
    /// Apps where Retypr is completely silent (keys, including Pause, pass through untouched).
    pub disabled_apps: Vec<String>,
    /// Dictionary folder (absolute, or relative to the data folder). Default: <data>/dict
    pub dict_dir: Option<PathBuf>,
    pub sounds: SoundConfig,
    pub hotkeys: HotkeysConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            auto_switch: true,
            deselect_after_convert: false,
            caret_after_convert: CaretSide::Right,
            theme: "system".into(),
            min_word_len: 3,
            disabled_languages: vec![],
            auto_exclude_apps: [
                "code.exe", "devenv.exe", "idea64.exe", "pycharm64.exe", "clion64.exe",
                "rustrover64.exe", "webstorm64.exe", "windowsterminal.exe", "wt.exe",
                "cmd.exe", "powershell.exe", "pwsh.exe", "putty.exe", "mintty.exe",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            disabled_apps: vec![],
            dict_dir: None,
            sounds: SoundConfig::default(),
            hotkeys: HotkeysConfig::default(),
        }
    }
}

// ───────────────────────────── data folder & paths ─────────────────────────────

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(|p| p.to_path_buf())
}

fn writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write-test");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(probe);
    ok
}

fn resolve_data_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("RETYPR_DATA_DIR") {
        return Some(PathBuf::from(p));
    }
    if let Some(exe_dir) = exe_dir() {
        let in_app_bundle = exe_dir.components().any(|c| c.as_os_str().to_string_lossy().ends_with(".app"));
        if !in_app_bundle {
            let portable = exe_dir.join("data");
            if writable(&portable) {
                return Some(portable);
            }
        }
    }
    directories::ProjectDirs::from("", "", "retypr").map(|d| d.config_dir().to_path_buf())
}

/// `p` if absolute, otherwise `base/p`.
fn resolve_against(base: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}

/// A path relative to `base` (with '/' separators) when `abs` lives inside it, otherwise `abs` as is.
fn relativize_against(base: &Path, abs: &Path) -> String {
    match abs.strip_prefix(base) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => abs.to_string_lossy().to_string(),
    }
}

impl Config {
    /// The data folder (portable: next to the exe).
    pub fn dir() -> Option<PathBuf> {
        static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
        DIR.get_or_init(resolve_data_dir).clone()
    }

    pub fn path() -> Option<PathBuf> {
        Self::dir().map(|d| d.join("config.toml"))
    }

    /// Resolve a path from the config (absolute, or relative to the data folder).
    pub fn resolve(p: &str) -> PathBuf {
        let base = Self::dir().unwrap_or_default();
        resolve_against(&base, Path::new(p))
    }

    /// Inverse of `resolve`: store paths inside the data folder as relative ones (keeps the folder portable).
    pub fn relativize(abs: &Path) -> String {
        match Self::dir() {
            Some(base) => relativize_against(&base, abs),
            None => abs.to_string_lossy().to_string(),
        }
    }

    /// Dictionary folder: `dict_dir` from the config (may be relative) or <data>/dict.
    pub fn dict_path(&self) -> PathBuf {
        match &self.dict_dir {
            Some(p) => Self::resolve(&p.to_string_lossy()),
            None => Self::dir().unwrap_or_default().join("dict"),
        }
    }

    pub fn is_language_disabled(&self, lang: Lang) -> bool {
        self.disabled_languages.iter().any(|c| Lang::from_code(c) == Some(lang))
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::dir().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("config.toml"), toml::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Read an existing config (without creating anything).
    pub fn load() -> Result<Self> {
        let path = Self::path().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
        Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
    }

    /// Read the config, creating the data folder, the default config and the default sounds on first run.
    pub fn load_or_create() -> Result<Self> {
        let dir = Self::dir().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
        std::fs::create_dir_all(&dir)?;
        ensure_default_sounds(&dir);
        if dir.join("config.toml").exists() {
            Self::load()
        } else {
            let cfg = Config::default();
            cfg.save()?;
            Ok(cfg)
        }
    }

    /// Bump the modification time of config.toml so a running Retypr reloads (e.g. after a dictionary download).
    pub fn touch() {
        if let Some(p) = Self::path() {
            if let Ok(f) = std::fs::OpenOptions::new().write(true).open(p) {
                let _ = f.set_modified(std::time::SystemTime::now());
            }
        }
    }
}

/// Write the bundled default sounds into <data>/sounds if they are missing.
fn ensure_default_sounds(data_dir: &Path) {
    let dir = data_dir.join("sounds");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    for (name, bytes) in [("manual-switch.mp3", MANUAL_SWITCH_MP3), ("automatic-correction.mp3", AUTO_CORRECTION_MP3)] {
        let p = dir.join(name);
        if !p.exists() {
            let _ = std::fs::write(p, bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_and_absolute_paths() {
        let base = Path::new("/apps/Retypr/data");
        assert_eq!(resolve_against(base, Path::new("sounds/a.mp3")), base.join("sounds/a.mp3"));
        let abs = if cfg!(windows) { Path::new("C:\\x\\y.mp3") } else { Path::new("/x/y.mp3") };
        assert_eq!(resolve_against(base, abs), abs);
    }

    #[test]
    fn relativize_keeps_portability() {
        let base = Path::new("/apps/Retypr/data");
        assert_eq!(relativize_against(base, &base.join("sounds").join("a.mp3")), "sounds/a.mp3");
        let outside = if cfg!(windows) { Path::new("C:\\other\\a.mp3") } else { Path::new("/other/a.mp3") };
        assert_eq!(relativize_against(base, outside), outside.to_string_lossy());
    }

    #[test]
    fn defaults() {
        let c = Config::default();
        assert!((c.sounds.volume - 0.3).abs() < 1e-6);
        assert_eq!(c.sounds.on_manual_switch, "sounds/manual-switch.mp3");
        assert_eq!(c.sounds.on_auto_switch, "sounds/automatic-correction.mp3");
        assert_eq!(c.sounds.on_selection_convert, "sounds/manual-switch.mp3");
        assert_eq!(c.sounds.on_rule_toggle, "");
        assert!(c.disabled_languages.is_empty());
    }

    #[test]
    fn toml_roundtrip() {
        let a = toml::to_string_pretty(&Config::default()).unwrap();
        let back: Config = toml::from_str(&a).unwrap();
        assert_eq!(toml::to_string_pretty(&back).unwrap(), a);
    }

    #[test]
    fn old_config_files_still_load() {
        // fields from older versions are ignored; missing sound fields get the new defaults
        let old = r#"
            auto_switch = false
            reselect_after_convert = true
            auto_languages = ["en", "ru"]
            [sounds]
            enabled = true
            volume = 0.7
            on_manual_switch = "C:\\sounds\\old.mp3"
        "#;
        let c: Config = toml::from_str(old).unwrap();
        assert!(!c.auto_switch);
        assert_eq!(c.sounds.on_manual_switch, "C:\\sounds\\old.mp3");
        assert_eq!(c.sounds.on_auto_switch, "sounds/automatic-correction.mp3");
        assert!((c.sounds.volume - 0.7).abs() < 1e-6);
    }

    #[test]
    fn explicit_empty_sound_means_none() {
        let c: Config = toml::from_str("[sounds]\non_auto_switch = \"\"\n").unwrap();
        assert_eq!(c.sounds.on_auto_switch, "");
        assert_eq!(c.sounds.on_manual_switch, "sounds/manual-switch.mp3");
    }

    #[test]
    fn language_codes() {
        let mut c = Config::default();
        c.disabled_languages = vec!["DE".into(), "uk".into()];
        assert!(c.is_language_disabled(Lang(0x07)));
        assert!(c.is_language_disabled(Lang::UK));
        assert!(!c.is_language_disabled(Lang::EN));
    }
}
