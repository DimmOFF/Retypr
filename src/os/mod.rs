//! Thin OS layer: key hook, text input, layout switching, selection conversion.

/// What to do with the text and the caret right after a selection has been converted and pasted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AfterConvert {
    /// keep the converted text selected (lets you press the hotkey again to cycle layouts)
    pub keep_selection: bool,
    /// caret at the left end of the converted text (otherwise at the right end)
    pub caret_left: bool,
    /// Restoring the selection / caret costs one keystroke per character. Texts longer than this are skipped
    /// (the caret then simply stays at the right end). 0 = no limit.
    pub max_chars: usize,
}

impl AfterConvert {
    pub fn from_config(c: &crate::config::Config) -> Self {
        Self {
            keep_selection: c.select_after_convert,
            caret_left: c.caret_after_convert == crate::config::CaretSide::Left,
            max_chars: c.selection_restore_limit,
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
    if a.max_chars > 0 && n > a.max_chars {
        return v; // too long to fix up key by key
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

/// Opaque layout identifier (on Windows: the HKL).
pub type LayoutId = isize;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum K {
    Shift,
    Ctrl,
    Alt,
    Meta,
    Backspace,
    Space,
    /// Enter / Tab / arrows / Esc / Delete etc.: the word is over, reset the buffer
    Reset,
    /// Any other key; its character (if any) is in `text`
    Char,
}

#[derive(Clone, Debug)]
pub struct KeyEvent {
    pub down: bool,
    pub key: K,
    pub text: String,
    /// Platform-neutral key name for hotkeys ("pause", "f12", "a"...); "" = unknown or a synthetic event
    pub name: &'static str,
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use self::macos::*;

// Linux and everything else: best-effort backend
#[cfg(all(not(windows), not(target_os = "macos")))]
mod generic;
#[cfg(all(not(windows), not(target_os = "macos")))]
pub use self::generic::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(keep: bool, left: bool, n: usize) -> Vec<Step> {
        caret_plan(AfterConvert { keep_selection: keep, caret_left: left, max_chars: 0 }, n)
    }

    #[test]
    fn nothing_to_do() {
        assert!(plan(false, false, 5).is_empty()); // not selected, caret at the right: nothing to press
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
    fn long_texts_skip_the_fixup() {
        let a = |max_chars| AfterConvert { keep_selection: true, caret_left: true, max_chars };
        assert_eq!(caret_plan(a(10), 10).len(), 12); // at the limit: ShiftDown + 10 x Left + ShiftUp
        assert!(caret_plan(a(10), 11).is_empty()); // above it: nothing is pressed
        assert_eq!(caret_plan(a(0), 100_000).len(), 100_002); // 0 = no limit
    }

    #[test]
    fn keep_selection_caret_right() {
        use Step::*;
        assert_eq!(plan(true, false, 2), vec![Left, Left, ShiftDown, Right, Right, ShiftUp]);
    }
}
