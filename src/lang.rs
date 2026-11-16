//! Languages of the keyboard layouts installed in the system.
//!
//! A language is identified by the *primary* Windows LANGID (the low 10 bits of a layout's language id),
//! e.g. 0x09 = English, 0x19 = Russian, 0x22 = Ukrainian, 0x07 = German. Nothing here is limited to a fixed
//! set of languages: whatever layouts the user installed in Windows are what Retypr works with.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Lang(pub u16);

struct Info {
    id: u16,
    code: &'static str,
    name: &'static str,
    /// Latin-script language? (auto-correction between layouts of different scripts is the reliable case)
    latin: bool,
}

const fn i(id: u16, code: &'static str, name: &'static str, latin: bool) -> Info {
    Info { id, code, name, latin }
}

/// Common languages. Anything not listed still works (code "l<hex id>", treated as non-Latin).
const TABLE: &[Info] = &[
    i(0x01, "ar", "Arabic", false),
    i(0x02, "bg", "Bulgarian", false),
    i(0x03, "ca", "Catalan", true),
    i(0x04, "zh", "Chinese", false),
    i(0x05, "cs", "Czech", true),
    i(0x06, "da", "Danish", true),
    i(0x07, "de", "German", true),
    i(0x08, "el", "Greek", false),
    i(0x09, "en", "English", true),
    i(0x0A, "es", "Spanish", true),
    i(0x0B, "fi", "Finnish", true),
    i(0x0C, "fr", "French", true),
    i(0x0D, "he", "Hebrew", false),
    i(0x0E, "hu", "Hungarian", true),
    i(0x0F, "is", "Icelandic", true),
    i(0x10, "it", "Italian", true),
    i(0x11, "ja", "Japanese", false),
    i(0x12, "ko", "Korean", false),
    i(0x13, "nl", "Dutch", true),
    i(0x14, "no", "Norwegian", true),
    i(0x15, "pl", "Polish", true),
    i(0x16, "pt", "Portuguese", true),
    i(0x18, "ro", "Romanian", true),
    i(0x19, "ru", "Russian", false),
    i(0x1A, "hr", "Croatian", true),
    i(0x1B, "sk", "Slovak", true),
    i(0x1C, "sq", "Albanian", true),
    i(0x1D, "sv", "Swedish", true),
    i(0x1E, "th", "Thai", false),
    i(0x1F, "tr", "Turkish", true),
    i(0x20, "ur", "Urdu", false),
    i(0x21, "id", "Indonesian", true),
    i(0x22, "uk", "Ukrainian", false),
    i(0x23, "be", "Belarusian", false),
    i(0x24, "sl", "Slovenian", true),
    i(0x25, "et", "Estonian", true),
    i(0x26, "lv", "Latvian", true),
    i(0x27, "lt", "Lithuanian", true),
    i(0x29, "fa", "Persian", false),
    i(0x2A, "vi", "Vietnamese", true),
    i(0x2B, "hy", "Armenian", false),
    i(0x2C, "az", "Azerbaijani", true),
    i(0x2F, "mk", "Macedonian", false),
    i(0x37, "ka", "Georgian", false),
    i(0x39, "hi", "Hindi", false),
    i(0x3E, "ms", "Malay", true),
    i(0x3F, "kk", "Kazakh", false),
    i(0x40, "ky", "Kyrgyz", false),
    i(0x45, "bn", "Bengali", false),
    i(0x4E, "mr", "Marathi", false),
    i(0x50, "mn", "Mongolian", false),
];

impl Lang {
    pub const EN: Lang = Lang(0x09);
    pub const RU: Lang = Lang(0x19);
    pub const UK: Lang = Lang(0x22);

    /// From a full Windows LANGID (as found in the low word of an HKL).
    pub fn from_langid(langid: u16) -> Lang {
        Lang(langid & 0x3FF)
    }

    fn info(self) -> Option<&'static Info> {
        TABLE.iter().find(|x| x.id == self.0)
    }

    /// Short code used for file names and config values: "en", "ru", "uk", "de"… (unknown: "l1a").
    pub fn code(self) -> String {
        self.info().map(|x| x.code.to_string()).unwrap_or_else(|| format!("l{:02x}", self.0))
    }

    /// English display name.
    pub fn name(self) -> String {
        self.info().map(|x| x.name.to_string()).unwrap_or_else(|| format!("Language 0x{:02X}", self.0))
    }

    pub fn is_latin(self) -> bool {
        self.info().map(|x| x.latin).unwrap_or(false)
    }

    pub fn from_code(s: &str) -> Option<Lang> {
        let s = s.trim().to_lowercase();
        let s = if s == "ua" { "uk".to_string() } else { s };
        if let Some(x) = TABLE.iter().find(|x| x.code == s) {
            return Some(Lang(x.id));
        }
        s.strip_prefix('l').and_then(|h| u16::from_str_radix(h, 16).ok()).map(Lang)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_roundtrip() {
        for x in TABLE {
            let l = Lang(x.id);
            assert_eq!(Lang::from_code(&l.code()), Some(l), "{}", x.code);
        }
        assert_eq!(Lang::from_code("UA"), Some(Lang::UK));
        assert_eq!(Lang(0x5F).code(), "l5f");
        assert_eq!(Lang::from_code("l5f"), Some(Lang(0x5F)));
        assert_eq!(Lang::from_code("zzz"), None);
    }

    #[test]
    fn from_langid_takes_primary_language() {
        assert_eq!(Lang::from_langid(0x0409), Lang::EN); // en-US
        assert_eq!(Lang::from_langid(0x0809), Lang::EN); // en-GB
        assert_eq!(Lang::from_langid(0x0419), Lang::RU);
        assert_eq!(Lang::from_langid(0x0422), Lang::UK);
    }

    #[test]
    fn scripts() {
        assert!(Lang::EN.is_latin());
        assert!(!Lang::RU.is_latin());
        assert!(!Lang::UK.is_latin());
        assert_eq!(Lang::EN.name(), "English");
    }

    #[test]
    fn table_ids_are_unique() {
        for (n, a) in TABLE.iter().enumerate() {
            assert!(TABLE.iter().skip(n + 1).all(|b| b.id != a.id && b.code != a.code), "{}", a.code);
        }
    }
}
