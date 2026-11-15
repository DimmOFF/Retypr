//! Тонкий слой над ОС: хук клавиш, ввод текста, смена раскладки, конвертация выделения.

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
