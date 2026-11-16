//! Static key tables for the EN / RU / UK layouts. They are used by the non-Windows backend and in tests;
//! on Windows the real conversion goes through the system layouts themselves (see os/windows.rs),
//! so it works for any installed layout.
use crate::lang::Lang;
use std::{collections::HashMap, sync::OnceLock};

// Rows are aligned by physical key: rows 1-3, then ` and \ - first without Shift, then with Shift.
const EN: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,./`\\QWERTYUIOP{}ASDFGHJKL:\"ZXCVBNM<>?~|";
const RU: &str = "йцукенгшщзхъфывапролджэячсмитьбю.ё\\ЙЦУКЕНГШЩЗХЪФЫВАПРОЛДЖЭЯЧСМИТЬБЮ,Ё/";
const UK: &str = "йцукенгшщзхїфівапролджєячсмитьбю.'ґЙЦУКЕНГШЩЗХЇФІВАПРОЛДЖЄЯЧСМИТЬБЮ,₴Ґ";

struct Table {
    chars: Vec<char>,
    idx: HashMap<char, usize>,
}

fn build(s: &str) -> Table {
    let chars: Vec<char> = s.chars().collect();
    let mut idx = HashMap::new();
    for (i, &c) in chars.iter().enumerate() {
        idx.entry(c).or_insert(i);
    }
    Table { chars, idx }
}

fn table(l: Lang) -> Option<&'static Table> {
    static ALL: OnceLock<[Table; 3]> = OnceLock::new();
    let all = ALL.get_or_init(|| [build(EN), build(RU), build(UK)]);
    match l {
        Lang::EN => Some(&all[0]),
        Lang::RU => Some(&all[1]),
        Lang::UK => Some(&all[2]),
        _ => None,
    }
}

pub fn is_cyrillic(c: char) -> bool {
    ('\u{0400}'..='\u{04FF}').contains(&c)
}

/// Text typed in layout `from` -> the same key presses in layout `to` (unchanged if either layout has no table).
pub fn convert_between(text: &str, from: Lang, to: Lang) -> String {
    if from == to {
        return text.to_string();
    }
    let (Some(f), Some(t)) = (table(from), table(to)) else {
        return text.to_string();
    };
    text.chars().map(|c| f.idx.get(&c).map(|&i| t.chars[i]).unwrap_or(c)).collect()
}

/// Simplified: contains Cyrillic -> RU->EN, otherwise EN->RU (non-Windows backend and tests).
#[allow(dead_code)]
pub fn convert(text: &str) -> String {
    if text.chars().any(is_cyrillic) {
        convert_between(text, Lang::RU, Lang::EN)
    } else {
        convert_between(text, Lang::EN, Lang::RU)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_aligned() {
        assert_eq!(EN.chars().count(), 70);
        assert_eq!(RU.chars().count(), 70);
        assert_eq!(UK.chars().count(), 70);
    }

    #[test]
    fn en_ru() {
        assert_eq!(convert("ghbdtn"), "привет");
        assert_eq!(convert("Ghbdtn"), "Привет");
        assert_eq!(convert("rf;lsq"), "каждый");
    }

    #[test]
    fn ru_en() {
        assert_eq!(convert("Руддщ"), "Hello");
    }

    #[test]
    fn en_uk() {
        assert_eq!(convert_between("ghbdsn", Lang::EN, Lang::UK), "привіт");
        assert_eq!(convert_between("ghbdsn", Lang::EN, Lang::RU), "привыт");
        assert_eq!(convert_between("\\", Lang::EN, Lang::UK), "ґ");
        assert_eq!(convert_between("ghbdsn", Lang::EN, Lang::EN), "ghbdsn");
    }

    #[test]
    fn ru_uk() {
        // same keys: ы -> і, э -> є
        assert_eq!(convert_between("привыт", Lang::RU, Lang::UK), "привіт");
        assert_eq!(convert_between("привіт", Lang::UK, Lang::EN), "ghbdsn");
    }

    #[test]
    fn unknown_language_is_left_alone() {
        assert_eq!(convert_between("hallo", Lang::EN, Lang(0x07)), "hallo");
    }

    #[test]
    fn roundtrip() {
        assert_eq!(convert(&convert("ghbdtn")), "ghbdtn");
    }
}
