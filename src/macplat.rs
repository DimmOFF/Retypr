//! Pure helpers for the macOS backend (key codes, hashing, plist generation, ...).
//! They live outside `os/macos.rs` so that they compile - and are unit-tested - on every platform.
#![allow(dead_code)]
use crate::{lang::Lang, os::K};

/// Value stored in the "event source user data" field of every event we post ourselves, so that our
/// own event tap can recognize and ignore them.
pub const MARK: i64 = 0x5254_5950;

pub const KC_C: u16 = 8;
pub const KC_V: u16 = 9;
pub const KC_BACKSPACE: u16 = 51;
pub const KC_LEFT: u16 = 123;
pub const KC_RIGHT: u16 = 124;

/// macOS virtual key codes (kVK_ANSI_*, kVK_F*, ...) -> canonical hotkey names (see hotkey.rs).
/// Mac keyboards have no Pause/ScrollLock/PrintScreen keys: use F13-F15 for them.
const KEYS: &[(u16, &str)] = &[
    (0, "a"), (11, "b"), (8, "c"), (2, "d"), (14, "e"), (3, "f"), (5, "g"), (4, "h"), (34, "i"),
    (38, "j"), (40, "k"), (37, "l"), (46, "m"), (45, "n"), (31, "o"), (35, "p"), (12, "q"),
    (15, "r"), (1, "s"), (17, "t"), (32, "u"), (9, "v"), (13, "w"), (7, "x"), (16, "y"), (6, "z"),
    (29, "0"), (18, "1"), (19, "2"), (20, "3"), (21, "4"), (23, "5"), (22, "6"), (26, "7"),
    (28, "8"), (25, "9"),
    (50, "grave"), (49, "space"), (48, "tab"), (36, "enter"), (76, "enter"), (51, "backspace"),
    (53, "escape"), (117, "delete"), (114, "insert"), (115, "home"), (119, "end"),
    (116, "pageup"), (121, "pagedown"), (123, "left"), (124, "right"), (125, "down"), (126, "up"),
    (122, "f1"), (120, "f2"), (99, "f3"), (118, "f4"), (96, "f5"), (97, "f6"), (98, "f7"),
    (100, "f8"), (101, "f9"), (109, "f10"), (103, "f11"), (111, "f12"), (105, "f13"),
    (107, "f14"), (113, "f15"), (106, "f16"), (64, "f17"), (79, "f18"), (80, "f19"), (90, "f20"),
];

pub fn key_name(code: u16) -> &'static str {
    KEYS.iter().find(|(c, _)| *c == code).map(|(_, n)| *n).unwrap_or("")
}

/// Modifier keys (they arrive as "flags changed" events).
pub fn modifier_kind(code: u16) -> Option<K> {
    match code {
        56 | 60 => Some(K::Shift),
        59 | 62 => Some(K::Ctrl),
        58 | 61 => Some(K::Alt), // Option
        55 | 54 => Some(K::Meta), // Command
        _ => None,
    }
}

/// How the engine should treat a (non-modifier) key.
pub fn classify(code: u16) -> K {
    match code {
        51 => K::Backspace,
        49 => K::Space,
        // Return, keypad Enter, Tab, Escape, Help/Insert, Home, End, PageUp, PageDown, Delete, arrows
        36 | 76 | 48 | 53 | 114 | 115 | 119 | 116 | 121 | 117 | 123..=126 => K::Reset,
        _ => K::Char,
    }
}

/// The modifier state as UCKeyTranslate wants it: (Carbon modifier mask >> 8).
pub fn uc_modifier_state(shift: bool, caps: bool, option: bool) -> u32 {
    (if shift { 0x02 } else { 0 }) | (if caps { 0x04 } else { 0 }) | (if option { 0x08 } else { 0 })
}

pub fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// A stable, non-zero layout id derived from an input source id such as "com.apple.keylayout.Russian".
pub fn layout_id(source_id: &str) -> isize {
    let h = fnv1a(source_id) as isize;
    if h == 0 { 1 } else { h }
}

/// TIS reports languages as BCP-47 tags ("ru", "en-US", "zh-Hans").
pub fn lang_from_tis(tag: &str) -> Option<Lang> {
    let primary = tag.split(|c| c == '-' || c == '_').next().unwrap_or("");
    Lang::from_code(primary)
}

/// Executable name of a process, lowercase: ".../Visual Studio Code.app/Contents/MacOS/Electron" -> "electron".
pub fn exe_name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A LaunchAgent that starts the app at login.
pub fn launch_agent_plist(label: &str, exe: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>ProcessType</key><string>Interactive</string>
</dict>
</plist>
"#,
        xml_escape(label),
        xml_escape(exe)
    )
}

/// Escape a string for an AppleScript string literal.
pub fn osascript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey;

    #[test]
    fn key_names_are_canonical_hotkey_names() {
        for (_, name) in KEYS {
            assert_eq!(hotkey::canonical(name), Some(*name), "{name}");
        }
    }

    #[test]
    fn key_codes_are_unique() {
        for (i, (code, name)) in KEYS.iter().enumerate() {
            assert!(KEYS.iter().skip(i + 1).all(|(c, _)| c != code), "duplicate key code {code} ({name})");
        }
    }

    #[test]
    fn well_known_codes() {
        assert_eq!(key_name(0), "a");
        assert_eq!(key_name(6), "z");
        assert_eq!(key_name(49), "space");
        assert_eq!(key_name(113), "f15");
        assert_eq!(key_name(999), "");
    }

    #[test]
    fn classification() {
        assert_eq!(classify(51), K::Backspace);
        assert_eq!(classify(49), K::Space);
        assert_eq!(classify(36), K::Reset);
        assert_eq!(classify(123), K::Reset);
        assert_eq!(classify(0), K::Char);
        assert_eq!(modifier_kind(56), Some(K::Shift));
        assert_eq!(modifier_kind(61), Some(K::Alt));
        assert_eq!(modifier_kind(55), Some(K::Meta));
        assert_eq!(modifier_kind(0), None);
    }

    #[test]
    fn modifier_state_bits() {
        assert_eq!(uc_modifier_state(false, false, false), 0);
        assert_eq!(uc_modifier_state(true, false, false), 2);
        assert_eq!(uc_modifier_state(true, true, true), 14);
    }

    #[test]
    fn layout_ids_are_stable_and_distinct() {
        let a = layout_id("com.apple.keylayout.US");
        assert_eq!(a, layout_id("com.apple.keylayout.US"));
        assert_ne!(a, layout_id("com.apple.keylayout.Russian"));
        assert_ne!(a, 0);
    }

    #[test]
    fn languages_from_tis_tags() {
        assert_eq!(lang_from_tis("ru"), Some(Lang::RU));
        assert_eq!(lang_from_tis("en-US"), Some(Lang::EN));
        assert_eq!(lang_from_tis("uk"), Some(Lang::UK));
        assert_eq!(lang_from_tis("zh-Hans"), Lang::from_code("zh"));
        assert_eq!(lang_from_tis("xx"), None);
    }

    #[test]
    fn executable_names() {
        assert_eq!(
            exe_name_from_path("/Applications/Visual Studio Code.app/Contents/MacOS/Electron"),
            "electron"
        );
        assert_eq!(exe_name_from_path("/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal"), "terminal");
        assert_eq!(exe_name_from_path(""), "");
    }

    #[test]
    fn plist_is_escaped() {
        let p = launch_agent_plist("com.retypr.app", "/Apps/R&D <x>/Retypr");
        assert!(p.contains("<string>/Apps/R&amp;D &lt;x&gt;/Retypr</string>"));
        assert!(p.contains("<key>RunAtLoad</key><true/>"));
        assert_eq!(osascript_escape("say \"hi\" \\ ok"), "say \\\"hi\\\" \\\\ ok");
    }
}
