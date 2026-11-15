use crate::{config::SoundConfig, state};
use rodio::{Decoder, OutputStream, Sink};
use std::{fs::File, io::BufReader, path::PathBuf, sync::mpsc, thread};

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
        // OutputStream не Send -> живёт в отдельном аудио-потоке
        thread::spawn(move || {
            let Ok((_stream, handle)) = OutputStream::try_default() else {
                log::warn!("no audio device");
                return;
            };
            for (path, vol) in rx {
                let Ok(f) = File::open(&path) else { continue };
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

    /// Проиграть файл напрямую (кнопка «Тест» в настройках; флаг enabled не учитывается).
    pub fn play_file(&self, path: &std::path::Path, volume: f32) {
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
        if let Some(p) = p {
            let _ = self.tx.send((p.clone(), self.cfg.volume));
        }
    }
}
