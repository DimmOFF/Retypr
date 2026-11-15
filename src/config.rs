use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub struct SoundConfig {
    pub enabled: bool,
    pub volume: f32,
    /// Путь к wav/mp3/ogg/flac. None = без звука для этого события.
    pub on_manual_switch: Option<PathBuf>,
    pub on_auto_switch: Option<PathBuf>,
    pub on_selection_convert: Option<PathBuf>,
    /// Звук при включении/выключении автопереключения для текущего приложения (Alt+Shift+Pause)
    pub on_rule_toggle: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum CaretSide {
    Left,
    #[default]
    Right,
}

/// Горячие клавиши: "Pause", "Shift+Pause", "Ctrl+Alt+F12", "Win+Scroll Lock"…
/// Модификаторы: Ctrl, Alt, Shift, Win. Клавиши: Pause, Break, ScrollLock, CapsLock, Insert, Delete, Home, End,
/// PageUp, PageDown, PrintScreen, Menu, Space, Tab, Enter, Esc, стрелки, F1–F24, A–Z, 0–9, Grave (`).
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
    /// Языки, между которыми работает автопереключение: "en", "ru", "uk". Нужна установленная в системе раскладка.
    pub auto_languages: Vec<String>,
    /// Приложения (имя exe), где отключено ТОЛЬКО автопереключение. Ручные Pause/Shift+Pause работают.
    pub auto_exclude_apps: Vec<String>,
    /// Приложения, где Retypr полностью «молчит» (клавиши, включая Pause, проходят как есть) — игры и т.п.
    pub disabled_apps: Vec<String>,
    /// Папка со словарями en.txt / ru.txt (по слову в строке)
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
            auto_languages: vec!["en".into(), "ru".into(), "uk".into()],
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
            sounds: SoundConfig {
                enabled: true,
                volume: 0.7,
                on_manual_switch: None,
                on_auto_switch: None,
                on_selection_convert: None,
                on_rule_toggle: None,
            },
            hotkeys: HotkeysConfig::default(),
        }
    }
}

impl Config {
    pub fn dir() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "retypr").map(|d| d.config_dir().to_path_buf())
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::dir().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("config.toml"), toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn path() -> Option<PathBuf> {
        Self::dir().map(|d| d.join("config.toml"))
    }

    /// Папка словарей: `dict_dir` из конфига или <config>/dict.
    pub fn dict_path(&self) -> PathBuf {
        self.dict_dir
            .clone()
            .or_else(|| Self::dir().map(|d| d.join("dict")))
            .unwrap_or_else(|| PathBuf::from("dict"))
    }

    /// Прочитать существующий конфиг (без создания).
    pub fn load() -> Result<Self> {
        let path = Self::path().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
        Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
    }

    pub fn load_or_create() -> Result<Self> {
        let dir = Self::dir().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
        std::fs::create_dir_all(&dir)?;
        if dir.join("config.toml").exists() {
            Self::load()
        } else {
            let cfg = Config::default();
            cfg.save()?;
            Ok(cfg)
        }
    }
}
