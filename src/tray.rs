//! Tray icon (Windows) / menu bar icon (macOS) and its menu. Everything lives in the main thread, so the objects are
//! kept in a thread_local instead of Send structures.
//!
//! IMPORTANT: menu item objects are never touched inside the menu's own click handler (re-entering the
//! menu library from its callback crashes it). The handler only changes `state` and raises a dirty flag;
//! `tick()`, called by a timer from the main loop, then refreshes the check marks, icon and tooltip.
use crate::{
    autostart,
    config::Config,
    icon::{self, IconKind},
    os,
    state::{self, AppMode},
};
use anyhow::Result;
use std::{
    cell::RefCell,
    panic::{catch_unwind, AssertUnwindSafe},
};
use tray_icon::{
    menu::{CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

/// Title of the settings window (also used to find, focus and close it on Windows).
pub const SETTINGS_TITLE: &str = "Retypr Settings";

#[cfg(windows)]
const START_LABEL: &str = "Start with Windows";
#[cfg(target_os = "macos")]
const START_LABEL: &str = "Start at login";

struct Tray {
    tray: TrayIcon,
    auto: CheckMenuItem,
    sound: CheckMenuItem,
    start: CheckMenuItem,
    kind: IconKind,
}

thread_local! {
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

struct Ids {
    settings: MenuId,
    auto: MenuId,
    sound: MenuId,
    start: MenuId,
    folder: MenuId,
    admin: Option<MenuId>,
    quit: MenuId,
}

/// Which icon to show:
///  blue  - auto-switching on
///  gray  - auto-switching off, or Retypr disabled for the focused app
///  split - auto-switching on, but paused for the focused app (blue upper-left, gray lower-right)
fn current_kind() -> IconKind {
    match state::app_mode() {
        _ if !state::auto() => IconKind::Gray,
        AppMode::Off => IconKind::Gray,
        AppMode::NoAuto => IconKind::Split,
        AppMode::Normal => IconKind::Blue,
    }
}

fn make_icon(kind: IconKind) -> Icon {
    let base = icon::rgba(kind);
    let s = icon::SIZE;
    if cfg!(target_os = "macos") {
        // double the size for the Retina menu bar (nearest neighbour keeps the crisp pixel look)
        let n = s * 2;
        let mut out = vec![0u8; n * n * 4];
        for y in 0..n {
            for x in 0..n {
                let from = ((y / 2) * s + x / 2) * 4;
                let to = (y * n + x) * 4;
                out[to..to + 4].copy_from_slice(&base[from..from + 4]);
            }
        }
        Icon::from_rgba(out, n as u32, n as u32).expect("icon")
    } else {
        Icon::from_rgba(base, s as u32, s as u32).expect("icon")
    }
}

fn tooltip() -> String {
    let mut s = format!("Retypr — auto-switching: {}", if state::auto() { "on" } else { "off" });
    match state::app_mode() {
        AppMode::NoAuto if state::auto() => s.push_str(" (paused in this app)"),
        AppMode::Off => s.push_str(" (disabled in this app)"),
        _ => {}
    }
    #[cfg(windows)]
    if os::is_elevated() {
        s.push_str(" (administrator)");
    }
    s
}

/// Bring the menu check marks, the icon and the tooltip in line with `state`. Main thread only; never call it
/// from inside a menu event handler.
fn sync() {
    TRAY.with(|t| {
        if let Some(t) = t.borrow_mut().as_mut() {
            t.auto.set_checked(state::auto());
            t.sound.set_checked(state::sound());
            t.start.set_checked(autostart::is_enabled());
            let kind = current_kind();
            if kind != t.kind && t.tray.set_icon(Some(make_icon(kind))).is_ok() {
                t.kind = kind;
            }
            let _ = t.tray.set_tooltip(Some(tooltip()));
        }
    });
}

/// Called by a timer in the main thread: refresh the tray if something changed.
pub fn tick() {
    if state::take_dirty() {
        sync();
    }
}

// ───────────────────────── the settings window ─────────────────────────

#[cfg(windows)]
fn open_settings() {
    // already open? just bring it to the front
    if os::focus_window(SETTINGS_TITLE) {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).arg("--settings").spawn();
    }
}

#[cfg(windows)]
pub fn close_settings() {
    os::close_window(SETTINGS_TITLE);
}

/// macOS: the settings window is a child process. We cannot find its window by title without extra permissions,
/// so we talk to it through small request files in the data folder, which it polls (see settings.rs).
#[cfg(target_os = "macos")]
static SETTINGS_CHILD: std::sync::Mutex<Option<std::process::Child>> = std::sync::Mutex::new(None);

#[cfg(target_os = "macos")]
fn request_file(name: &str) {
    if let Some(dir) = Config::dir() {
        let _ = std::fs::write(dir.join(name), b"");
    }
}

#[cfg(target_os = "macos")]
fn open_settings() {
    let mut guard = SETTINGS_CHILD.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(child) = guard.as_mut() {
        if matches!(child.try_wait(), Ok(None)) {
            request_file(".settings-raise"); // already open: ask it to come to the front
            return;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(child) = std::process::Command::new(exe).arg("--settings").spawn() {
            *guard = Some(child);
        }
    }
}

#[cfg(target_os = "macos")]
pub fn close_settings() {
    if let Some(mut child) = SETTINGS_CHILD.lock().unwrap_or_else(|p| p.into_inner()).take() {
        let _ = child.kill();
    }
}

fn open_folder(dir: std::path::PathBuf) {
    let opener = if cfg!(target_os = "macos") { "open" } else { "explorer" };
    let _ = std::process::Command::new(opener).arg(dir).spawn();
}

// ───────────────────────── menu ─────────────────────────

/// Menu click handler. It must not touch the menu items (see the module docs).
fn on_menu(ids: &Ids, e: MenuEvent) {
    let id = &e.id;
    if *id == ids.auto {
        state::set_auto(!state::auto());
    } else if *id == ids.sound {
        state::set_sound(!state::sound());
    } else if *id == ids.start {
        if let Err(err) = autostart::set(!autostart::is_enabled()) {
            log::error!("autostart: {err}");
        }
        state::mark_dirty();
    } else if *id == ids.settings {
        open_settings();
    } else if *id == ids.folder {
        if let Some(dir) = Config::dir() {
            open_folder(dir);
        }
    } else if Some(id) == ids.admin.as_ref() {
        #[cfg(windows)]
        match os::restart_elevated() {
            Ok(()) => os::quit(), // the new copy waits until we release the single-instance lock
            Err(err) => log::warn!("{err:#}"),
        }
    } else if *id == ids.quit {
        os::quit();
    }
}

pub fn init(toggle_hint: &str) -> Result<()> {
    let settings = MenuItem::new("Settings…", true, None);
    let auto = CheckMenuItem::new(format!("Auto-switching ({toggle_hint})"), true, state::auto(), None);
    let sound = CheckMenuItem::new("Sounds", true, state::sound(), None);
    let start = CheckMenuItem::new(START_LABEL, true, autostart::is_enabled(), None);
    let folder = MenuItem::new("Open settings folder", true, None);
    #[cfg(windows)]
    let admin = {
        let elevated = os::is_elevated();
        MenuItem::new(
            if elevated { "Running as administrator ✓" } else { "Restart as administrator" },
            !elevated,
            None,
        )
    };
    let quit = MenuItem::new(if cfg!(target_os = "macos") { "Quit Retypr" } else { "Exit" }, true, None);
    let (sep1, sep2) = (PredefinedMenuItem::separator(), PredefinedMenuItem::separator());

    let menu = Menu::new();
    let mut items: Vec<&dyn IsMenuItem> = vec![&settings, &auto, &sound, &start, &sep1, &folder];
    #[cfg(windows)]
    items.push(&admin);
    items.push(&sep2);
    items.push(&quit);
    menu.append_items(&items)?;

    let ids = Ids {
        settings: settings.id().clone(),
        auto: auto.id().clone(),
        sound: sound.id().clone(),
        start: start.id().clone(),
        folder: folder.id().clone(),
        #[cfg(windows)]
        admin: Some(admin.id().clone()),
        #[cfg(not(windows))]
        admin: None,
        quit: quit.id().clone(),
    };

    let kind = current_kind();
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(tooltip())
        .with_icon(make_icon(kind))
        .build()?;

    TRAY.with(|t| *t.borrow_mut() = Some(Tray { tray, auto, sound, start, kind }));

    // Invoked from the main thread. A panic must never unwind into the OS.
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let _ = catch_unwind(AssertUnwindSafe(|| on_menu(&ids, e)));
    }));
    Ok(())
}

/// Remove the icon before exiting so it does not linger in the tray / menu bar.
pub fn shutdown() {
    TRAY.with(|t| {
        t.borrow_mut().take();
    });
}
