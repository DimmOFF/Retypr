// В release под Windows консоль не показываем (управление — через трей).
// Для отладки с логами используйте `build.cmd debug`.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
mod autostart;
mod config;
mod detector;
mod engine;
mod hotkey;
#[cfg(windows)]
mod icon;
mod layout_map;
mod os;
#[cfg(windows)]
mod settings;
mod sound;
mod state;
#[cfg(windows)]
mod tray;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

fn main() {
    if let Err(e) = run() {
        let msg = format!("{e:#}");
        eprintln!("Retypr: {msg}");
        os::fatal(&msg);
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    env_logger::init();

    // `retypr.exe --settings` — окно настроек (отдельный процесс, без хуков)
    #[cfg(windows)]
    {
        if std::env::args().any(|a| a == "--settings") {
            return settings::run();
        }
    }

    // Один экземпляр. Второй запуск молча завершается; но после «Перезапустить от администратора»
    // (флаг --restarted) ждём, пока прежняя копия освободит мьютекс.
    const NAME: &str = "Retypr-6f1c9d3a-layout-switcher";
    let make = || single_instance::SingleInstance::new(NAME).map_err(|e| anyhow::anyhow!("single-instance: {e:?}"));
    let mut slot = Some(make()?);
    if std::env::args().any(|a| a == "--restarted") {
        for _ in 0..40 {
            if slot.as_ref().is_some_and(|i| i.is_single()) {
                break;
            }
            drop(slot.take()); // закрыть свой дескриптор, иначе новый «увидит» мьютекс занятым
            std::thread::sleep(Duration::from_millis(150));
            slot = Some(make()?);
        }
    }
    let inst = slot.expect("instance");
    if !inst.is_single() {
        return Ok(());
    }

    let cfg = config::Config::load_or_create()?;
    state::init(&cfg);

    let (keys, warns) = hotkey::Bindings::from_config(&cfg.hotkeys);
    if !warns.is_empty() {
        os::warn(&format!("Problems in the [hotkeys] section of config.toml:\n\n{}", warns.join("\n")));
    }

    #[cfg(windows)]
    {
        tray::init(&cfg.hotkeys.toggle_auto)?;
        os::every(300, tray::tick);
    }

    let hk = &cfg.hotkeys;
    println!(
        "Retypr is running.  {} = word | {} = selection | {} = toggle auto | {} = toggle auto for app",
        hk.convert_word, hk.convert_selection, hk.toggle_auto, hk.toggle_app_auto
    );
    println!("Config: {:?}", config::Config::dir());

    let engine = Arc::new(Mutex::new(engine::Engine::new(cfg, keys)));
    watch_config(engine.clone());

    let hook_engine = engine.clone();
    let res = os::run(move |ev| hook_engine.lock().unwrap_or_else(|p| p.into_inner()).handle(&ev));

    #[cfg(windows)]
    tray::shutdown();
    drop(inst);
    res
}

/// Следит за config.toml: когда окно настроек (или вы вручную) сохраняет файл, применяет изменения без перезапуска.
/// Тяжёлая часть (словари) выполняется здесь, вне потока хука, чтобы не задерживать клавиши.
fn watch_config(engine: Arc<Mutex<engine::Engine>>) {
    let Some(path) = config::Config::path() else { return };
    std::thread::spawn(move || {
        let mtime = || std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let mut last = mtime();
        loop {
            std::thread::sleep(Duration::from_secs(1));
            if mtime() == last {
                continue;
            }
            std::thread::sleep(Duration::from_millis(250)); // дать дописать файл
            last = mtime();
            match config::Config::load() {
                Ok(cfg) => {
                    let (keys, warns) = hotkey::Bindings::from_config(&cfg.hotkeys);
                    for w in &warns {
                        log::warn!("{w}");
                    }
                    let det = detector::Detector::new(&cfg);
                    state::init(&cfg);
                    state::mark_dirty(); // трей обновится в главном потоке
                    engine.lock().unwrap_or_else(|p| p.into_inner()).reconfigure(&cfg, keys, det);
                    log::info!("config reloaded");
                }
                Err(e) => log::warn!("could not read config: {e:#}"),
            }
        }
    });
}
