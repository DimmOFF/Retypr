//! Settings window (egui). Runs as a separate process: `retypr.exe --settings`.
//! It writes config.toml; the running Retypr picks changes up on its own (within ~1 second).
use crate::{
    autostart,
    config::{CaretSide, Config, HotkeysConfig},
    hotkey::{self, Hotkey},
    icon,
    sound::Sounds,
};
use eframe::egui::{self, vec2, Align, Align2, Color32, FontId, Frame, Layout, Margin, RichText, Rounding, Sense, Stroke};
use std::path::{Path, PathBuf};

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

fn setup_fonts(ctx: &egui::Context) {
    // Segoe UI gives the native Windows look (and Cyrillic coverage for future translations).
    let mut fonts = egui::FontDefinitions::default();
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        fonts.font_data.insert("segoe".to_owned(), egui::FontData::from_owned(bytes));
        if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            f.insert(0, "segoe".to_owned());
        }
    }
    ctx.set_fonts(fonts);
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

/// Pill-shaped on/off chip (used for modifiers and languages).
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

/// Setting row: title + description on the left, control on the right.
fn row(ui: &mut egui::Ui, title: &str, desc: &str, controls: impl FnOnce(&mut egui::Ui)) {
    let w = ui.available_width();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width(w * 0.55);
            ui.label(RichText::new(title).size(15.5));
            if !desc.is_empty() {
                ui.label(RichText::new(desc).size(12.5).color(ui.visuals().weak_text_color()));
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
    let inst = single_instance::SingleInstance::new("Retypr-settings-6f1c9d3a")
        .map_err(|e| anyhow::anyhow!("single-instance: {e:?}"))?;
    if !inst.is_single() {
        return Ok(()); // settings window is already open
    }
    let cfg = Config::load_or_create()?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Retypr Settings")
            .with_inner_size([820.0, 600.0])
            .with_min_inner_size([700.0, 480.0])
            .with_icon(egui::IconData {
                rgba: icon::rgba(true),
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

fn count_words(path: &Path) -> Option<usize> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.lines().filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#')).count())
}

fn open_folder(path: &Path) {
    let _ = std::fs::create_dir_all(path);
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

struct App {
    cfg: Config,
    saved_cfg: Config,
    tab: Tab,
    hk: [Hotkey; 4],
    langs: [bool; 3], // en, ru, uk
    all_keys: Vec<&'static str>,
    new_auto_app: String,
    new_disabled_app: String,
    exceptions: String,
    dict_counts: [Option<usize>; 3],
    autostart: bool,
    player: Sounds,
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
        let has = |c: &str| cfg.auto_languages.iter().any(|x| x.eq_ignore_ascii_case(c));
        let langs = [has("en"), has("ru"), has("uk") || has("ua")];
        let exceptions = std::fs::read_to_string(cfg.dict_path().join("exceptions.txt")).unwrap_or_default();
        let player = Sounds::new(cfg.sounds.clone());
        let mut app = Self {
            tab: Tab::General,
            hk,
            langs,
            all_keys: hotkey::all_keys(),
            new_auto_app: String::new(),
            new_disabled_app: String::new(),
            exceptions,
            dict_counts: [None; 3],
            autostart: autostart::is_enabled(),
            player,
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

    fn refresh_dicts(&mut self) {
        let dir = self.cfg.dict_path();
        self.dict_counts = [
            count_words(&dir.join("en.txt")),
            count_words(&dir.join("ru.txt")),
            count_words(&dir.join("uk.txt")),
        ];
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
        c.auto_languages = ["en", "ru", "uk"]
            .iter()
            .zip(self.langs)
            .filter(|(_, on)| *on)
            .map(|(code, _)| code.to_string())
            .collect();
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
            row(ui, "Languages", "Installed Windows layouts that auto-switching may use.", |ui| {
                // right-to-left layout: add in reverse to show English · Russian · Ukrainian
                chip(ui, pal, "Українська", &mut self.langs[2]);
                chip(ui, pal, "Русский", &mut self.langs[1]);
                chip(ui, pal, "English", &mut self.langs[0]);
            });
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
        });
        section(ui, "System");
        card(ui, pal, |ui| {
            row(ui, "Start with Windows", "Applies immediately.", |ui| {
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
                ui.label(RichText::new(*hint).size(12.5).color(ui.visuals().weak_text_color()));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let hk = &mut self.hk[i];
                    chip(ui, pal, "Ctrl", &mut hk.ctrl);
                    chip(ui, pal, "Alt", &mut hk.alt);
                    chip(ui, pal, "Shift", &mut hk.shift);
                    chip(ui, pal, "Win", &mut hk.meta);
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
                    ui.label(RichText::new("Shortcut").size(12.5).color(ui.visuals().weak_text_color()));
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
            ui.label(RichText::new("Manual hotkeys still work. Good for IDEs and terminals.").size(12.5).color(ui.visuals().weak_text_color()));
            ui.add_space(4.0);
            app_list(ui, pal, "auto_ex", &mut self.cfg.auto_exclude_apps, &mut self.new_auto_app);
        });
        section(ui, "Retypr disabled");
        card(ui, pal, |ui| {
            ui.label(RichText::new("All keys (including Pause) pass through untouched. Good for games and remote desktops.").size(12.5).color(ui.visuals().weak_text_color()));
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
                ui.add(egui::Slider::new(&mut self.cfg.sounds.volume, 0.0..=1.0).show_value(false));
            });
        });
        section(ui, "Events");
        let vol = self.cfg.sounds.volume;
        card(ui, pal, |ui| {
            sound_item(ui, "Manual conversion of a word", &mut self.cfg.sounds.on_manual_switch, &self.player, vol);
            divider(ui);
            sound_item(ui, "Automatic correction", &mut self.cfg.sounds.on_auto_switch, &self.player, vol);
            divider(ui);
            sound_item(ui, "Conversion of selected text", &mut self.cfg.sounds.on_selection_convert, &self.player, vol);
            divider(ui);
            sound_item(ui, "Per-app rule toggled", &mut self.cfg.sounds.on_rule_toggle, &self.player, vol);
        });
    }

    fn page_dict(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        page_header(ui, "Dictionaries", "Word lists let Retypr tell a typo from a wrong layout.");
        card(ui, pal, |ui| {
            let names = ["English", "Russian", "Ukrainian"];
            for (i, name) in names.iter().enumerate() {
                let count = self.dict_counts[i];
                row(ui, name, "", |ui| match count {
                    Some(n) => badge(ui, &format!("{n} words"), GOOD),
                    None => badge(ui, "Not loaded", WARN),
                });
                if i + 1 < names.len() {
                    divider(ui);
                }
            }
        });
        card(ui, pal, |ui| {
            let dir = self.cfg.dict_path();
            row(ui, "Dictionary folder", &dir.display().to_string(), |ui| {
                if ui.button("Refresh").clicked() {
                    self.refresh_dicts();
                }
                if ui.button("Open folder").clicked() {
                    open_folder(&dir);
                }
            });
            ui.add_space(4.0);
            ui.label(RichText::new("Run get-dicts.cmd to download en.txt, ru.txt and uk.txt.").size(12.5).color(ui.visuals().weak_text_color()));
        });
        section(ui, "Exceptions");
        card(ui, pal, |ui| {
            ui.label(RichText::new("Words that must never be converted. One per line, lowercase.").size(12.5).color(ui.visuals().weak_text_color()));
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
            let path = Config::path().map(|p| p.display().to_string()).unwrap_or_default();
            row(ui, "Settings file", &path, |ui| {
                if ui.button("Open folder").clicked() {
                    if let Some(d) = Config::dir() {
                        open_folder(&d);
                    }
                }
            });
        });
        ui.label(RichText::new("Changes are applied by the running Retypr automatically; no restart needed.").size(12.5).color(ui.visuals().weak_text_color()));
    }
}

fn sound_item(ui: &mut egui::Ui, title: &str, path: &mut Option<PathBuf>, player: &Sounds, vol: f32) {
    ui.label(RichText::new(title).size(15.0));
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 260.0).max(120.0);
        let mut text = path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        if ui.add(egui::TextEdit::singleline(&mut text).desired_width(w).hint_text("No sound")).changed() {
            *path = if text.trim().is_empty() { None } else { Some(PathBuf::from(text.trim())) };
        }
        if ui.button("Browse…").clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("Audio", &["wav", "mp3", "ogg", "flac"]).pick_file() {
                *path = Some(p);
            }
        }
        if ui.add_enabled(path.is_some(), egui::Button::new("Test")).clicked() {
            if let Some(p) = path.as_ref() {
                player.play_file(p, vol);
            }
        }
        if ui.add_enabled(path.is_some(), egui::Button::new("Clear")).clicked() {
            *path = None;
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

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.styled {
            self.system_dark = ctx.style().visuals.dark_mode;
            setup_fonts(ctx);
            self.restyle(ctx);
            self.styled = true;
        }
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
                    Tab::Dict => self.page_dict(ui, &pal),
                    Tab::About => self.page_about(ui, &pal, ctx),
                });
            });
    }
}
