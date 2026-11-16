//! Decides whether a word was typed in the wrong layout and PICKS the layout to switch to.
//! It works with whatever layouts are installed in the system (any languages).
//!
//! For every candidate layout:
//!  1. Safety filters: digits, mixed scripts, ACRONYMS, exceptions, code-like characters.
//!  2. Dictionaries (dict/<code>.txt): the word is known in its own language -> leave it alone;
//!     its conversion is known in the candidate's language -> confirmed by the dictionary.
//!  3. Otherwise, built-in rules (English, Russian, Ukrainian only) for layouts of DIFFERENT scripts:
//!     the original is phonetically impossible while the conversion is possible.
//!     Other languages are detected by dictionary only.
//! A dictionary-confirmed candidate wins; otherwise the first rule-based one (in system layout order).
use crate::{
    config::Config,
    lang::Lang,
    os::{self, LayoutId},
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

/// Dictionary file: one word per line; "word count" lines (FrequencyWords) are fine; `#` starts a comment.
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

/// Characters that become letters in another layout ([ ] ; ' , . ` etc.), plus the hyphen.
fn letter_like(c: char) -> bool {
    c.is_alphabetic() || "[];',.`{}:\"<>~-".contains(c)
}

fn is_latin_char(c: char) -> bool {
    c.is_alphabetic() && (c as u32) < 0x250
}

enum Verdict {
    /// the word is fine in its own language - do not convert at all
    Own,
    No,
    Convert { by_dict: bool },
}

impl Detector {
    pub fn new(cfg: &Config) -> Self {
        let dir = cfg.dict_path();
        let mut dicts = HashMap::new();
        // load only dictionaries of languages that have an installed layout and are enabled
        let mut seen = HashSet::new();
        for lang in os::layouts().into_iter().filter_map(os::layout_lang) {
            if !seen.insert(lang) || cfg.is_language_disabled(lang) {
                continue;
            }
            let set = load(dir.join(format!("{}.txt", lang.code())));
            log::info!("dictionary {}: {} words", lang.code(), set.len());
            if !set.is_empty() {
                dicts.insert(lang, set);
            }
        }
        Self { dicts, exc: load(dir.join("exceptions.txt")), min_len: cfg.min_word_len }
    }

    /// Pick the layout to which `word`, typed in layout `from`, should be switched.
    /// `conv(text, from, to)` converts text between layouts (on Windows: through the system layouts).
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
            // Trailing punctuation (up to 3 marks) that is not a letter in either layout: "ghbdtn!" -> "ghbdtn"
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
                continue; // brackets, slashes, @ and the like inside: code, a path or an address
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
        // the word must be written in one script, and it must be the script of its layout's language
        let letters: Vec<char> = o.chars().filter(|ch| ch.is_alphabetic()).collect();
        let all_latin = letters.iter().all(|&ch| is_latin_char(ch));
        let none_latin = letters.iter().all(|&ch| !is_latin_char(ch));
        if letters.is_empty() || all_latin == none_latin || all_latin != fl.is_latin() {
            return Verdict::No;
        }
        // ACRONYMS (USB, TCP) are left alone
        if letters.len() >= 2 && letters.iter().all(|ch| ch.is_uppercase()) {
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
        // Unknown to the dictionaries (names, slang) or no dictionaries: strict rules, and only between
        // layouts of different scripts (Latin <-> non-Latin). Same-script pairs rely on dictionaries alone.
        let cross_script = fl.is_latin() != tl.is_latin();
        if cross_script && implausible(o, fl) && !implausible(c, tl) {
            Verdict::Convert { by_dict: false }
        } else {
            Verdict::No
        }
    }
}

/// Built-in rules exist for English, Russian and Ukrainian; other languages never look "implausible".
fn implausible(w: &str, l: Lang) -> bool {
    if l == Lang::EN {
        bad_en(w)
    } else if l == Lang::RU {
        bad_ru(w)
    } else if l == Lang::UK {
        bad_uk(w)
    } else {
        false
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

/// "Not English": no vowels, 5+ consonants in a row, impossible bigrams, triple letters, and `[`, `]`, `;`
/// before a letter (those are the Cyrillic keys х, ъ/ї, ж).
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

/// "Not Russian": starts with ъ/ы/ь, ends with ъ, no vowels, 5+ consonants in a row, a vowel before ы/ь/ъ,
/// ъ not before е/ё/ю/я, impossible pairs (жы, шы, чя...).
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

/// "Not Ukrainian": contains ы/э/ъ/ё (absent from the alphabet), starts with ь, no vowels,
/// 5+ consonants in a row, a vowel before ь. The apostrophe is fine inside a word.
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
    use crate::layout_map;

    const DE: Lang = Lang(0x07);
    const EN: (LayoutId, Lang) = (1, Lang::EN);
    const RU: (LayoutId, Lang) = (2, Lang::RU);
    const UK: (LayoutId, Lang) = (3, Lang::UK);
    const GER: (LayoutId, Lang) = (4, DE);

    fn lang(id: LayoutId) -> Lang {
        match id {
            1 => Lang::EN,
            2 => Lang::RU,
            3 => Lang::UK,
            _ => DE,
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
        assert_eq!(pick(&d, "TCP", EN, &c), None); // acronym
        assert_eq!(pick(&d, "USB", EN, &c), None);
        assert_eq!(pick(&d, "ok", EN, &c), None); // too short
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
            (Lang::EN, &["hello", "world"]),
            (Lang::RU, &["привет", "мир"]),
            (Lang::UK, &["привіт", "світ"]),
        ]);
        let c = [RU, UK];
        assert_eq!(pick(&d, "ghbdtn", EN, &c), Some(2)); // привет -> RU
        assert_eq!(pick(&d, "ghbdsn", EN, &c), Some(3)); // привіт -> UK
        assert_eq!(pick(&d, "ghbdtn!", EN, &c), Some(2)); // trailing punctuation
        assert_eq!(pick(&d, "hello", EN, &c), None); // fine in its own language
        assert_eq!(pick(&d, "руддщ", RU, &[EN, UK]), Some(1)); // "hello" typed in the Russian layout
        assert_eq!(pick(&d, "привіт", UK, &[EN, RU]), None);
    }

    #[test]
    fn other_scripts_need_a_dictionary() {
        // No built-in rules for German: without a dictionary nothing is converted...
        let none = det(&[]);
        assert_eq!(pick(&none, "ghbdtn", EN, &[GER]), None);
        // ...and Latin<->Latin pairs never use the rules, only dictionaries
        let d = det(&[(Lang::EN, &["hello"]), (DE, &["hallo"])]);
        assert_eq!(pick(&d, "hello", EN, &[GER]), None);
    }

    #[test]
    fn non_latin_to_latin_without_rules_for_target() {
        // Russian layout -> a language we have no rules for: needs its dictionary
        let d = det(&[(Lang::RU, &["привет"]), (DE, &["hallo"])]);
        // "hallo" typed on the Russian layout is "руддщ"-like; here we only check the dictionary route exists
        assert_eq!(pick(&d, "привет", RU, &[EN, GER]), None);
    }

    #[test]
    fn uk_heuristics() {
        assert!(bad_uk("привыт")); // ы is not Ukrainian
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
