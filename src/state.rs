//! Global state shared by the engine (hotkeys), the tray menu and the config watcher.
//!
//! Nothing here touches UI objects. Anything that needs the tray to refresh just raises the `DIRTY` flag;
//! the main thread notices it on its next timer tick (see tray::tick). That keeps tray menu items from being
//! modified inside their own click handlers, which is not safe.
use crate::config::Config;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// How Retypr behaves in the currently focused application.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppMode {
    Normal,
    /// auto-switching off, manual hotkeys work
    NoAuto,
    /// Retypr is completely disabled
    Off,
}

static AUTO: AtomicBool = AtomicBool::new(true);
static SOUND: AtomicBool = AtomicBool::new(true);
static APP_MODE: AtomicU8 = AtomicU8::new(0);
static DIRTY: AtomicBool = AtomicBool::new(false);

pub fn init(cfg: &Config) {
    AUTO.store(cfg.auto_switch, Ordering::SeqCst);
    SOUND.store(cfg.sounds.enabled, Ordering::SeqCst);
}

pub fn auto() -> bool {
    AUTO.load(Ordering::SeqCst)
}
pub fn sound() -> bool {
    SOUND.load(Ordering::SeqCst)
}

pub fn app_mode() -> AppMode {
    match APP_MODE.load(Ordering::SeqCst) {
        1 => AppMode::NoAuto,
        2 => AppMode::Off,
        _ => AppMode::Normal,
    }
}

/// Called by the engine whenever the focused application changes.
pub fn set_app_mode(m: AppMode) {
    let v = match m {
        AppMode::Normal => 0,
        AppMode::NoAuto => 1,
        AppMode::Off => 2,
    };
    if APP_MODE.swap(v, Ordering::SeqCst) != v {
        mark_dirty();
    }
}

pub fn set_auto(v: bool) {
    AUTO.store(v, Ordering::SeqCst);
    changed();
}
pub fn set_sound(v: bool) {
    SOUND.store(v, Ordering::SeqCst);
    changed();
}

/// Something changed: the tray must refresh (from any thread).
pub fn mark_dirty() {
    DIRTY.store(true, Ordering::SeqCst);
}
pub fn take_dirty() -> bool {
    DIRTY.swap(false, Ordering::SeqCst)
}

fn changed() {
    mark_dirty();
    // persist the choice in config.toml, but never block the keyboard hook on disk I/O
    std::thread::spawn(|| {
        if let Ok(mut c) = Config::load_or_create() {
            c.auto_switch = auto();
            c.sounds.enabled = sound();
            if let Err(e) = c.save() {
                log::warn!("could not save config: {e:#}");
            }
        }
    });
}
