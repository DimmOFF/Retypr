// The release build on Windows has no console window (it is controlled from the tray).
// For debugging with logs use `build.cmd debug`.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
mod autostart;
mod config;
mod detector;
mod engine;
mod hotkey;
#[cfg(windows)]
mod icon;
mod lang;
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

    // `Retypr.exe --settings` opens the settings window (a separate process, no hooks)
    #[cfg(windows)]
    {
        if std::env::args().any(|a| a == "--settings") {
            return settings::run();
        }
    }

    // Single instance. A second launch silently exits; after "Restart as administrator" (flag --restarted)
    // we wait until the previous copy releases the lock.
    const NAME: &str = "Retypr-6f1c9d3a-layout-switcher";
    let make = || single_instance::SingleInstance::new(NAME).map_err(|e| anyhow::anyhow!("single-instance: {e:?}"));
    let mut slot = Some(make()?);
    if std::env::args().any(|a| a == "--restarted") {
        for _ in 0..40 {
            if slot.as_ref().is_some_and(|i| i.is_single()) {
                break;
            }
            drop(slot.take()); // close our own handle, otherwise the new one would see the lock as taken
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

    // the program folder may have been moved: keep "Start with Windows" pointing at this copy
    #[cfg(windows)]
    autostart::refresh_if_enabled();

    let (keys, warns) = hotkey::Bindings::from_config(&cfg.hotkeys);
    if !warns.is_empty() {
        os::warn(&format!("Problems in the [hotkeys] section of config.toml:\n\n{}", warns.join("\n")));
    }

    #[cfg(windows)]
    {
        tray::init(&cfg.hotkeys.toggle_auto)?;
        os::every(150, tray::tick);
    }

    let hk = &cfg.hotkeys;
    println!(
        "Retypr is running.  {} = word | {} = selection | {} = toggle auto | {} = toggle auto for app",
        hk.convert_word, hk.convert_selection, hk.toggle_auto, hk.toggle_app_auto
    );
    println!("Data folder: {:?}", config::Config::dir());

    let engine = Arc::new(Mutex::new(engine::Engine::new(cfg, keys)));
    watch_config(engine.clone());

    let hook_engine = engine.clone();
    let res = os::run(move |ev| hook_engine.lock().unwrap_or_else(|p| p.into_inner()).handle(&ev));

    // we are exiting (tray -> Exit, or restarting as administrator): close the settings window too
    #[cfg(windows)]
    {
        os::close_window(tray::SETTINGS_TITLE);
        tray::shutdown();
    }
    drop(inst);
    res
}

/// Watches config.toml: when the settings window (or you, by hand) saves it, the changes are applied without a
/// restart. The heavy part (dictionaries) runs here, outside the hook thread, so keystrokes are never delayed.
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
            std::thread::sleep(Duration::from_millis(250)); // let the writer finish
            last = mtime();
            match config::Config::load() {
                Ok(cfg) => {
                    let (keys, warns) = hotkey::Bindings::from_config(&cfg.hotkeys);
                    for w in &warns {
                        log::warn!("{w}");
                    }
                    let det = detector::Detector::new(&cfg);
                    state::init(&cfg);
                    state::mark_dirty(); // the tray refreshes in the main thread
                    engine.lock().unwrap_or_else(|p| p.into_inner()).reconfigure(&cfg, keys, det);
                    log::info!("config reloaded");
                }
                Err(e) => log::warn!("could not read config: {e:#}"),
            }
        }
    });
}
