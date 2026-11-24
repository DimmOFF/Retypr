use crate::{
    config::Config,
    detector::Detector,
    hotkey::{Action, Bindings},
    lang::Lang,
    os::{self, AfterConvert, KeyEvent, LayoutId, K},
    sound::{SoundEvent, Sounds},
    state::{self, AppMode},
};
use std::{
    collections::HashSet,
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex},
    thread,
    time::Duration,
};

struct Rules {
    disabled: HashSet<String>,
    no_auto: HashSet<String>,
    no_switch: HashSet<String>,
}

impl Rules {
    fn from_config(cfg: &Config) -> Self {
        let set = |v: &Vec<String>| v.iter().map(|s| s.trim().to_lowercase()).collect();
        Self {
            disabled: set(&cfg.disabled_apps),
            no_auto: set(&cfg.auto_exclude_apps),
            no_switch: set(&cfg.no_layout_switch_apps),
        }
    }

    /// Should converted text be accompanied by a layout switch in this app?
    fn switches_layout(&self, app: &str) -> bool {
        !self.no_switch.contains(app)
    }

    fn mode_for(&self, app: &str) -> AppMode {
        if app.is_empty() {
            AppMode::Normal
        } else if self.disabled.contains(app) {
            AppMode::Off
        } else if self.no_auto.contains(app) {
            AppMode::NoAuto
        } else {
            AppMode::Normal
        }
    }
}

/// Cycling state for a word. We always convert from the ORIGINAL text, so cycling never accumulates losses.
struct Cycle {
    origin_text: String,
    origin: LayoutId,
    shown: LayoutId,
}

/// The same for selected text.
struct SelCycle {
    origin_text: String,
    origin: LayoutId,
    shown: LayoutId,
    shown_text: String,
}

pub struct Engine {
    det: Arc<Detector>,
    sounds: Arc<Sounds>,
    busy: Arc<AtomicBool>,
    after: AfterConvert,
    rules: Rules,
    mode: AppMode,
    /// false in apps listed in `no_layout_switch_apps`
    switch_layout: bool,
    keys: Bindings,
    /// languages that do not take part in auto-switching
    disabled_langs: HashSet<Lang>,
    /// keys whose press we swallowed as a hotkey (so the release / auto-repeat is swallowed as well)
    swallowed: HashSet<&'static str>,
    word: Vec<char>, // the current / last word, as it is on screen right now
    tail: String,    // spaces after the word
    cycle: Option<Cycle>,
    sel: Arc<Mutex<Option<SelCycle>>>,
    shift: bool,
    ctrl: bool,
    alt: bool,
    meta: bool,
}

/// Run an action on another thread (the keyboard hook must return quickly).
fn run_busy<F>(busy: &Arc<AtomicBool>, f: F)
where
    F: FnOnce() -> anyhow::Result<()> + Send + 'static,
{
    busy.store(true, Ordering::SeqCst);
    let b = busy.clone();
    thread::spawn(move || {
        if let Err(e) = f() {
            log::error!("action failed: {e:#}");
        }
        thread::sleep(Duration::from_millis(30));
        b.store(false, Ordering::SeqCst);
    });
}

fn parse_disabled(cfg: &Config) -> HashSet<Lang> {
    cfg.disabled_languages.iter().filter_map(|c| Lang::from_code(c)).collect()
}

impl Engine {
    /// Apply a re-read config on the fly (the caller loads the heavy part - dictionaries - beforehand).
    pub fn reconfigure(&mut self, cfg: &Config, keys: Bindings, det: Detector) {
        self.rules = Rules::from_config(cfg);
        self.keys = keys;
        self.det = Arc::new(det);
        self.sounds = Arc::new(Sounds::new(cfg.sounds.clone()));
        self.after = AfterConvert::from_config(cfg);
        self.disabled_langs = parse_disabled(cfg);
        self.swallowed.clear();
        self.reset();
        self.refresh_app();
    }

    pub fn new(cfg: Config, keys: Bindings) -> Self {
        let rules = Rules::from_config(&cfg);
        let app = os::foreground_app();
        let mode = rules.mode_for(&app);
        let switch_layout = rules.switches_layout(&app);
        state::set_app_mode(mode);
        Self {
            det: Arc::new(Detector::new(&cfg)),
            sounds: Arc::new(Sounds::new(cfg.sounds.clone())),
            after: AfterConvert::from_config(&cfg),
            busy: Arc::new(AtomicBool::new(false)),
            rules,
            mode,
            switch_layout,
            keys,
            disabled_langs: parse_disabled(&cfg),
            swallowed: HashSet::new(),
            word: vec![],
            tail: String::new(),
            cycle: None,
            sel: Arc::new(Mutex::new(None)),
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
        }
    }

    fn reset(&mut self) {
        self.word.clear();
        self.tail.clear();
        self.cycle = None;
        if let Ok(mut g) = self.sel.try_lock() {
            *g = None;
        }
    }

    fn set_mode(&mut self, mode: AppMode) {
        self.mode = mode;
        state::set_app_mode(mode); // the tray icon follows (see state.rs)
    }

    fn refresh_app(&mut self) {
        let app = os::foreground_app();
        let mode = self.rules.mode_for(&app);
        self.switch_layout = self.rules.switches_layout(&app);
        self.set_mode(mode);
        log::debug!("app: {app:?}, mode: {mode:?}, switch layout: {}", self.switch_layout);
    }

    /// Is this a hotkey? Some(true): the event was swallowed; None: not our key.
    fn hotkey(&mut self, ev: &KeyEvent) -> Option<bool> {
        if ev.name.is_empty() {
            return None;
        }
        if !ev.down {
            return if self.swallowed.remove(ev.name) { Some(true) } else { None };
        }
        if self.swallowed.contains(ev.name) {
            return Some(true); // auto-repeat of a held hotkey
        }
        let action = self.keys.find(ev.name, self.ctrl, self.alt, self.shift, self.meta)?;
        self.swallowed.insert(ev.name);
        if self.alt || self.meta {
            os::mask_menu();
        }
        if !self.busy.load(Ordering::SeqCst) {
            self.run_action(action);
        }
        Some(true)
    }

    fn run_action(&mut self, a: Action) {
        match a {
            Action::Word => {
                if !self.word.is_empty() {
                    self.cycle_word();
                }
            }
            Action::Selection => self.cycle_selection(),
            Action::ToggleAuto => state::set_auto(!state::auto()), // the tray refreshes itself
            Action::ToggleApp => self.toggle_app_auto(),
        }
    }

    /// true = swallow the event (the application will not see it).
    pub fn handle(&mut self, ev: &KeyEvent) -> bool {
        // Always track the modifiers
        match ev.key {
            K::Shift => { self.shift = ev.down; return false; }
            K::Ctrl => { self.ctrl = ev.down; return false; }
            K::Alt => { self.alt = ev.down; return false; }
            K::Meta => { self.meta = ev.down; return false; }
            _ => {}
        }
        // Synthetic event: the focused window changed / a mouse click - drop the word, recompute the mode
        if ev.key == K::Reset && ev.down && ev.name.is_empty() {
            self.refresh_app();
            self.reset();
            return false;
        }
        if self.mode == AppMode::Off {
            return false;
        }
        if let Some(swallow) = self.hotkey(ev) {
            return swallow;
        }
        if ev.key == K::Reset && ev.down {
            self.reset(); // Enter, Tab, arrows, Esc...
            return false;
        }
        if !ev.down || self.busy.load(Ordering::SeqCst) {
            return false;
        }
        if self.ctrl || self.alt || self.meta {
            self.reset(); // Ctrl+V, Alt+Tab and the like
            return false;
        }
        match ev.key {
            K::Backspace => {
                self.cycle = None;
                if !self.tail.is_empty() { self.tail.pop(); } else { self.word.pop(); }
            }
            K::Space => {
                if !self.word.is_empty() {
                    self.tail.push(' ');
                    if state::auto() && self.mode == AppMode::Normal && self.tail.len() == 1 {
                        self.try_auto();
                    }
                }
            }
            K::Char => {
                if !ev.text.is_empty() && ev.text.chars().all(|c| !c.is_control()) {
                    if !self.tail.is_empty() { self.reset(); }
                    self.cycle = None;
                    self.word.extend(ev.text.chars());
                }
            }
            _ => {}
        }
        false
    }

    /// Auto-switching: the word has just been finished with a space.
    /// Among the installed layouts (languages not disabled in the settings) the detector picks the right one.
    fn try_auto(&mut self) {
        if self.cycle.is_some() {
            return; // the user already converted this word by hand
        }
        let cur = os::current_layout();
        let Some(cl) = os::layout_lang(cur) else { return };
        if self.disabled_langs.contains(&cl) {
            return;
        }
        let mut cands: Vec<(LayoutId, Lang)> = vec![];
        for l in os::layouts() {
            if l == cur {
                continue;
            }
            if let Some(g) = os::layout_lang(l) {
                if g != cl && !self.disabled_langs.contains(&g) && !cands.iter().any(|(_, x)| *x == g) {
                    cands.push((l, g));
                }
            }
        }
        if cands.is_empty() {
            return;
        }
        let w: String = self.word.iter().collect();
        if let Some(to) = self.det.pick_target(&w, (cur, cl), &cands, |t, f, to| os::convert(t, f, to)) {
            self.apply(w, cur, to, SoundEvent::Auto);
        }
    }

    /// Replace the word with `origin_text` as if it had been typed in layout `target`.
    fn apply(&mut self, origin_text: String, origin: LayoutId, target: LayoutId, snd: SoundEvent) {
        let text = os::convert(&origin_text, origin, target);
        let erase = self.word.len() + self.tail.chars().count();
        let out = format!("{text}{}", self.tail);
        self.word = text.chars().collect();
        self.cycle = Some(Cycle { origin_text, origin, shown: target });

        let sounds = self.sounds.clone();
        let to = if self.switch_layout { Some(target) } else { None };
        run_busy(&self.busy, move || {
            os::replace_typed(erase, &out, to)?;
            sounds.play(snd);
            Ok(())
        });
    }

    /// Action `toggle_app_auto` (Alt+Shift+Pause by default): auto-switching for the current app (saved to config.toml).
    fn toggle_app_auto(&mut self) {
        let app = os::foreground_app();
        if app.is_empty() {
            return;
        }
        let excluded = if self.rules.no_auto.remove(&app) {
            false
        } else {
            self.rules.no_auto.insert(app.clone());
            true
        };
        let mode = self.rules.mode_for(&app);
        self.set_mode(mode);
        println!("{app}: auto-switching {}", if excluded { "OFF" } else { "ON" });
        self.sounds.play(SoundEvent::Rule);

        thread::spawn(move || {
            if let Ok(mut c) = Config::load_or_create() {
                c.auto_exclude_apps.retain(|x| !x.eq_ignore_ascii_case(&app));
                if excluded {
                    c.auto_exclude_apps.push(app);
                }
                if let Err(e) = c.save() {
                    log::warn!("could not save config: {e:#}");
                }
            }
        });
    }

    /// convert_word: the next layout in the cycle for the last word.
    fn cycle_word(&mut self) {
        let (origin_text, origin, shown) = match &self.cycle {
            Some(c) => (c.origin_text.clone(), c.origin, c.shown),
            None => {
                let cur = os::current_layout();
                (self.word.iter().collect(), cur, cur)
            }
        };
        let target = os::next_layout(shown);
        if target == shown {
            return; // only one layout is installed
        }
        self.apply(origin_text, origin, target, SoundEvent::Manual);
    }

    /// convert_selection: the next layout in the cycle for the selected text.
    fn cycle_selection(&mut self) {
        let st = self.sel.clone();
        let sounds = self.sounds.clone();
        let mut after = self.after;
        after.switch_layout = self.switch_layout;
        run_busy(&self.busy, move || {
            os::convert_selection(after, move |sel, cur| {
                let mut g = st.lock().unwrap_or_else(|p| p.into_inner());
                // Continue the cycle only if exactly the text we pasted is selected
                let (origin_text, origin, shown) = match g.as_ref() {
                    Some(c) if c.shown_text == sel => (c.origin_text.clone(), c.origin, c.shown),
                    _ => (sel.to_string(), cur, cur),
                };
                let target = os::next_layout(shown);
                if target == shown {
                    return None;
                }
                let text = os::convert(&origin_text, origin, target);
                *g = Some(SelCycle { origin_text, origin, shown: target, shown_text: text.clone() });
                Some((text, target))
            })?;
            sounds.play(SoundEvent::Selection);
            Ok(())
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_modes() {
        let mut cfg = Config::default();
        cfg.disabled_apps = vec!["Game.exe".into()];
        let r = Rules::from_config(&cfg);
        assert_eq!(r.mode_for("game.exe"), AppMode::Off);
        assert_eq!(r.mode_for("code.exe"), AppMode::NoAuto);
        assert_eq!(r.mode_for("notepad.exe"), AppMode::Normal);
        assert_eq!(r.mode_for(""), AppMode::Normal);
    }

    #[test]
    fn layout_switching_can_be_turned_off_per_app() {
        let mut cfg = Config::default();
        cfg.no_layout_switch_apps = vec!["PhotoCraft.exe".into()];
        let r = Rules::from_config(&cfg);
        assert!(!r.switches_layout("photocraft.exe"));
        assert!(r.switches_layout("notepad.exe"));
        assert!(r.switches_layout(""));
    }

    #[test]
    fn disabled_languages_parse() {
        let mut cfg = Config::default();
        cfg.disabled_languages = vec!["de".into(), "bogus".into()];
        let s = parse_disabled(&cfg);
        assert!(s.contains(&Lang(0x07)));
        assert_eq!(s.len(), 1);
    }
}
