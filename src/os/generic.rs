//! Linux/macOS: best-effort через rdev + enigo. Раскладок «две»: 0 = EN, 1 = RU
//! (системный список раскладок здесь пока не читается; Windows использует нативный бэкенд).
use super::{caret_plan, AfterConvert, KeyEvent, LayoutId, Step, K};
use crate::layout_map::{self, Lang};
use anyhow::{bail, Result};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use rdev::{EventType, Key as RKey};
use std::{thread::sleep, time::Duration};

fn name(k: RKey) -> &'static str {
    match k {
        RKey::Pause => "pause",
        RKey::ScrollLock => "scrolllock",
        RKey::PrintScreen => "printscreen",
        RKey::Insert => "insert",
        RKey::Delete => "delete",
        RKey::Home => "home",
        RKey::End => "end",
        RKey::PageUp => "pageup",
        RKey::PageDown => "pagedown",
        RKey::F1 => "f1", RKey::F2 => "f2", RKey::F3 => "f3", RKey::F4 => "f4",
        RKey::F5 => "f5", RKey::F6 => "f6", RKey::F7 => "f7", RKey::F8 => "f8",
        RKey::F9 => "f9", RKey::F10 => "f10", RKey::F11 => "f11", RKey::F12 => "f12",
        _ => "",
    }
}

fn map(k: RKey) -> K {
    match k {
        RKey::ShiftLeft | RKey::ShiftRight => K::Shift,
        RKey::ControlLeft | RKey::ControlRight => K::Ctrl,
        RKey::Alt | RKey::AltGr => K::Alt,
        RKey::MetaLeft | RKey::MetaRight => K::Meta,
        RKey::Backspace => K::Backspace,
        RKey::Space => K::Space,
        RKey::Return | RKey::KpReturn | RKey::Tab | RKey::Escape | RKey::Delete
        | RKey::LeftArrow | RKey::RightArrow | RKey::UpArrow | RKey::DownArrow
        | RKey::Home | RKey::End | RKey::PageUp | RKey::PageDown => K::Reset,
        _ => K::Char,
    }
}

pub fn run<F>(handler: F) -> Result<()>
where
    F: Fn(KeyEvent) -> bool + Send + Sync + 'static,
{
    rdev::grab(move |ev| {
        let (down, key, nm) = match &ev.event_type {
            EventType::KeyPress(k) => (true, map(*k), name(*k)),
            EventType::KeyRelease(k) => (false, map(*k), name(*k)),
            EventType::ButtonPress(_) => (true, K::Reset, ""),
            _ => return Some(ev),
        };
        let text = ev.name.clone().unwrap_or_default();
        if handler(KeyEvent { down, key, text, name: nm }) { None } else { Some(ev) }
    })
    .map_err(|e| anyhow::anyhow!("could not install the keyboard hook: {e:?}"))
}

pub fn current_layout() -> LayoutId { 0 }
pub fn next_layout(id: LayoutId) -> LayoutId { if id == 0 { 1 } else { 0 } }
pub fn layouts() -> Vec<LayoutId> { vec![0, 1] }
pub fn layout_lang(id: LayoutId) -> Option<Lang> {
    match id { 0 => Some(Lang::En), 1 => Some(Lang::Ru), _ => None }
}
pub fn convert(text: &str, from: LayoutId, to: LayoutId) -> String {
    match (layout_lang(from), layout_lang(to)) {
        (Some(f), Some(t)) => layout_map::convert_between(text, f, t),
        _ => text.to_string(),
    }
}

pub fn switch_to(_id: LayoutId) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        // X11 + xkb-switch (переключает на следующую раскладку)
        let st = std::process::Command::new("xkb-switch").arg("-n").status()?;
        if !st.success() {
            bail!("xkb-switch failed");
        }
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        bail!("layout switching is not implemented on this OS")
    }
}

pub fn replace_typed(erase: usize, text: &str, to: LayoutId) -> Result<()> {
    let mut e = Enigo::new(&Settings::default())?;
    for _ in 0..erase {
        e.key(Key::Backspace, Direction::Click)?;
    }
    e.text(text)?;
    switch_to(to)
}

pub fn convert_selection<F>(after: AfterConvert, f: F) -> Result<()>
where
    F: FnOnce(&str, LayoutId) -> Option<(String, LayoutId)>,
{
    let mut cb = arboard::Clipboard::new()?;
    let saved = cb.get_text().ok();
    cb.set_text("")?;
    let modk = if cfg!(target_os = "macos") { Key::Meta } else { Key::Control };
    let mut e = Enigo::new(&Settings::default())?;

    e.key(modk, Direction::Press)?;
    e.key(Key::Unicode('c'), Direction::Click)?;
    e.key(modk, Direction::Release)?;
    sleep(Duration::from_millis(120));

    let sel = cb.get_text().unwrap_or_default();
    if !sel.is_empty() {
        if let Some((conv, target)) = f(&sel, current_layout()) {
            cb.set_text(conv.clone())?;
            e.key(modk, Direction::Press)?;
            e.key(Key::Unicode('v'), Direction::Click)?;
            e.key(modk, Direction::Release)?;
            sleep(Duration::from_millis(150));
            let _ = switch_to(target);
            for step in caret_plan(after, conv.replace("\r\n", "\n").chars().count()) {
                match step {
                    Step::ShiftDown => e.key(Key::Shift, Direction::Press)?,
                    Step::ShiftUp => e.key(Key::Shift, Direction::Release)?,
                    Step::Left => e.key(Key::LeftArrow, Direction::Click)?,
                    Step::Right => e.key(Key::RightArrow, Direction::Click)?,
                }
            }
        }
    }
    if let Some(s) = saved {
        cb.set_text(s)?;
    }
    Ok(())
}

pub fn quit() {
    std::process::exit(0);
}

pub fn fatal(msg: &str) {
    eprintln!("Retypr: {msg}");
}

/// Имя exe активного окна (на Linux/macOS пока не определяется).
pub fn foreground_app() -> String {
    String::new()
}

pub fn mask_menu() {}

pub fn warn(msg: &str) {
    eprintln!("Retypr: {msg}");
}
