//! Configurable hotkeys: parsing strings like "Ctrl+Alt+F12" and looking up the action for a key press.
use crate::config::HotkeysConfig;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// convert the last word (press again: the next layout)
    Word,
    /// convert the selected text
    Selection,
    /// turn auto-switching on/off globally
    ToggleAuto,
    /// turn auto-switching on/off for the current app
    ToggleApp,
}

pub const LETTERS: &str = "abcdefghijklmnopqrstuvwxyz";
pub const DIGITS: &str = "0123456789";
pub const FKEYS: [&str; 24] = [
    "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12", "f13", "f14", "f15",
    "f16", "f17", "f18", "f19", "f20", "f21", "f22", "f23", "f24",
];
const FIXED: &[&str] = &[
    "pause", "break", "scrolllock", "capslock", "insert", "delete", "home", "end", "pageup",
    "pagedown", "printscreen", "menu", "space", "backspace", "tab", "enter", "escape", "left",
    "right", "up", "down", "grave",
];

/// Map a key name (aliases allowed) to its canonical static name.
pub fn canonical(name: &str) -> Option<&'static str> {
    let n = name.trim().to_lowercase().replace(' ', "");
    let canon: String = match n.as_str() {
        "esc" => "escape".into(),
        "return" => "enter".into(),
        "del" => "delete".into(),
        "ins" => "insert".into(),
        "pgup" => "pageup".into(),
        "pgdn" => "pagedown".into(),
        "scroll" => "scrolllock".into(),
        "caps" => "capslock".into(),
        "prtsc" | "prtscn" | "printscr" => "printscreen".into(),
        "apps" => "menu".into(),
        "cancel" => "break".into(),
        "`" | "tilde" | "backtick" => "grave".into(),
        _ => n,
    };
    if let Some(k) = FIXED.iter().chain(FKEYS.iter()).copied().find(|k| *k == canon) {
        return Some(k);
    }
    if canon.len() == 1 {
        if let Some(i) = LETTERS.find(canon.as_str()) {
            return Some(&LETTERS[i..i + 1]);
        }
        if let Some(i) = DIGITS.find(canon.as_str()) {
            return Some(&DIGITS[i..i + 1]);
        }
    }
    None
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    pub key: &'static str,
}

/// All supported key names (for the drop-down in the settings).
pub fn all_keys() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = FIXED.iter().chain(FKEYS.iter()).copied().collect();
    for i in 0..26 {
        v.push(&LETTERS[i..i + 1]);
    }
    for i in 0..10 {
        v.push(&DIGITS[i..i + 1]);
    }
    v
}

/// Display name of a key: "pause" -> "Pause", "scrolllock" -> "ScrollLock", "f5" -> "F5", "a" -> "A".
pub fn pretty(k: &str) -> String {
    match k {
        "scrolllock" => "ScrollLock".into(),
        "capslock" => "CapsLock".into(),
        "pageup" => "PageUp".into(),
        "pagedown" => "PageDown".into(),
        "printscreen" => "PrintScreen".into(),
        _ if k.len() == 1 => k.to_uppercase(),
        _ if k.len() > 1 && k.starts_with('f') && k[1..].chars().all(|c| c.is_ascii_digit()) => k.to_uppercase(),
        _ => {
            let mut c = k.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        }
    }
}

impl Hotkey {
    /// String for config.toml: "Ctrl+Alt+Pause".
    pub fn to_config_string(&self) -> String {
        let mut p: Vec<String> = vec![];
        if self.ctrl { p.push("Ctrl".into()); }
        if self.alt { p.push("Alt".into()); }
        if self.shift { p.push("Shift".into()); }
        if self.meta { p.push("Win".into()); }
        p.push(pretty(self.key));
        p.join("+")
    }

    pub fn parse(s: &str) -> Result<Hotkey, String> {
        let parts: Vec<&str> = s.split('+').map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
        let Some((key, mods)) = parts.split_last() else {
            return Err("empty shortcut".into());
        };
        let mut h = Hotkey { ctrl: false, alt: false, shift: false, meta: false, key: "" };
        for m in mods {
            match m.to_lowercase().as_str() {
                "ctrl" | "control" => h.ctrl = true,
                "alt" => h.alt = true,
                "shift" => h.shift = true,
                "win" | "meta" | "cmd" | "super" => h.meta = true,
                other => return Err(format!("unknown modifier \"{other}\"")),
            }
        }
        h.key = canonical(key).ok_or_else(|| format!("unknown key \"{key}\""))?;
        Ok(h)
    }

    pub fn matches(&self, key: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> bool {
        self.key == key && self.ctrl == ctrl && self.alt == alt && self.shift == shift && self.meta == meta
    }
}

pub struct Bindings(Vec<(Hotkey, Action)>);

impl Bindings {
    /// Returns the bindings and a list of warnings (broken or duplicate shortcuts fall back to the defaults).
    pub fn from_config(c: &HotkeysConfig) -> (Self, Vec<String>) {
        let d = HotkeysConfig::default();
        let items = [
            ("convert_word", &c.convert_word, &d.convert_word, Action::Word),
            ("convert_selection", &c.convert_selection, &d.convert_selection, Action::Selection),
            ("toggle_auto", &c.toggle_auto, &d.toggle_auto, Action::ToggleAuto),
            ("toggle_app_auto", &c.toggle_app_auto, &d.toggle_app_auto, Action::ToggleApp),
        ];
        let (mut list, mut warns): (Vec<(Hotkey, Action)>, Vec<String>) = (vec![], vec![]);
        for (name, text, default, action) in items {
            let hk = match Hotkey::parse(text) {
                Ok(h) => h,
                Err(e) => {
                    warns.push(format!("hotkeys.{name} = \"{text}\": {e}. Using \"{default}\" instead."));
                    Hotkey::parse(default).expect("default hotkey")
                }
            };
            if let Some((_, other)) = list.iter().find(|(h, _)| *h == hk) {
                warns.push(format!("hotkeys.{name}: shortcut already used by {other:?}, skipped."));
                continue;
            }
            list.push((hk, action));
        }
        (Self(list), warns)
    }

    pub fn find(&self, key: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> Option<Action> {
        self.0.iter().find(|(h, _)| h.matches(key, ctrl, alt, shift, meta)).map(|(_, a)| *a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ok() {
        let h = Hotkey::parse("Shift+Pause").unwrap();
        assert!(h.shift && !h.ctrl && !h.alt && !h.meta && h.key == "pause");
        let h = Hotkey::parse(" ctrl + ALT + f12 ").unwrap();
        assert!(h.ctrl && h.alt && h.key == "f12");
        assert_eq!(Hotkey::parse("Esc").unwrap().key, "escape");
        assert_eq!(Hotkey::parse("Win+Scroll Lock").unwrap().key, "scrolllock");
        assert_eq!(Hotkey::parse("Ctrl+K").unwrap().key, "k");
        assert_eq!(Hotkey::parse("Ctrl+Break").unwrap().key, "break");
    }

    #[test]
    fn parse_err() {
        assert!(Hotkey::parse("").is_err());
        assert!(Hotkey::parse("Ctrl+").is_err());
        assert!(Hotkey::parse("Foo+Pause").is_err());
        assert!(Hotkey::parse("Ctrl+Banana").is_err());
    }

    #[test]
    fn to_string_roundtrip() {
        for s in ["Pause", "Shift+Pause", "Ctrl+Alt+F12", "Alt+Shift+ScrollLock", "Win+K", "Ctrl+PageUp", "Ctrl+5"] {
            assert_eq!(Hotkey::parse(s).unwrap().to_config_string(), s);
        }
        assert_eq!(all_keys().len(), 22 + 24 + 26 + 10);
        assert!(all_keys().iter().all(|k| canonical(k) == Some(*k)));
    }

    #[test]
    fn exact_modifiers() {
        let h = Hotkey::parse("Pause").unwrap();
        assert!(h.matches("pause", false, false, false, false));
        assert!(!h.matches("pause", false, false, true, false)); // Shift+Pause is a different shortcut
    }

    #[test]
    fn defaults_are_valid_and_distinct() {
        let (b, warns) = Bindings::from_config(&HotkeysConfig::default());
        assert!(warns.is_empty(), "{warns:?}");
        assert_eq!(b.find("pause", false, false, false, false), Some(Action::Word));
        assert_eq!(b.find("pause", false, false, true, false), Some(Action::Selection));
        assert_eq!(b.find("pause", false, true, false, false), Some(Action::ToggleAuto));
        assert_eq!(b.find("pause", false, true, true, false), Some(Action::ToggleApp));
    }

    #[test]
    fn bad_config_falls_back() {
        let mut c = HotkeysConfig::default();
        c.convert_word = "Nope".into();
        let (b, warns) = Bindings::from_config(&c);
        assert_eq!(warns.len(), 1);
        assert_eq!(b.find("pause", false, false, false, false), Some(Action::Word));
    }
}
