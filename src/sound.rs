use crate::{config::{Config, SoundConfig}, state};
use rodio::{Decoder, OutputStream, Sink};
use std::{fs::File, io::BufReader, path::{Path, PathBuf}, sync::mpsc, thread};

#[derive(Clone, Copy)]
pub enum SoundEvent {
    Manual,
    Auto,
    Selection,
    Rule,
}

pub struct Sounds {
    cfg: SoundConfig,
    tx: mpsc::Sender<(PathBuf, f32)>,
}

impl Sounds {
    pub fn new(cfg: SoundConfig) -> Self {
        let (tx, rx) = mpsc::channel::<(PathBuf, f32)>();
        // OutputStream is not Send -> it lives in a dedicated audio thread
        thread::spawn(move || {
            let Ok((_stream, handle)) = OutputStream::try_default() else {
                log::warn!("no audio device");
                return;
            };
            for (path, vol) in rx {
                let Ok(f) = File::open(&path) else {
                    log::warn!("sound file not found: {}", path.display());
                    continue;
                };
                let Ok(src) = Decoder::new(BufReader::new(f)) else { continue };
                if let Ok(sink) = Sink::try_new(&handle) {
                    sink.set_volume(vol);
                    sink.append(src);
                    sink.detach();
                }
            }
        });
        Self { cfg, tx }
    }

    /// Play a file directly (the "Test" button in settings; the `enabled` flag is ignored).
    pub fn play_file(&self, path: &Path, volume: f32) {
        let _ = self.tx.send((path.to_path_buf(), volume));
    }

    pub fn play(&self, ev: SoundEvent) {
        if !state::sound() {
            return;
        }
        let p = match ev {
            SoundEvent::Manual => &self.cfg.on_manual_switch,
            SoundEvent::Auto => &self.cfg.on_auto_switch,
            SoundEvent::Selection => &self.cfg.on_selection_convert,
            SoundEvent::Rule => &self.cfg.on_rule_toggle,
        };
        if !p.trim().is_empty() {
            // relative paths are resolved against the (portable) data folder
            let _ = self.tx.send((Config::resolve(p.trim()), self.cfg.volume));
        }
    }
}
