use crate::{config::{Config, SoundConfig}, state};
use rodio::{
    cpal::{
        self,
        traits::{DeviceTrait, HostTrait},
    },
    Decoder, OutputStream, OutputStreamHandle, Sink,
};
use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub enum SoundEvent {
    Manual,
    Auto,
    Selection,
    Rule,
}

/// Open the audio output again when it has been idle this long. A stream that sat unused through sleep/resume,
/// a Bluetooth headset powering down or an audio-service restart is often dead without reporting any error.
const IDLE_REOPEN: Duration = Duration::from_secs(30);

/// Should the audio output be (re)opened before playing the next sound?
fn needs_reopen(has_stream: bool, default_device_changed: bool, idle: Duration) -> bool {
    !has_stream || default_device_changed || idle > IDLE_REOPEN
}

struct Audio {
    _stream: OutputStream, // must stay alive while sounds play
    handle: OutputStreamHandle,
    device: String,
    last_used: Instant,
}

fn default_device_name() -> String {
    cpal::default_host().default_output_device().and_then(|d| d.name().ok()).unwrap_or_default()
}

fn open_audio() -> Option<Audio> {
    let device = cpal::default_host().default_output_device()?;
    let name = device.name().unwrap_or_default();
    match OutputStream::try_from_device(&device) {
        Ok((stream, handle)) => Some(Audio { _stream: stream, handle, device: name, last_used: Instant::now() }),
        Err(e) => {
            log::warn!("cannot open the audio output: {e}");
            None
        }
    }
}

enum PlayError {
    /// the file is missing or cannot be decoded - reopening the audio output will not help
    File(String),
    /// the audio output itself failed
    Audio(String),
}

fn play_now(a: &Audio, path: &Path, volume: f32) -> Result<(), PlayError> {
    let file = File::open(path).map_err(|e| PlayError::File(format!("sound file {}: {e}", path.display())))?;
    let source = Decoder::new(BufReader::new(file))
        .map_err(|e| PlayError::File(format!("sound file {}: {e}", path.display())))?;
    let sink = Sink::try_new(&a.handle).map_err(|e| PlayError::Audio(e.to_string()))?;
    sink.set_volume(volume);
    sink.append(source);
    sink.detach();
    Ok(())
}

pub struct Sounds {
    cfg: SoundConfig,
    tx: mpsc::Sender<(PathBuf, f32)>,
}

impl Sounds {
    pub fn new(cfg: SoundConfig) -> Self {
        let (tx, rx) = mpsc::channel::<(PathBuf, f32)>();
        // The output stream is not Send, so it lives in a dedicated audio thread. The thread never gives up: the
        // device is opened lazily, re-checked before every sound, and reopened whenever it may have gone stale.
        thread::spawn(move || {
            let mut audio: Option<Audio> = None;
            for (path, volume) in rx {
                let device_changed = audio.as_ref().map(|a| a.device != default_device_name()).unwrap_or(false);
                let idle = audio.as_ref().map(|a| a.last_used.elapsed()).unwrap_or_default();
                if needs_reopen(audio.is_some(), device_changed, idle) {
                    drop(audio.take()); // release the old device before opening the new one
                    audio = open_audio();
                }
                let Some(a) = audio.as_mut() else {
                    continue; // no audio device right now; the next sound tries again
                };
                match play_now(a, &path, volume) {
                    Ok(()) => a.last_used = Instant::now(),
                    Err(PlayError::File(msg)) => log::warn!("{msg}"),
                    Err(PlayError::Audio(msg)) => {
                        // one retry on a fresh output
                        log::warn!("audio output failed ({msg}), reopening it");
                        drop(audio.take());
                        audio = open_audio();
                        if let Some(a) = audio.as_mut() {
                            match play_now(a, &path, volume) {
                                Ok(()) => a.last_used = Instant::now(),
                                Err(PlayError::File(m)) | Err(PlayError::Audio(m)) => log::warn!("{m}"),
                            }
                        }
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_output_is_reopened_when_needed() {
        let s = Duration::from_secs;
        assert!(needs_reopen(false, false, s(0))); // never opened (or the first attempt failed)
        assert!(needs_reopen(true, true, s(1))); // the default device changed
        assert!(needs_reopen(true, false, s(31))); // idle for a while: may be stale after sleep/resume
        assert!(!needs_reopen(true, false, s(5))); // in active use: keep the stream
        assert!(!needs_reopen(true, false, IDLE_REOPEN)); // exactly at the limit is still fine
    }
}
