//! Тонкий слой над ОС: хук клавиш, ввод текста, смена раскладки, конвертация выделения.

/// What to do with the text and the caret right after a selection has been converted and pasted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AfterConvert {
    /// keep the converted text selected (lets you press the hotkey again to cycle layouts)
    pub keep_selection: bool,
    /// caret at the left end of the converted text (otherwise at the right end)
    pub caret_left: bool,
}

impl AfterConvert {
    pub fn from_config(c: &crate::config::Config) -> Self {
        Self {
            keep_selection: !c.deselect_after_convert,
            caret_left: c.caret_after_convert == crate::config::CaretSide::Left,
        }
    }
}

/// One keyboard step of the post-paste caret/selection fix-up (platform-neutral).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    ShiftDown,
    ShiftUp,
    Left,
    Right,
}

/// Keys to press after the converted text (`n` characters) was pasted; the caret is then at its right end.
pub fn caret_plan(a: AfterConvert, n: usize) -> Vec<Step> {
    let rep = |s: Step| std::iter::repeat(s).take(n);
    let mut v = Vec::new();
    if n == 0 {
        return v;
    }
    match (a.keep_selection, a.caret_left) {
        // text stays unselected, caret already at the right end
        (false, false) => {}
        // move the caret to the left end
        (false, true) => v.extend(rep(Step::Left)),
        // select backwards: the caret (active end) is at the left
        (true, true) => {
            v.push(Step::ShiftDown);
            v.extend(rep(Step::Left));
            v.push(Step::ShiftUp);
        }
        // go to the start, then select forwards: the caret (active end) is at the right
        (true, false) => {
            v.extend(rep(Step::Left));
            v.push(Step::ShiftDown);
            v.extend(rep(Step::Right));
            v.push(Step::ShiftUp);
        }
    }
    v
}

/// Непрозрачный идентификатор раскладки (на Windows — HKL).
pub type LayoutId = isize;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum K {
    Shift,
    Ctrl,
    Alt,
    Meta,
    Backspace,
    Space,
    /// Enter / Tab / стрелки / Esc / Delete и т.п. — слово закончилось, буфер сбросить
    Reset,
    /// Любая другая клавиша; символ (если есть) лежит в `text`
    Char,
}

#[derive(Clone, Debug)]
pub struct KeyEvent {
    pub down: bool,
    pub key: K,
    pub text: String,
    /// Платформенно-нейтральное имя клавиши для горячих клавиш ("pause", "f12", "a"…); "" — неизвестна/синтетическое
    pub name: &'static str,
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(not(windows))]
mod generic;
#[cfg(not(windows))]
pub use self::generic::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(keep: bool, left: bool, n: usize) -> Vec<Step> {
        caret_plan(AfterConvert { keep_selection: keep, caret_left: left }, n)
    }

    #[test]
    fn nothing_to_do() {
        assert!(plan(false, false, 5).is_empty()); // deselected, caret at the right: nothing to press
        assert!(plan(true, true, 0).is_empty());
    }

    #[test]
    fn deselect_caret_left() {
        assert_eq!(plan(false, true, 3), vec![Step::Left; 3]);
    }

    #[test]
    fn keep_selection_caret_left() {
        use Step::*;
        assert_eq!(plan(true, true, 2), vec![ShiftDown, Left, Left, ShiftUp]);
    }

    #[test]
    fn keep_selection_caret_right() {
        use Step::*;
        assert_eq!(plan(true, false, 2), vec![Left, Left, ShiftDown, Right, Right, ShiftUp]);
    }
}
