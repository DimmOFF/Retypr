//! Иконка в трее и меню. Всё живёт в главном потоке (там же крутится message loop хуков),
//! поэтому состояние лежит в thread_local, а не в Send-структурах.
use crate::{autostart, config::Config, icon, os, state};
use anyhow::Result;
use std::cell::RefCell;
use tray_icon::{
    menu::{CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

struct Tray {
    tray: TrayIcon,
    auto: CheckMenuItem,
    sound: CheckMenuItem,
    start: CheckMenuItem,
}

thread_local! {
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

fn make_icon(on: bool) -> Icon {
    Icon::from_rgba(icon::rgba(on), icon::SIZE as u32, icon::SIZE as u32).expect("icon")
}

fn tooltip() -> String {
    format!(
        "Retypr — auto: {}{}",
        if state::auto() { "on" } else { "off" },
        if os::is_elevated() { " (administrator)" } else { "" }
    )
}

/// Привести меню и значок в соответствие с `state` (вызывается и из хука, в главном потоке).
pub fn sync() {
    TRAY.with(|t| {
        if let Some(t) = t.borrow().as_ref() {
            t.auto.set_checked(state::auto());
            t.sound.set_checked(state::sound());
            let _ = t.tray.set_icon(Some(make_icon(state::auto())));
            let _ = t.tray.set_tooltip(Some(tooltip()));
        }
    });
}

pub fn init(toggle_hint: &str) -> Result<()> {
    let auto = CheckMenuItem::new(format!("Auto-switching ({toggle_hint})"), true, state::auto(), None);
    let sound = CheckMenuItem::new("Sounds", true, state::sound(), None);
    let start = CheckMenuItem::new("Start with Windows", true, autostart::is_enabled(), None);
    let folder = MenuItem::new("Open settings folder", true, None);
    let elevated = os::is_elevated();
    let admin = MenuItem::new(
        if elevated { "Running as administrator ✓" } else { "Restart as administrator" },
        !elevated,
        None,
    );
    let settings = MenuItem::new("Settings…", true, None);
    let quit = MenuItem::new("Exit", true, None);
    let (sep1, sep2) = (PredefinedMenuItem::separator(), PredefinedMenuItem::separator());

    let menu = Menu::new();
    let items: [&dyn IsMenuItem; 9] = [&settings, &auto, &sound, &start, &sep1, &folder, &admin, &sep2, &quit];
    menu.append_items(&items)?;

    let (id_auto, id_sound, id_start) = (auto.id().clone(), sound.id().clone(), start.id().clone());
    let (id_folder, id_quit, id_admin) = (folder.id().clone(), quit.id().clone(), admin.id().clone());
    let id_settings = settings.id().clone();

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(tooltip())
        .with_icon(make_icon(state::auto()))
        .build()?;

    TRAY.with(|t| *t.borrow_mut() = Some(Tray { tray, auto, sound, start }));
    state::on_change(sync);

    // Вызывается из window proc в главном потоке (внутри нашего message loop)
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        if e.id == id_auto {
            let v = TRAY.with(|t| t.borrow().as_ref().map(|t| t.auto.is_checked()));
            if let Some(v) = v { state::set_auto(v); }
        } else if e.id == id_sound {
            let v = TRAY.with(|t| t.borrow().as_ref().map(|t| t.sound.is_checked()));
            if let Some(v) = v { state::set_sound(v); }
        } else if e.id == id_start {
            TRAY.with(|t| {
                if let Some(t) = t.borrow().as_ref() {
                    if let Err(err) = autostart::set(t.start.is_checked()) {
                        log::error!("autostart: {err}");
                        t.start.set_checked(autostart::is_enabled()); // откатить галочку
                    }
                }
            });
        } else if e.id == id_folder {
            if let Some(dir) = Config::dir() {
                let _ = std::process::Command::new("explorer").arg(dir).spawn();
            }
        } else if e.id == id_settings {
            if let Ok(exe) = std::env::current_exe() {
                let _ = std::process::Command::new(exe).arg("--settings").spawn();
            }
        } else if e.id == id_admin {
            match os::restart_elevated() {
                Ok(()) => os::quit(), // новая копия дождётся, пока мы освободим единственный экземпляр
                Err(err) => log::warn!("{err:#}"),
            }
        } else if e.id == id_quit {
            os::quit();
        }
    }));
    Ok(())
}

/// Вызывается таймером в главном потоке: если конфиг перечитан в другом потоке — обновить меню и значок.
pub fn tick() {
    if state::take_dirty() {
        sync();
    }
}

/// Убрать значок до выхода, чтобы он не «висел» в трее.
pub fn shutdown() {
    TRAY.with(|t| {
        t.borrow_mut().take();
    });
}
