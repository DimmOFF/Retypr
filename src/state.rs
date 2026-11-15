//! Глобальное состояние, которое меняют и движок (Alt+Pause), и меню трея.
//! Всё меняется из главного потока, колбэк `on_change` обновляет трей.
use crate::config::Config;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    OnceLock,
};

static AUTO: AtomicBool = AtomicBool::new(true);
static SOUND: AtomicBool = AtomicBool::new(true);
static ON_CHANGE: OnceLock<fn()> = OnceLock::new();
/// Состояние изменено из другого потока (перезагрузка конфига) — главному потоку пора обновить трей.
static DIRTY: AtomicBool = AtomicBool::new(false);

pub fn mark_dirty() {
    DIRTY.store(true, Ordering::SeqCst);
}
pub fn take_dirty() -> bool {
    DIRTY.swap(false, Ordering::SeqCst)
}

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

pub fn set_auto(v: bool) {
    AUTO.store(v, Ordering::SeqCst);
    changed();
}
pub fn set_sound(v: bool) {
    SOUND.store(v, Ordering::SeqCst);
    changed();
}

/// Зарегистрировать обновление UI (вызывается из того потока, где создан трей).
#[allow(dead_code)]
pub fn on_change(f: fn()) {
    let _ = ON_CHANGE.set(f);
}

fn changed() {
    if let Some(f) = ON_CHANGE.get() {
        f();
    }
    // Сохраняем выбор в config.toml, но не блокируем хук диском
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
