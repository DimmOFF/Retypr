//! Settings window (egui). Runs as a separate process: `Retypr.exe --settings`.
//! It writes config.toml; the running Retypr picks the changes up on its own (within ~1 second).
use crate::{
    autostart,
    config::{CaretSide, Config, HotkeysConfig},
    hotkey::{self, Hotkey},
    icon::{self, IconKind},
    lang::Lang,
    os,
    sound::Sounds,
    tray::SETTINGS_TITLE,
};
use eframe::egui::{self, vec2, Align, Align2, Color32, FontId, Frame, Layout, Margin, RichText, Rounding, Sense, Stroke};
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};

const GITHUB_URL: &str = "https://github.com/DimmOFF/Retypr";

// ───────────────────────────── palette & style ─────────────────────────────

const ACCENT: Color32 = Color32::from_rgb(46, 125, 246);
const GOOD: Color32 = Color32::from_rgb(52, 168, 83);
const WARN: Color32 = Color32::from_rgb(230, 150, 30);
const BAD: Color32 = Color32::from_rgb(220, 70, 70);

#[derive(Clone, Copy)]
struct Pal {
    sidebar: Color32,
    page: Color32,
    card: Color32,
    stroke: Color32,
    chip: Color32,
    keycap: Color32,
}

fn palette(dark: bool) -> Pal {
    if dark {
        Pal {
            sidebar: Color32::from_rgb(22, 24, 28),
            page: Color32::from_rgb(28, 30, 35),
            card: Color32::from_rgb(37, 40, 46),
            stroke: Color32::from_rgb(54, 58, 66),
            chip: Color32::from_rgb(52, 56, 64),
            keycap: Color32::from_rgb(46, 50, 58),
        }
    } else {
        Pal {
            sidebar: Color32::from_rgb(238, 240, 244),
            page: Color32::from_rgb(247, 248, 250),
            card: Color32::from_rgb(255, 255, 255),
            stroke: Color32::from_rgb(222, 225, 231),
            chip: Color32::from_rgb(233, 236, 241),
            keycap: Color32::from_rgb(240, 242, 246),
        }
    }
}

#[cfg(windows)]
fn setup_fonts(ctx: &egui::Context) {
    // Segoe UI gives the native Windows look (and covers Cyrillic and many other scripts).
    let mut fonts = egui::FontDefinitions::default();
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        fonts.font_data.insert("segoe".to_owned(), egui::FontData::from_owned(bytes));
        if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            f.insert(0, "segoe".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

/// macOS: egui's built-in fonts already cover Latin, Cyrillic and Greek.
#[cfg(not(windows))]
fn setup_fonts(_ctx: &egui::Context) {}

/// OS-specific wording.
#[cfg(windows)]
mod os_text {
    pub const START_LABEL: &str = "Start with Windows";
    pub const SYSTEM: &str = "Windows";
    pub const ALT: &str = "Alt";
    pub const META: &str = "Win";
}
#[cfg(target_os = "macos")]
mod os_text {
    pub const START_LABEL: &str = "Start at login";
    pub const SYSTEM: &str = "macOS";
    pub const ALT: &str = "Option";
    pub const META: &str = "Cmd";
}

fn apply_style(ctx: &egui::Context, dark: bool) -> Pal {
    let pal = palette(dark);
    let mut style = (*ctx.style()).clone();

    use egui::TextStyle::*;
    style.text_styles = [
        (Heading, FontId::proportional(26.0)),
        (Body, FontId::proportional(15.0)),
        (Button, FontId::proportional(15.0)),
        (Small, FontId::proportional(12.5)),
        (Monospace, FontId::monospace(14.0)),
    ]
    .into();

    style.spacing.item_spacing = vec2(10.0, 9.0);
    style.spacing.button_padding = vec2(14.0, 6.0);
    style.spacing.interact_size = vec2(40.0, 30.0);
    style.spacing.slider_width = 150.0;

    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = pal.page;
    v.window_fill = pal.card;
    v.extreme_bg_color = if dark { Color32::from_rgb(24, 26, 30) } else { Color32::from_rgb(244, 245, 248) };
    v.faint_bg_color = pal.keycap;
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.hyperlink_color = ACCENT;
    v.slider_trailing_fill = true;
    v.window_rounding = Rounding::same(12.0);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal.stroke);
    v.widgets.noninteractive.rounding = Rounding::same(8.0);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.rounding = Rounding::same(8.0);
    }
    v.widgets.inactive.weak_bg_fill = pal.chip;
    v.widgets.inactive.bg_fill = pal.chip;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.weak_bg_fill = if dark { Color32::from_rgb(62, 67, 77) } else { Color32::from_rgb(224, 228, 235) };
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.active.bg_stroke = Stroke::NONE;
    style.visuals = v;
    ctx.set_style(style);
    pal
}

// ───────────────────────────── small widgets ─────────────────────────────

/// iOS-style toggle switch.
fn toggle(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let size = vec2(44.0, 24.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let how = ui.ctx().animate_bool(resp.id, *on);
        let off_col = if ui.visuals().dark_mode { Color32::from_rgb(78, 84, 96) } else { Color32::from_rgb(190, 195, 205) };
        let track = off_col.lerp_to_gamma(ACCENT, how);
        let radius = rect.height() / 2.0;
        ui.painter().rect_filled(rect, radius, track);
        let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how);
        ui.painter().circle_filled(egui::pos2(x, rect.center().y), radius - 3.0, Color32::WHITE);
    }
    resp
}

/// Pill-shaped on/off chip (used for modifiers).
fn chip(ui: &mut egui::Ui, pal: &Pal, text: &str, on: &mut bool) -> egui::Response {
    let (bg, fg) = if *on { (ACCENT, Color32::WHITE) } else { (pal.chip, ui.visuals().text_color()) };
    let r = ui.add(
        egui::Button::new(RichText::new(text).color(fg).size(14.0))
            .fill(bg)
            .stroke(Stroke::NONE)
            .rounding(16.0)
            .min_size(vec2(0.0, 28.0)),
    );
    if r.clicked() {
        *on = !*on;
    }
    r
}

/// Single-choice chip; returns true when clicked.
fn choice_chip(ui: &mut egui::Ui, pal: &Pal, text: &str, selected: bool) -> bool {
    let (bg, fg) = if selected { (ACCENT, Color32::WHITE) } else { (pal.chip, ui.visuals().text_color()) };
    ui.add(
        egui::Button::new(RichText::new(text).color(fg).size(14.0))
            .fill(bg)
            .stroke(Stroke::NONE)
            .rounding(16.0)
            .min_size(vec2(0.0, 28.0)),
    )
    .clicked()
}

fn keycap(ui: &mut egui::Ui, pal: &Pal, text: &str) {
    Frame::none()
        .fill(pal.keycap)
        .stroke(Stroke::new(1.0_f32, pal.stroke))
        .rounding(6.0)
        .inner_margin(Margin::symmetric(9.0, 3.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).monospace().size(13.0));
        });
}

fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    Frame::none()
        .fill(Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 38))
        .rounding(10.0)
        .inner_margin(Margin::symmetric(10.0, 3.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(13.0).color(color).strong());
        });
}

fn card(ui: &mut egui::Ui, pal: &Pal, add: impl FnOnce(&mut egui::Ui)) {
    Frame::none()
        .fill(pal.card)
        .stroke(Stroke::new(1.0_f32, pal.stroke))
        .rounding(12.0)
        .inner_margin(Margin::symmetric(18.0, 14.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    ui.add_space(6.0);
}

fn divider(ui: &mut egui::Ui) {
    ui.add_space(2.0);
    ui.separator();
    ui.add_space(2.0);
}

fn weak(ui: &egui::Ui, text: &str) -> RichText {
    RichText::new(text).size(12.5).color(ui.visuals().weak_text_color())
}

/// Setting row: title + description on the left, control on the right.
fn row(ui: &mut egui::Ui, title: &str, desc: &str, controls: impl FnOnce(&mut egui::Ui)) {
    let w = ui.available_width();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width(w * 0.55);
            ui.label(RichText::new(title).size(15.5));
            if !desc.is_empty() {
                ui.label(weak(ui, desc));
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), controls);
    });
}

fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(26.0).strong());
    ui.label(RichText::new(subtitle).color(ui.visuals().weak_text_color()));
    ui.add_space(14.0);
}

fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text.to_uppercase()).size(11.5).strong().color(ui.visuals().weak_text_color()));
    ui.add_space(2.0);
}

fn nav_item(ui: &mut egui::Ui, selected: bool, text: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, 8.0, Color32::from_rgba_unmultiplied(46, 125, 246, 40));
        let bar = egui::Rect::from_min_size(rect.left_center() + vec2(0.0, -10.0), vec2(3.0, 20.0));
        ui.painter().rect_filled(bar, 2.0, ACCENT);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 8.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    let color = if selected { ui.visuals().strong_text_color() } else { ui.visuals().text_color() };
    ui.painter().text(rect.left_center() + vec2(16.0, 0.0), Align2::LEFT_CENTER, text, FontId::proportional(15.5), color);
    resp
}

/// Vector logo: blue disc with a pixel "R" (same as the tray icon, but crisp at any size).
fn draw_logo(ui: &mut egui::Ui, size: f32) {
    const GLYPH: [&str; 7] = ["1111.", "1...1", "1...1", "1111.", "1.1..", "1..1.", "1...1"];
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let p = ui.painter();
    p.circle_filled(rect.center(), size / 2.0, ACCENT);
    let s = size * 0.62 / 7.0;
    let origin = rect.center() - vec2(5.0 * s / 2.0, 7.0 * s / 2.0);
    for (ry, line) in GLYPH.iter().enumerate() {
        for (rx, ch) in line.chars().enumerate() {
            if ch == '1' {
                let r = egui::Rect::from_min_size(origin + vec2(rx as f32 * s, ry as f32 * s), vec2(s + 0.5, s + 0.5));
                p.rect_filled(r, 0.0, Color32::WHITE);
            }
        }
    }
}

/// The OS command that opens a file, folder or URL with its default application.
fn opener() -> &'static str {
    if cfg!(target_os = "macos") { "open" } else { "explorer" }
}

fn open_url(url: &str) {
    let _ = std::process::Command::new(opener()).arg(url).spawn();
}

fn open_folder(path: &Path) {
    let _ = std::fs::create_dir_all(path);
    let _ = std::process::Command::new(opener()).arg(path).spawn();
}

// ───────────────────────────── dictionaries ─────────────────────────────

#[derive(Clone)]
struct DictInfo {
    words: usize,
    modified: Option<SystemTime>,
}

#[derive(Clone)]
enum Dl {
    Running,
    Failed(String),
}

type Downloads = Arc<Mutex<HashMap<Lang, Dl>>>;
type Infos = Arc<Mutex<HashMap<Lang, DictInfo>>>;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn dict_info(path: &Path) -> Option<DictInfo> {
    let text = std::fs::read_to_string(path).ok()?;
    let words = text.lines().filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#')).count();
    if words == 0 {
        return None;
    }
    Some(DictInfo { words, modified: std::fs::metadata(path).and_then(|m| m.modified()).ok() })
}

fn ago(t: SystemTime) -> String {
    let secs = SystemTime::now().duration_since(t).map(|d| d.as_secs()).unwrap_or(0);
    match secs {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=86399 => format!("{} h ago", secs / 3600),
        _ => format!("{} days ago", secs / 86400),
    }
}

fn fmt_num(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Download <lang>_50k.txt (50,000 most frequent words, FrequencyWords project, CC-BY-SA 4.0) as <dir>/<code>.txt.
fn download_dict(dir: &Path, lang: Lang) -> Result<DictInfo, String> {
    let code = lang.code();
    let url = format!("https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/{code}/{code}_50k.txt");
    let resp = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(404, _) => format!("no word list is available for {}", lang.name()),
            other => other.to_string(),
        })?;
    let mut body = String::new();
    resp.into_reader()
        .take(32 * 1024 * 1024)
        .read_to_string(&mut body)
        .map_err(|e| e.to_string())?;
    if body.lines().filter(|l| !l.trim().is_empty()).count() < 100 {
        return Err("the downloaded file looks empty".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{code}.txt.part"));
    let dst = dir.join(format!("{code}.txt"));
    std::fs::write(&tmp, &body).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &dst).map_err(|e| e.to_string())?;
    dict_info(&dst).ok_or_else(|| "could not read the saved file".to_string())
}

/// Start a background download. A running Retypr reloads its dictionaries by itself afterwards.
fn spawn_download(dir: &Path, dl: &Downloads, infos: &Infos, ctx: &egui::Context, lang: Lang) {
    {
        let mut g = lock(dl);
        if matches!(g.get(&lang), Some(Dl::Running)) {
            return;
        }
        g.insert(lang, Dl::Running);
    }
    let (dir, dl, infos, ctx) = (dir.to_path_buf(), dl.clone(), infos.clone(), ctx.clone());
    std::thread::spawn(move || {
        let result = download_dict(&dir, lang);
        {
            let mut g = lock(&dl);
            match &result {
                Ok(_) => {
                    g.remove(&lang);
                }
                Err(e) => {
                    g.insert(lang, Dl::Failed(e.clone()));
                }
            }
        }
        if let Ok(info) = result {
            lock(&infos).insert(lang, info);
            Config::touch(); // makes a running Retypr reload
        }
        ctx.request_repaint();
    });
}

fn set_lang_enabled(cfg: &mut Config, lang: Lang, on: bool) {
    cfg.disabled_languages.retain(|c| Lang::from_code(c) != Some(lang));
    if !on {
        cfg.disabled_languages.push(lang.code());
    }
}

// ───────────────────────────── sounds ─────────────────────────────

/// Copy a chosen sound file into <data>/sounds (so the program folder stays portable) and return the
/// path to store in the config (relative to the data folder when possible).
fn import_sound(src: &Path) -> String {
    let Some(data) = Config::dir() else { return src.to_string_lossy().to_string() };
    let rel = Config::relativize(src);
    if !Path::new(&rel).is_absolute() {
        return rel; // already inside the data folder
    }
    let sounds = data.join("sounds");
    if std::fs::create_dir_all(&sounds).is_err() {
        return src.to_string_lossy().to_string();
    }
    let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "sound".into());
    let ext = src.extension().map(|s| format!(".{}", s.to_string_lossy())).unwrap_or_default();
    let src_len = std::fs::metadata(src).map(|m| m.len()).ok();
    let mut dest = sounds.join(format!("{stem}{ext}"));
    let mut n = 1;
    // same name and same size: it is the same file, reuse it; otherwise pick a free name
    while dest.exists() && std::fs::metadata(&dest).map(|m| m.len()).ok() != src_len {
        n += 1;
        dest = sounds.join(format!("{stem}-{n}{ext}"));
    }
    if !dest.exists() && std::fs::copy(src, &dest).is_err() {
        return src.to_string_lossy().to_string();
    }
    Config::relativize(&dest)
}

/// What the user sees: the absolute path (relative config values are resolved against the data folder).
fn display_path(stored: &str) -> String {
    let s = stored.trim();
    if s.is_empty() {
        return String::new();
    }
    Config::resolve(s).components().collect::<PathBuf>().display().to_string()
}

/// What goes into config.toml: relative to the data folder when the file is inside it (keeps the folder
/// portable), otherwise the absolute path as is.
fn stored_path(shown: &str) -> String {
    let s = shown.trim();
    if s.is_empty() {
        return String::new();
    }
    let p = Path::new(s);
    if p.is_absolute() {
        Config::relativize(p)
    } else {
        s.to_string()
    }
}

/// One sound: `cfg_path` is the value stored in the config, `text` the absolute path shown in the edit box.
fn sound_item(ui: &mut egui::Ui, title: &str, cfg_path: &mut String, text: &mut String, player: &Sounds, vol: f32) {
    ui.label(RichText::new(title).size(15.0));
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 260.0).max(120.0);
        if ui.add(egui::TextEdit::singleline(text).desired_width(w).hint_text("No sound")).changed() {
            *cfg_path = stored_path(text);
        }
        if ui.button("Browse…").clicked() {
            let mut dlg = rfd::FileDialog::new().add_filter("Audio", &["wav", "mp3", "ogg", "flac"]);
            // open the dialog in the folder of the current sound (or in data/sounds when there is none)
            if cfg_path.trim().is_empty() {
                if let Some(d) = Config::dir() {
                    dlg = dlg.set_directory(d.join("sounds"));
                }
            } else {
                let cur = Config::resolve(cfg_path.trim());
                if let Some(dir) = cur.parent().filter(|d| d.is_dir()) {
                    dlg = dlg.set_directory(dir);
                    if let Some(name) = cur.file_name() {
                        dlg = dlg.set_file_name(name.to_string_lossy().to_string());
                    }
                }
            }
            if let Some(p) = dlg.pick_file() {
                *cfg_path = import_sound(&p);
                *text = display_path(cfg_path);
            }
        }
        let has = !cfg_path.trim().is_empty();
        if ui.add_enabled(has, egui::Button::new("Test")).clicked() {
            player.play_file(&Config::resolve(cfg_path.trim()), vol);
        }
        if ui.add_enabled(has, egui::Button::new("Clear")).clicked() {
            cfg_path.clear();
            text.clear();
        }
    });
}

fn app_list(ui: &mut egui::Ui, pal: &Pal, id: &str, list: &mut Vec<String>, input: &mut String) {
    ui.push_id(id, |ui| {
        let mut remove = None;
        if list.is_empty() {
            ui.label(RichText::new("No apps added yet.").italics().color(ui.visuals().weak_text_color()));
        }
        for (i, app) in list.iter().enumerate() {
            ui.horizontal(|ui| {
                keycap(ui, pal, app);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove {
            list.remove(i);
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(input).hint_text("e.g. notepad++.exe").desired_width(200.0));
            let add = ui.button("Add").clicked();
            if ui.button("Browse…").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("Programs", &["exe"]).pick_file() {
                    if let Some(n) = p.file_name() {
                        *input = n.to_string_lossy().to_lowercase();
                    }
                }
            }
            if add {
                let v = input.trim().to_lowercase();
                if !v.is_empty() && !list.iter().any(|x| x.eq_ignore_ascii_case(&v)) {
                    list.push(v);
                }
                input.clear();
            }
        });
    });
}

// ───────────────────────────── app ─────────────────────────────

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    General,
    Hotkeys,
    Apps,
    Sounds,
    Dict,
    About,
}

const TABS: [(Tab, &str); 6] = [
    (Tab::General, "General"),
    (Tab::Hotkeys, "Hotkeys"),
    (Tab::Apps, "Apps"),
    (Tab::Sounds, "Sounds"),
    (Tab::Dict, "Dictionaries"),
    (Tab::About, "About"),
];

const HK_ROWS: [(&str, &str); 4] = [
    ("Convert last word", "Press again to cycle through every installed layout."),
    ("Convert selected text", "Copies the selection, converts it and pastes it back."),
    ("Toggle auto-switching", "Global on/off switch for automatic correction."),
    ("Toggle auto-switching for this app", "Adds or removes the active window's app from the exclusion list."),
];

pub fn run() -> anyhow::Result<()> {
    let inst = crate::instance::Instance::new("Retypr-settings-6f1c9d3a")?;
    if !inst.is_single() {
        return Ok(()); // the settings window is already open
    }
    let cfg = Config::load_or_create()?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(SETTINGS_TITLE)
            .with_inner_size([820.0, 600.0])
            .with_min_inner_size([700.0, 480.0])
            .with_icon(egui::IconData {
                rgba: icon::rgba(IconKind::Blue),
                width: icon::SIZE as u32,
                height: icon::SIZE as u32,
            }),
        ..Default::default()
    };
    eframe::run_native("Retypr settings", options, Box::new(move |_cc| Ok(Box::new(App::new(cfg)))))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    drop(inst);
    Ok(())
}

fn parse_or(s: &str, default: &str) -> Hotkey {
    Hotkey::parse(s).or_else(|_| Hotkey::parse(default)).unwrap_or_default()
}

struct App {
    cfg: Config,
    saved_cfg: Config,
    tab: Tab,
    hk: [Hotkey; 4],
    /// languages of the keyboard layouts installed in the system
    installed: Vec<Lang>,
    all_keys: Vec<&'static str>,
    new_auto_app: String,
    new_disabled_app: String,
    exceptions: String,
    dict_infos: Infos,
    downloads: Downloads,
    autostart: bool,
    player: Sounds,
    /// the sound paths as shown in the Sounds page (absolute): manual, auto, selection, rule
    sound_text: [String; 4],
    status: String,
    saved: String, // snapshot of the saved state, to detect unsaved changes
    styled: bool,
    system_dark: bool,
    dark: bool,
    pal: Pal,
}

impl App {
    fn new(cfg: Config) -> Self {
        let d = HotkeysConfig::default();
        let hk = [
            parse_or(&cfg.hotkeys.convert_word, &d.convert_word),
            parse_or(&cfg.hotkeys.convert_selection, &d.convert_selection),
            parse_or(&cfg.hotkeys.toggle_auto, &d.toggle_auto),
            parse_or(&cfg.hotkeys.toggle_app_auto, &d.toggle_app_auto),
        ];
        let exceptions = std::fs::read_to_string(cfg.dict_path().join("exceptions.txt")).unwrap_or_default();
        let player = Sounds::new(cfg.sounds.clone());
        let mut app = Self {
            tab: Tab::General,
            hk,
            installed: os::installed_languages(),
            all_keys: hotkey::all_keys(),
            new_auto_app: String::new(),
            new_disabled_app: String::new(),
            exceptions,
            dict_infos: Arc::new(Mutex::new(HashMap::new())),
            downloads: Arc::new(Mutex::new(HashMap::new())),
            autostart: autostart::is_enabled(),
            player,
            sound_text: [
                display_path(&cfg.sounds.on_manual_switch),
                display_path(&cfg.sounds.on_auto_switch),
                display_path(&cfg.sounds.on_selection_convert),
                display_path(&cfg.sounds.on_rule_toggle),
            ],
            status: String::new(),
            saved: String::new(),
            styled: false,
            system_dark: true,
            dark: true,
            pal: palette(true),
            saved_cfg: cfg.clone(),
            cfg,
        };
        app.refresh_dicts();
        app.saved = app.snapshot();
        app
    }

    fn restyle(&mut self, ctx: &egui::Context) {
        self.dark = match self.cfg.theme.as_str() {
            "dark" => true,
            "light" => false,
            _ => self.system_dark,
        };
        self.pal = apply_style(ctx, self.dark);
    }

    fn refresh_dicts(&self) {
        let dir = self.cfg.dict_path();
        let mut m = HashMap::new();
        for l in &self.installed {
            if let Some(i) = dict_info(&dir.join(format!("{}.txt", l.code()))) {
                m.insert(*l, i);
            }
        }
        *lock(&self.dict_infos) = m;
    }

    fn hk_config(&self) -> HotkeysConfig {
        HotkeysConfig {
            convert_word: self.hk[0].to_config_string(),
            convert_selection: self.hk[1].to_config_string(),
            toggle_auto: self.hk[2].to_config_string(),
            toggle_app_auto: self.hk[3].to_config_string(),
        }
    }

    /// Config with every edit made in the window applied.
    fn current_cfg(&self) -> Config {
        let mut c = self.cfg.clone();
        c.hotkeys = self.hk_config();
        c
    }

    fn snapshot(&self) -> String {
        format!("{}\n--\n{}", toml::to_string(&self.current_cfg()).unwrap_or_default(), self.exceptions)
    }

    fn conflicts(&self) -> Vec<String> {
        let mut v = vec![];
        for i in 0..4 {
            if self.hk[i].key.is_empty() {
                v.push(format!("Choose a key for \"{}\".", HK_ROWS[i].0));
            }
        }
        for i in 0..4 {
            for j in i + 1..4 {
                if !self.hk[i].key.is_empty() && self.hk[i] == self.hk[j] {
                    v.push(format!(
                        "\"{}\" and \"{}\" use the same shortcut ({}).",
                        HK_ROWS[i].0,
                        HK_ROWS[j].0,
                        self.hk[i].to_config_string()
                    ));
                }
            }
        }
        v
    }

    fn save(&mut self) {
        let cfg = self.current_cfg();
        let result = cfg.save().and_then(|_| {
            let dir = cfg.dict_path();
            std::fs::create_dir_all(&dir)?;
            std::fs::write(dir.join("exceptions.txt"), &self.exceptions)?;
            Ok(())
        });
        match result {
            Ok(()) => {
                self.saved_cfg = cfg.clone();
                self.cfg = cfg;
                self.saved = self.snapshot();
                self.status = "Saved. The running Retypr applies it within a second.".into();
                self.refresh_dicts();
            }
            Err(e) => self.status = format!("Could not save: {e:#}"),
        }
    }

    fn discard(&mut self, ctx: &egui::Context) {
        let (tab, sys, styled) = (self.tab, self.system_dark, self.styled);
        *self = App::new(self.saved_cfg.clone());
        self.tab = tab;
        self.system_dark = sys;
        self.styled = styled;
        self.restyle(ctx);
        self.status = "Changes discarded.".into();
    }

    /// macOS: the menu bar process asks us to come to the front / to close through small files in the data folder.
    #[cfg(target_os = "macos")]
    fn handle_requests(&mut self, ctx: &egui::Context) {
        ctx.request_repaint_after(std::time::Duration::from_millis(400)); // keep polling while idle
        let Some(dir) = Config::dir() else { return };
        let take = |name: &str| std::fs::remove_file(dir.join(name)).is_ok();
        if take(".settings-raise") {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        if take(".settings-close") {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    // ───────────── pages ─────────────

    fn page_general(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        page_header(ui, "General", "How Retypr corrects text while you type.");
        section(ui, "Auto-switching");
        card(ui, pal, |ui| {
            row(ui, "Automatic layout correction", "Fixes a word typed in the wrong layout when you press Space.", |ui| {
                toggle(ui, &mut self.cfg.auto_switch);
            });
            divider(ui);
            row(ui, "Minimum word length", "Shorter words are never corrected automatically.", |ui| {
                ui.add(egui::Slider::new(&mut self.cfg.min_word_len, 2..=8));
            });
            divider(ui);
            ui.label(weak(ui, "Which languages take part, and their dictionaries: see the Dictionaries page."));
        });
        section(ui, "Converting selected text");
        card(ui, pal, |ui| {
            row(
                ui,
                "Remove selection after converting",
                "Off: the text stays selected, so pressing the hotkey again cycles to the next layout.",
                |ui| {
                    toggle(ui, &mut self.cfg.deselect_after_convert);
                },
            );
            divider(ui);
            row(
                ui,
                "Caret position after converting",
                "Which end of the converted text the cursor ends up at. With the selection kept, it is the active end of the selection.",
                |ui| {
                    let left = self.cfg.caret_after_convert == CaretSide::Left;
                    // right-to-left layout: add Right first to show Left · Right
                    if choice_chip(ui, pal, "Right", !left) {
                        self.cfg.caret_after_convert = CaretSide::Right;
                    }
                    if choice_chip(ui, pal, "Left", left) {
                        self.cfg.caret_after_convert = CaretSide::Left;
                    }
                },
            );
            divider(ui);
            row(
                ui,
                "Restore selection and caret up to",
                "Putting the selection or caret back costs one keystroke per character, so very long texts are skipped (the caret stays at the right end). 0 = no limit.",
                |ui| {
                    ui.add(
                        egui::Slider::new(&mut self.cfg.selection_restore_limit, 0..=20000)
                            .logarithmic(true)
                            .suffix(" chars"),
                    );
                },
            );
        });
        section(ui, "System");
        card(ui, pal, |ui| {
            row(ui, os_text::START_LABEL, "Applies immediately.", |ui| {
                if toggle(ui, &mut self.autostart).changed() {
                    if let Err(e) = autostart::set(self.autostart) {
                        self.status = format!("Autostart: {e}");
                        self.autostart = autostart::is_enabled();
                    }
                }
            });
        });
    }

    fn page_hotkeys(&mut self, ui: &mut egui::Ui, pal: &Pal, conflicts: &[String]) {
        page_header(ui, "Hotkeys", "Pick modifiers and a key. \"Break\" is Ctrl+Pause.");
        for c in conflicts {
            card(ui, pal, |ui| {
                ui.label(RichText::new(c).color(BAD));
            });
        }
        let defaults = HotkeysConfig::default();
        let default_strs = [&defaults.convert_word, &defaults.convert_selection, &defaults.toggle_auto, &defaults.toggle_app_auto];
        for (i, (title, hint)) in HK_ROWS.iter().enumerate() {
            card(ui, pal, |ui| {
                ui.label(RichText::new(*title).size(15.5).strong());
                ui.label(weak(ui, hint));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let hk = &mut self.hk[i];
                    chip(ui, pal, "Ctrl", &mut hk.ctrl);
                    chip(ui, pal, os_text::ALT, &mut hk.alt);
                    chip(ui, pal, "Shift", &mut hk.shift);
                    chip(ui, pal, os_text::META, &mut hk.meta);
                    ui.label("+");
                    egui::ComboBox::from_id_salt(i)
                        .width(130.0)
                        .selected_text(hotkey::pretty(hk.key))
                        .show_ui(ui, |ui| {
                            for k in &self.all_keys {
                                ui.selectable_value(&mut hk.key, *k, hotkey::pretty(k));
                            }
                        });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Reset").clicked() {
                            *hk = parse_or(default_strs[i], default_strs[i]);
                        }
                    });
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(weak(ui, "Shortcut"));
                    let s = self.hk[i].to_config_string();
                    for (n, part) in s.split('+').enumerate() {
                        if n > 0 {
                            ui.label("+");
                        }
                        keycap(ui, pal, part);
                    }
                });
            });
        }
    }

    fn page_apps(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        page_header(ui, "Apps", "Per-application behavior. Use the executable name, e.g. code.exe.");
        section(ui, "No auto-switching");
        card(ui, pal, |ui| {
            ui.label(weak(ui, "Manual hotkeys still work. Good for IDEs and terminals. The tray icon turns half gray while such an app has focus."));
            ui.add_space(4.0);
            app_list(ui, pal, "auto_ex", &mut self.cfg.auto_exclude_apps, &mut self.new_auto_app);
        });
        section(ui, "Retypr disabled");
        card(ui, pal, |ui| {
            ui.label(weak(ui, "All keys (including Pause) pass through untouched. Good for games and remote desktops."));
            ui.add_space(4.0);
            app_list(ui, pal, "disabled", &mut self.cfg.disabled_apps, &mut self.new_disabled_app);
        });
    }

    fn page_sounds(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        page_header(ui, "Sounds", "Play a sound when Retypr switches a layout. Formats: wav, mp3, ogg, flac.");
        card(ui, pal, |ui| {
            row(ui, "Enable sounds", "", |ui| {
                toggle(ui, &mut self.cfg.sounds.enabled);
            });
            divider(ui);
            row(ui, "Volume", "", |ui| {
                ui.add(
                    egui::Slider::new(&mut self.cfg.sounds.volume, 0.0..=1.0)
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                        .custom_parser(|s| s.trim().trim_end_matches('%').trim().parse::<f64>().ok().map(|v| v / 100.0)),
                );
            });
        });
        section(ui, "Events");
        let vol = self.cfg.sounds.volume;
        card(ui, pal, |ui| {
            let [t_manual, t_auto, t_selection, t_rule] = &mut self.sound_text;
            let sounds = &mut self.cfg.sounds;
            sound_item(ui, "Manual conversion of a word", &mut sounds.on_manual_switch, t_manual, &self.player, vol);
            divider(ui);
            sound_item(ui, "Automatic correction", &mut sounds.on_auto_switch, t_auto, &self.player, vol);
            divider(ui);
            sound_item(ui, "Conversion of selected text", &mut sounds.on_selection_convert, t_selection, &self.player, vol);
            divider(ui);
            sound_item(ui, "Per-app rule toggled", &mut sounds.on_rule_toggle, t_rule, &self.player, vol);
        });
        ui.label(weak(
            ui,
            "Paths are shown in full; inside the program folder they are stored relative to it. Files you pick elsewhere are copied into the \"sounds\" folder next to Retypr.exe, so the program folder stays portable.",
        ));
    }

    fn page_dict(&mut self, ui: &mut egui::Ui, pal: &Pal, ctx: &egui::Context) {
        page_header(
            ui,
            "Dictionaries",
            &format!("Auto-switching works with the keyboard layouts installed in {}. A word list per language lets Retypr tell a typo from a wrong layout.", os_text::SYSTEM),
        );

        let langs = self.installed.clone();
        let dir = self.cfg.dict_path();
        let (dl, infos) = (self.downloads.clone(), self.dict_infos.clone());

        if langs.is_empty() {
            card(ui, pal, |ui| {
                ui.label("No keyboard layouts were found.");
            });
            return;
        }

        // ── all languages at once ──
        card(ui, pal, |ui| {
            let all_on = langs.iter().all(|l| !self.cfg.is_language_disabled(*l));
            row(ui, "All languages", "Turn auto-switching (and its dictionary) on or off for every installed layout at once.", |ui| {
                let mut on = all_on;
                if toggle(ui, &mut on).changed() {
                    for l in &langs {
                        set_lang_enabled(&mut self.cfg, *l, on);
                    }
                }
            });
            divider(ui);
            ui.horizontal(|ui| {
                let any_enabled = langs.iter().any(|l| !self.cfg.is_language_disabled(*l));
                if ui.add_enabled(any_enabled, egui::Button::new("Download / update all enabled")).clicked() {
                    for l in langs.iter().filter(|l| !self.cfg.is_language_disabled(**l)) {
                        spawn_download(&dir, &dl, &infos, ctx, *l);
                    }
                }
            });
        });

        // ── one row per installed language ──
        section(ui, "Installed layouts");
        card(ui, pal, |ui| {
            for (n, &lang) in langs.iter().enumerate() {
                let enabled = !self.cfg.is_language_disabled(lang);
                let info = lock(&infos).get(&lang).cloned();
                let state = lock(&dl).get(&lang).cloned();
                let builtin = lang == Lang::EN || lang == Lang::RU || lang == Lang::UK;
                let desc = match &state {
                    Some(Dl::Failed(msg)) => format!("Download failed: {msg}"),
                    _ if builtin => format!("{}.txt · built-in rules also work without a dictionary", lang.code()),
                    _ => format!("{}.txt · a dictionary is required to detect this language", lang.code()),
                };
                row(ui, &lang.name(), &desc, |ui| {
                    // right-to-left: toggle, button, then the status badge
                    let mut on = enabled;
                    if toggle(ui, &mut on).changed() {
                        set_lang_enabled(&mut self.cfg, lang, on);
                    }
                    if matches!(state, Some(Dl::Running)) {
                        ui.add(egui::Spinner::new());
                    } else {
                        let label = if info.is_some() { "Update" } else { "Download" };
                        if ui.button(label).clicked() {
                            spawn_download(&dir, &dl, &infos, ctx, lang);
                        }
                    }
                    match &info {
                        Some(i) => {
                            let when = i.modified.map(ago).map(|a| format!(" · {a}")).unwrap_or_default();
                            badge(ui, &format!("{} words{when}", fmt_num(i.words)), GOOD);
                        }
                        None => badge(ui, "No dictionary", WARN),
                    }
                });
                if n + 1 < langs.len() {
                    divider(ui);
                }
            }
        });

        card(ui, pal, |ui| {
            row(ui, "Dictionary folder", &dir.display().to_string(), |ui| {
                if ui.button("Refresh").clicked() {
                    self.refresh_dicts();
                }
                if ui.button("Open folder").clicked() {
                    open_folder(&dir);
                }
            });
            ui.add_space(4.0);
            ui.label(weak(ui, "Word lists: FrequencyWords by Hermit Dave (CC-BY-SA 4.0), 50,000 most frequent words per language."));
        });

        section(ui, "Exceptions");
        card(ui, pal, |ui| {
            ui.label(weak(ui, "Words that must never be converted. One per line, lowercase."));
            ui.add_space(4.0);
            ui.add(egui::TextEdit::multiline(&mut self.exceptions).desired_rows(7).desired_width(f32::INFINITY));
        });
    }

    fn page_about(&mut self, ui: &mut egui::Ui, pal: &Pal, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            draw_logo(ui, 64.0);
            ui.vertical(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("Retypr").size(28.0).strong());
                ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(ui.visuals().weak_text_color()));
            });
        });
        ui.add_space(6.0);
        ui.label("Keyboard layout switcher: automatic correction and hotkeys.");
        ui.add_space(14.0);

        section(ui, "Project");
        card(ui, pal, |ui| {
            row(ui, "Source code and releases", GITHUB_URL, |ui| {
                if ui.button("Open GitHub").clicked() {
                    open_url(GITHUB_URL);
                }
            });
        });

        section(ui, "Appearance");
        card(ui, pal, |ui| {
            row(ui, "Theme", "", |ui| {
                let cur = self.cfg.theme.clone();
                let mut pick = None;
                for (id, name) in [("dark", "Dark"), ("light", "Light"), ("system", "System")] {
                    if choice_chip(ui, pal, name, cur == id) {
                        pick = Some(id);
                    }
                }
                if let Some(id) = pick {
                    self.cfg.theme = id.to_string();
                    self.restyle(ctx);
                }
            });
        });

        section(ui, "Files");
        card(ui, pal, |ui| {
            let dir = Config::dir().map(|p| p.display().to_string()).unwrap_or_default();
            row(ui, "Data folder", &dir, |ui| {
                if ui.button("Open folder").clicked() {
                    if let Some(d) = Config::dir() {
                        open_folder(&d);
                    }
                }
            });
            ui.add_space(4.0);
            ui.label(weak(
                ui,
                "Settings, dictionaries and sounds live next to Retypr.exe. Move the whole program folder anywhere; relative paths keep working.",
            ));
        });
        ui.label(weak(ui, "Changes are applied by the running Retypr automatically; no restart needed."));
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.styled {
            self.system_dark = ctx.style().visuals.dark_mode;
            setup_fonts(ctx);
            self.restyle(ctx);
            self.styled = true;
        }
        #[cfg(target_os = "macos")]
        self.handle_requests(ctx);
        let pal = self.pal;
        let conflicts = self.conflicts();
        let dirty = self.snapshot() != self.saved;
        let can_save = dirty && conflicts.is_empty();

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) && can_save {
            self.save();
        }

        // ── sidebar ──
        egui::SidePanel::left("nav")
            .exact_width(212.0)
            .resizable(false)
            .frame(Frame::none().fill(pal.sidebar).inner_margin(Margin::symmetric(12.0, 18.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    draw_logo(ui, 34.0);
                    ui.vertical(|ui| {
                        ui.add_space(1.0);
                        ui.label(RichText::new("Retypr").size(18.0).strong());
                        ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).size(12.0).color(ui.visuals().weak_text_color()));
                    });
                });
                ui.add_space(18.0);
                for (t, name) in TABS {
                    if nav_item(ui, self.tab == t, name).clicked() {
                        self.tab = t;
                        self.status.clear();
                    }
                }
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.add_space(4.0);
                    ui.label(RichText::new("Ctrl+S to save").size(12.0).color(ui.visuals().weak_text_color()));
                });
            });

        // ── bottom bar ──
        egui::TopBottomPanel::bottom("bar")
            .frame(Frame::none().fill(pal.sidebar).inner_margin(Margin::symmetric(24.0, 12.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (color, text) = if !conflicts.is_empty() {
                        (BAD, "Fix the shortcut conflicts to save".to_string())
                    } else if dirty {
                        (WARN, "Unsaved changes".to_string())
                    } else if !self.status.is_empty() {
                        (GOOD, self.status.clone())
                    } else {
                        (GOOD, "All changes saved".to_string())
                    };
                    let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 4.0, color);
                    ui.label(RichText::new(text).color(ui.visuals().weak_text_color()));

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let save = egui::Button::new(RichText::new("Save changes").color(Color32::WHITE).strong())
                            .fill(ACCENT)
                            .stroke(Stroke::NONE)
                            .rounding(8.0)
                            .min_size(vec2(130.0, 34.0));
                        if ui.add_enabled(can_save, save).clicked() {
                            self.save();
                        }
                        if ui.add_enabled(dirty, egui::Button::new("Discard").min_size(vec2(84.0, 34.0))).clicked() {
                            self.discard(ctx);
                        }
                    });
                });
            });

        // ── page ──
        egui::CentralPanel::default()
            .frame(Frame::none().fill(pal.page).inner_margin(Margin::symmetric(28.0, 22.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
                    Tab::General => self.page_general(ui, &pal),
                    Tab::Hotkeys => self.page_hotkeys(ui, &pal, &conflicts),
                    Tab::Apps => self.page_apps(ui, &pal),
                    Tab::Sounds => self.page_sounds(ui, &pal),
                    Tab::Dict => self.page_dict(ui, &pal, ctx),
                    Tab::About => self.page_about(ui, &pal, ctx),
                });
            });
    }
}
