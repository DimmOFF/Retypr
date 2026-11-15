//! Решает, набрано ли слово в неправильной раскладке, и ВЫБИРАЕТ раскладку, в которую нужно переключиться
//! (поддерживаются EN, RU, UK — любые комбинации установленных в системе раскладок).
//!
//! Для каждой раскладки-кандидата:
//!  1. Защитные фильтры: цифры, смешанные алфавиты, АКРОНИМЫ, исключения, спецсимволы кода.
//!  2. Словари: слово известно в своём языке → не трогаем совсем;
//!     его конвертация известна в языке кандидата → кандидат подтверждён словарём.
//!  3. Иначе строгая эвристика: оригинал фонетически невозможен, а конвертация — возможна.
//! Побеждает кандидат, подтверждённый словарём; если таких нет — первый по порядку раскладок системы.
use crate::{
    config::Config,
    layout_map::{self, Lang},
    os::LayoutId,
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

pub struct Detector {
    dicts: HashMap<Lang, HashSet<String>>,
    exc: HashSet<String>,
    min_len: usize,
}

fn norm(s: &str) -> String {
    s.to_lowercase().replace('ё', "е").replace(['’', 'ʼ'], "'")
}

/// Словарь: по слову в строке; допускается формат «слово частота» (FrequencyWords); `#` — комментарий.
fn load(path: PathBuf) -> HashSet<String> {
    std::fs::read_to_string(path)
        .map(|s| {
            s.lines()
                .filter_map(|l| {
                    let l = l.trim().trim_start_matches('\u{feff}');
                    if l.is_empty() || l.starts_with('#') {
                        None
                    } else {
                        l.split_whitespace().next().map(norm)
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Символы, которые в другой раскладке становятся буквами ([ ] ; ' , . ` и т.д.), плюс дефис.
fn letter_like(c: char) -> bool {
    c.is_alphabetic() || "[];',.`{}:\"<>~-".contains(c)
}

enum Verdict {
    /// слово корректно в своём языке — не конвертировать вообще
    Own,
    No,
    Convert { by_dict: bool },
}

impl Detector {
    pub fn new(cfg: &Config) -> Self {
        let dir = cfg.dict_dir.clone().or_else(|| Config::dir().map(|d| d.join("dict")));
        let mut dicts = HashMap::new();
        let mut exc = HashSet::new();
        if let Some(d) = dir {
            for (lang, file) in [(Lang::En, "en.txt"), (Lang::Ru, "ru.txt"), (Lang::Uk, "uk.txt")] {
                let set = load(d.join(file));
                log::info!("словарь {lang:?}: {} слов", set.len());
                if !set.is_empty() {
                    dicts.insert(lang, set);
                }
            }
            exc = load(d.join("exceptions.txt"));
        }
        Self { dicts, exc, min_len: cfg.min_word_len }
    }

    /// Выбрать раскладку, в которую нужно переключить слово `word`, набранное в раскладке `from`.
    /// `conv(text, from, to)` — перевод текста между раскладками (на Windows — через сами раскладки).
    pub fn pick_target<F>(
        &self,
        word: &str,
        from: (LayoutId, Lang),
        cands: &[(LayoutId, Lang)],
        conv: F,
    ) -> Option<LayoutId>
    where
        F: Fn(&str, LayoutId, LayoutId) -> String,
    {
        let chars: Vec<char> = word.chars().collect();
        if chars.is_empty() || chars.iter().any(|c| c.is_ascii_digit()) {
            return None;
        }
        let (fid, fl) = from;
        let mut best: Option<LayoutId> = None;

        for &(tid, tl) in cands {
            let c: Vec<char> = conv(word, fid, tid).chars().collect();
            if c.len() != chars.len() {
                continue;
            }
            // Хвостовая пунктуация (до 3 знаков), не буква ни в одной раскладке: «ghbdtn!» → «ghbdtn»
            let mut n = 0;
            while n < 3 && n < chars.len() {
                let i = chars.len() - 1 - n;
                if !chars[i].is_alphabetic() && !c[i].is_alphabetic() {
                    n += 1;
                } else {
                    break;
                }
            }
            let core_o: String = chars[..chars.len() - n].iter().collect();
            let core_c: String = c[..c.len() - n].iter().collect();
            if !core_o.chars().all(letter_like) {
                continue; // скобки, слэши, @ и т.п. внутри — это код/путь/адрес
            }
            match self.eval(&core_o, fl, &core_c, tl) {
                Verdict::Own => return None,
                Verdict::Convert { by_dict: true } => return Some(tid),
                Verdict::Convert { by_dict: false } => {
                    if best.is_none() {
                        best = Some(tid);
                    }
                }
                Verdict::No => {}
            }
        }
        best
    }

    fn dict(&self, l: Lang) -> Option<&HashSet<String>> {
        self.dicts.get(&l)
    }

    fn eval(&self, o: &str, fl: Lang, c: &str, tl: Lang) -> Verdict {
        if o.chars().count() < self.min_len {
            return Verdict::No;
        }
        let has_cyr = o.chars().any(layout_map::is_cyrillic);
        let has_lat = o.chars().any(|ch| ch.is_ascii_alphabetic());
        if has_cyr == has_lat || has_cyr != fl.is_cyrillic() {
            return Verdict::No; // смесь алфавитов, нет букв или слово не того алфавита, что раскладка
        }
        // АКРОНИМЫ (USB, TCP) не трогаем
        let alpha: Vec<char> = o.chars().filter(|ch| ch.is_alphabetic()).collect();
        if alpha.len() >= 2 && alpha.iter().all(|ch| ch.is_uppercase()) {
            return Verdict::No;
        }
        let (ol, cl) = (norm(o), norm(c));
        if self.exc.contains(&ol) {
            return Verdict::Own;
        }
        if let Some(own) = self.dict(fl) {
            if own.contains(&ol) {
                return Verdict::Own;
            }
            if let Some(oth) = self.dict(tl) {
                if oth.contains(&cl) {
                    return Verdict::Convert { by_dict: true };
                }
            }
        }
        // слово неизвестно словарям (имя, сленг) либо словарей нет — строгая эвристика
        if implausible(o, fl) && !implausible(c, tl) {
            Verdict::Convert { by_dict: false }
        } else {
            Verdict::No
        }
    }
}

fn implausible(w: &str, l: Lang) -> bool {
    match l {
        Lang::En => bad_en(w),
        Lang::Ru => bad_ru(w),
        Lang::Uk => bad_uk(w),
    }
}

const BAD_EN: &[&str] = &[
    "bq", "bx", "bz", "cj", "cv", "cx", "dx", "fq", "fv", "fx", "fz", "gq", "gv", "gx", "hx", "hz",
    "jb", "jc", "jd", "jf", "jg", "jh", "jk", "jl", "jm", "jn", "jp", "jq", "jr", "jt", "jv", "jw",
    "jx", "jz", "kq", "kx", "kz", "mx", "mz", "pq", "pv", "px", "qb", "qc", "qd", "qf", "qg", "qh",
    "qj", "qk", "ql", "qm", "qn", "qp", "qr", "qs", "qt", "qv", "qw", "qx", "qy", "qz", "sx", "tq",
    "vb", "vc", "vd", "vf", "vg", "vh", "vj", "vk", "vm", "vn", "vp", "vq", "vt", "vw", "vx", "vz",
    "wq", "wv", "wx", "wz", "xd", "xg", "xj", "xk", "xq", "xv", "xw", "xz", "yq", "yx", "zb", "zc",
    "zd", "zf", "zg", "zj", "zk", "zp", "zq", "zr", "zv", "zx",
];

/// «Не английское»: нет гласных, ≥5 согласных подряд, невозможные биграммы, тройные буквы,
/// а также `[`, `]`, `;` перед буквой (это клавиши х, ъ/ї, ж русской/украинской раскладки).
fn bad_en(w: &str) -> bool {
    let lw: Vec<char> = w.to_lowercase().chars().collect();
    for i in 0..lw.len().saturating_sub(1) {
        if matches!(lw[i], '[' | ']' | ';') && lw[i + 1].is_alphabetic() {
            return true;
        }
    }
    let l: Vec<char> = lw.iter().copied().filter(|c| c.is_ascii_alphabetic()).collect();
    if l.len() < 2 {
        return false;
    }
    let vowel = |i: usize| matches!(l[i], 'a' | 'e' | 'i' | 'o' | 'u') || (l[i] == 'y' && i > 0);
    if !(0..l.len()).any(|i| vowel(i)) {
        return true;
    }
    let mut run = 0;
    for i in 0..l.len() {
        if vowel(i) {
            run = 0;
        } else {
            run += 1;
            if run >= 5 {
                return true;
            }
        }
    }
    for i in 0..l.len() - 1 {
        let pair: String = [l[i], l[i + 1]].iter().collect();
        if BAD_EN.contains(&pair.as_str()) || (l[i] == 'q' && l[i + 1] != 'u') {
            return true;
        }
        if i + 2 < l.len() && l[i] == l[i + 1] && l[i + 1] == l[i + 2] {
            return true;
        }
    }
    false
}

const RU_VOW: &str = "аеёиоуыэюя";
const RU_CONS: &str = "бвгджзйклмнпрстфхцчшщ";
const BAD_RU: &[&str] = &["жы", "шы", "щы", "чы", "щю", "чя", "щя", "жя", "шя"];

/// «Не русское»: начинается с ъ/ы/ь, кончается на ъ, нет гласных, ≥5 согласных подряд,
/// гласная перед ы/ь/ъ, ъ не перед е/ё/ю/я, невозможные сочетания (жы, шы, чя…).
fn bad_ru(w: &str) -> bool {
    let l: Vec<char> = w
        .to_lowercase()
        .chars()
        .filter(|&c| ('а'..='я').contains(&c) || c == 'ё')
        .collect();
    if l.is_empty() {
        return false;
    }
    if matches!(l[0], 'ъ' | 'ы' | 'ь') || l[l.len() - 1] == 'ъ' {
        return true;
    }
    if l.len() >= 3 && !l.iter().any(|&c| RU_VOW.contains(c)) {
        return true;
    }
    let mut run = 0;
    for &c in &l {
        if RU_CONS.contains(c) {
            run += 1;
            if run >= 5 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    for i in 0..l.len() - 1 {
        let (a, b) = (l[i], l[i + 1]);
        let pair: String = [a, b].iter().collect();
        if BAD_RU.contains(&pair.as_str())
            || (RU_VOW.contains(a) && matches!(b, 'ы' | 'ь' | 'ъ'))
            || (a == 'ъ' && !matches!(b, 'е' | 'ё' | 'ю' | 'я'))
            || (a == 'ь' && matches!(b, 'ъ' | 'ь' | 'ы'))
        {
            return true;
        }
    }
    false
}

const UK_VOW: &str = "аеєиіїоуюя";
const UK_CONS: &str = "бвгґджзйклмнпрстфхцчшщ";

/// «Не украинское»: есть ы/э/ъ/ё (в украинском их нет), начинается с ь, нет гласных,
/// ≥5 согласных подряд, гласная перед ь. Апостроф (') допустим внутри слова.
fn bad_uk(w: &str) -> bool {
    let lw = w.to_lowercase();
    if lw.contains(['ы', 'э', 'ъ', 'ё']) {
        return true;
    }
    let l: Vec<char> = lw
        .chars()
        .filter(|&c| ('а'..='я').contains(&c) || "ґєії".contains(c))
        .collect();
    if l.is_empty() {
        return false;
    }
    if l[0] == 'ь' {
        return true;
    }
    if l.len() >= 3 && !l.iter().any(|&c| UK_VOW.contains(c)) {
        return true;
    }
    let mut run = 0;
    for &c in &l {
        if UK_CONS.contains(c) {
            run += 1;
            if run >= 5 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    for i in 0..l.len() - 1 {
        if UK_VOW.contains(l[i]) && l[i + 1] == 'ь' {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const EN: (LayoutId, Lang) = (1, Lang::En);
    const RU: (LayoutId, Lang) = (2, Lang::Ru);
    const UK: (LayoutId, Lang) = (3, Lang::Uk);

    fn lang(id: LayoutId) -> Lang {
        match id {
            1 => Lang::En,
            2 => Lang::Ru,
            _ => Lang::Uk,
        }
    }
    fn conv(t: &str, f: LayoutId, to: LayoutId) -> String {
        layout_map::convert_between(t, lang(f), lang(to))
    }

    fn det(dicts: &[(Lang, &[&str])]) -> Detector {
        Detector {
            dicts: dicts
                .iter()
                .map(|(l, w)| (*l, w.iter().map(|s| s.to_string()).collect()))
                .collect(),
            exc: HashSet::new(),
            min_len: 3,
        }
    }

    fn pick(d: &Detector, w: &str, from: (LayoutId, Lang), cands: &[(LayoutId, Lang)]) -> Option<LayoutId> {
        d.pick_target(w, from, cands, conv)
    }

    #[test]
    fn heuristic_en_ru() {
        let d = det(&[]);
        let c = [RU];
        assert_eq!(pick(&d, "ghbdtn", EN, &c), Some(2)); // привет
        assert_eq!(pick(&d, "rf;lsq", EN, &c), Some(2)); // каждый
        assert_eq!(pick(&d, "ntrcn", EN, &c), Some(2)); // текст
        assert_eq!(pick(&d, "hello", EN, &c), None);
        assert_eq!(pick(&d, "привет", RU, &[EN]), None);
        assert_eq!(pick(&d, "TCP", EN, &c), None); // акроним
        assert_eq!(pick(&d, "USB", EN, &c), None);
        assert_eq!(pick(&d, "ok", EN, &c), None); // короткое
    }

    #[test]
    fn code_is_left_alone() {
        let d = det(&[]);
        for w in ["return;", "arr[i]", "arr[0]", "if(x)", "file_1"] {
            assert_eq!(pick(&d, w, EN, &[RU, UK]), None, "{w}");
        }
    }

    #[test]
    fn dict_picks_right_language() {
        let d = det(&[
            (Lang::En, &["hello", "world"]),
            (Lang::Ru, &["привет", "мир"]),
            (Lang::Uk, &["привіт", "світ"]),
        ]);
        let c = [RU, UK];
        assert_eq!(pick(&d, "ghbdtn", EN, &c), Some(2)); // привет → RU
        assert_eq!(pick(&d, "ghbdsn", EN, &c), Some(3)); // привіт → UK
        assert_eq!(pick(&d, "ghbdtn!", EN, &c), Some(2)); // хвостовая пунктуация
        assert_eq!(pick(&d, "hello", EN, &c), None); // слово корректно в своём языке
        assert_eq!(pick(&d, "руддщ", RU, &[EN, UK]), Some(1)); // hello, набрано в RU
        assert_eq!(pick(&d, "привіт", UK, &[EN, RU]), None);
    }

    #[test]
    fn uk_heuristics() {
        assert!(bad_uk("привыт")); // ы нет в украинском
        assert!(!bad_uk("привіт"));
        assert!(!bad_uk("п'ять"));
        assert!(bad_uk("ьаб"));
    }

    #[test]
    fn exceptions() {
        let mut d = det(&[]);
        d.exc.insert("ghbdtn".into());
        assert_eq!(pick(&d, "ghbdtn", EN, &[RU]), None);
    }
}
