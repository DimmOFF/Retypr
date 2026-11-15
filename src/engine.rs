use crate::{
    config::Config,
    detector::Detector,
    hotkey::{Action, Bindings},
    layout_map::Lang,
    os::{self, AfterConvert, KeyEvent, LayoutId, K},
    sound::{SoundEvent, Sounds},
    state,
};
use std::{
    collections::HashSet,
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex},
    thread,
    time::Duration,
};

/// Режим работы в текущем приложении.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AppMode {
    Normal,
    /// Автопереключение выключено, ручные горячие клавиши работают
    NoAuto,
    /// Полностью выключено: клавиши (включая Pause) проходят в приложение как есть
    Off,
}

struct Rules {
    disabled: HashSet<String>,
    no_auto: HashSet<String>,
}

impl Rules {
    fn from_config(cfg: &Config) -> Self {
        let set = |v: &Vec<String>| v.iter().map(|s| s.trim().to_lowercase()).collect();
        Self { disabled: set(&cfg.disabled_apps), no_auto: set(&cfg.auto_exclude_apps) }
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

/// Состояние циклического перебора раскладок для слова.
/// Всегда конвертируем из исходного текста — так цикл не накапливает потерь.
struct Cycle {
    origin_text: String,
    origin: LayoutId,
    shown: LayoutId,
}

/// То же для выделенного текста.
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
    keys: Bindings,
    auto_langs: Vec<Lang>,
    /// клавиши, чьё нажатие мы поглотили как горячую клавишу (чтобы поглотить и отпускание/автоповтор)
    swallowed: HashSet<&'static str>,
    word: Vec<char>, // текущее / последнее слово (как оно сейчас на экране)
    tail: String,    // пробелы после слова
    cycle: Option<Cycle>,
    sel: Arc<Mutex<Option<SelCycle>>>,
    shift: bool,
    ctrl: bool,
    alt: bool,
    meta: bool,
}

/// Выполнить действие в отдельном потоке (хук обязан вернуться быстро).
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

fn parse_langs(cfg: &Config) -> Vec<Lang> {
    let v: Vec<Lang> = cfg.auto_languages.iter().filter_map(|s| Lang::from_code(s)).collect();
    if v.is_empty() { vec![Lang::En, Lang::Ru, Lang::Uk] } else { v }
}

impl Engine {
    /// Применить перечитанный конфиг «на лету» (тяжёлую загрузку словарей вызывающий делает заранее).
    pub fn reconfigure(&mut self, cfg: &Config, keys: Bindings, det: Detector) {
        self.rules = Rules::from_config(cfg);
        self.keys = keys;
        self.det = Arc::new(det);
        self.sounds = Arc::new(Sounds::new(cfg.sounds.clone()));
        self.after = AfterConvert::from_config(cfg);
        self.auto_langs = parse_langs(cfg);
        self.swallowed.clear();
        self.reset();
        self.refresh_app();
    }

    pub fn new(cfg: Config, keys: Bindings) -> Self {
        let rules = Rules::from_config(&cfg);
        let auto_langs = parse_langs(&cfg);
        let mode = rules.mode_for(&os::foreground_app());
        Self {
            det: Arc::new(Detector::new(&cfg)),
            sounds: Arc::new(Sounds::new(cfg.sounds.clone())),
            after: AfterConvert::from_config(&cfg),
            busy: Arc::new(AtomicBool::new(false)),
            rules,
            mode,
            keys,
            auto_langs,
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

    fn refresh_app(&mut self) {
        let app = os::foreground_app();
        self.mode = self.rules.mode_for(&app);
        log::debug!("app: {app:?}, mode: {:?}", self.mode);
    }

    /// Горячая клавиша? Some(true) — событие поглощено, None — не наша клавиша.
    fn hotkey(&mut self, ev: &KeyEvent) -> Option<bool> {
        if ev.name.is_empty() {
            return None;
        }
        if !ev.down {
            return if self.swallowed.remove(ev.name) { Some(true) } else { None };
        }
        if self.swallowed.contains(ev.name) {
            return Some(true); // автоповтор зажатой горячей клавиши
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
            Action::ToggleAuto => state::set_auto(!state::auto()), // трей обновится через колбэк
            Action::ToggleApp => self.toggle_app_auto(),
        }
    }

    /// true = событие подавить (приложение его не увидит).
    pub fn handle(&mut self, ev: &KeyEvent) -> bool {
        // Модификаторы отслеживаем всегда
        match ev.key {
            K::Shift => { self.shift = ev.down; return false; }
            K::Ctrl => { self.ctrl = ev.down; return false; }
            K::Alt => { self.alt = ev.down; return false; }
            K::Meta => { self.meta = ev.down; return false; }
            _ => {}
        }
        // Синтетическое событие: смена окна / клик мышью — слово сбрасываем, режим пересчитываем
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
            self.reset(); // Enter, Tab, стрелки, Esc…
            return false;
        }
        if !ev.down || self.busy.load(Ordering::SeqCst) {
            return false;
        }
        if self.ctrl || self.alt || self.meta {
            self.reset(); // Ctrl+V, Alt+Tab и т.п.
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

    /// Автопереключение: слово только что закончилось пробелом.
    /// Среди установленных раскладок (из `auto_languages`) детектор выбирает подходящую.
    fn try_auto(&mut self) {
        if self.cycle.is_some() {
            return; // слово уже конвертировал пользователь вручную
        }
        let cur = os::current_layout();
        let Some(cl) = os::layout_lang(cur) else { return };
        if !self.auto_langs.contains(&cl) {
            return;
        }
        let mut cands: Vec<(LayoutId, Lang)> = vec![];
        for l in os::layouts() {
            if l == cur {
                continue;
            }
            if let Some(g) = os::layout_lang(l) {
                if g != cl && self.auto_langs.contains(&g) && !cands.iter().any(|(_, x)| *x == g) {
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

    /// Заменить слово на `origin_text`, «набранный» в раскладке `target`.
    fn apply(&mut self, origin_text: String, origin: LayoutId, target: LayoutId, snd: SoundEvent) {
        let text = os::convert(&origin_text, origin, target);
        let erase = self.word.len() + self.tail.chars().count();
        let out = format!("{text}{}", self.tail);
        self.word = text.chars().collect();
        self.cycle = Some(Cycle { origin_text, origin, shown: target });

        let sounds = self.sounds.clone();
        run_busy(&self.busy, move || {
            os::replace_typed(erase, &out, target)?;
            sounds.play(snd);
            Ok(())
        });
    }

    /// Действие toggle_app_auto (по умолчанию Alt+Shift+Pause): автопереключение для текущего приложения (с сохранением в config.toml).
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
        self.mode = self.rules.mode_for(&app);
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

    /// convert_word: следующая раскладка по кругу для последнего слова.
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
            return; // установлена всего одна раскладка
        }
        self.apply(origin_text, origin, target, SoundEvent::Manual);
    }

    /// convert_selection: следующая раскладка по кругу для выделенного текста.
    fn cycle_selection(&mut self) {
        let st = self.sel.clone();
        let sounds = self.sounds.clone();
        let after = self.after;
        run_busy(&self.busy, move || {
            os::convert_selection(after, move |sel, cur| {
                let mut g = st.lock().unwrap_or_else(|p| p.into_inner());
                // Продолжаем цикл, только если выделен ровно тот текст, что мы вставили
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
}
