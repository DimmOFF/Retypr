//! Статические таблицы раскладок (по физическим клавишам). Нужны детектору в тестах и не-Windows бэкенду;
//! на Windows реальная конвертация идёт через сами раскладки системы (см. os/windows.rs).
use std::{collections::HashMap, sync::OnceLock};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Lang {
    En,
    Ru,
    Uk,
}

impl Lang {
    pub fn is_cyrillic(self) -> bool {
        self != Lang::En
    }

    pub fn from_code(s: &str) -> Option<Lang> {
        match s.trim().to_lowercase().as_str() {
            "en" => Some(Lang::En),
            "ru" => Some(Lang::Ru),
            "uk" | "ua" => Some(Lang::Uk),
            _ => None,
        }
    }
}

// Строки выровнены по физическим клавишам: строка 1, 2, 3, ` и \ — сначала без Shift, затем с Shift.
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

fn table(l: Lang) -> &'static Table {
    static ALL: OnceLock<[Table; 3]> = OnceLock::new();
    &ALL.get_or_init(|| [build(EN), build(RU), build(UK)])[l as usize]
}

pub fn is_cyrillic(c: char) -> bool {
    ('\u{0400}'..='\u{04FF}').contains(&c)
}

/// Текст, набранный в раскладке `from`, → тот же набор клавиш в раскладке `to`.
pub fn convert_between(text: &str, from: Lang, to: Lang) -> String {
    if from == to {
        return text.to_string();
    }
    let (f, t) = (table(from), table(to));
    text.chars().map(|c| f.idx.get(&c).map(|&i| t.chars[i]).unwrap_or(c)).collect()
}

#[allow(dead_code)]
/// Упрощённо: есть кириллица → RU→EN, иначе EN→RU (для не-Windows бэкенда и тестов).
pub fn convert(text: &str) -> String {
    if text.chars().any(is_cyrillic) {
        convert_between(text, Lang::Ru, Lang::En)
    } else {
        convert_between(text, Lang::En, Lang::Ru)
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
        assert_eq!(convert_between("ghbdsn", Lang::En, Lang::Uk), "привіт");
        assert_eq!(convert_between("ghbdsn", Lang::En, Lang::Ru), "привыт");
        assert_eq!(convert_between("\\", Lang::En, Lang::Uk), "ґ");
        assert_eq!(convert_between("ghbdsn", Lang::En, Lang::En), "ghbdsn");
    }

    #[test]
    fn ru_uk() {
        // те же клавиши: ы → і, э → є
        assert_eq!(convert_between("привыт", Lang::Ru, Lang::Uk), "привіт");
        assert_eq!(convert_between("привіт", Lang::Uk, Lang::En), "ghbdsn");
    }

    #[test]
    fn roundtrip() {
        assert_eq!(convert(&convert("ghbdtn")), "ghbdtn");
    }
}
