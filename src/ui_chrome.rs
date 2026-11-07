//! Fensteraufbau: Schriften, Farben, Menüs, Werkzeugleiste, Tabs, Statuszeile und Tastenkürzel.
use crate::app::{Action, App, HashKind};
use crate::commands::Cmd;
use crate::editor;
use crate::fileio::{self, Enc, Eol};
use crate::highlight;
use crate::icons::{self, Icon};
use crate::theme::{self, ThemeId};
use crate::textops::CaseKind;
use crate::ui_find::FindTab;
use egui::{pos2, vec2, Align2, Button, Color32, Context, FontData, FontDefinitions, FontFamily, FontId, Key, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// Schriftauswahl: je Eintrag die Dateien in Wunschreihenfolge. Zuerst der Windows-Name,
/// danach gleich breite Schriften, die es unter Linux gibt.
pub const FONT_CHOICES: [(&str, &[&str], &[&str]); 4] = [
    (
        "Courier New",
        &["cour.ttf", "Courier_New.ttf", "LiberationMono-Regular.ttf", "FreeMono.ttf", "DejaVuSansMono.ttf"],
        &["courbd.ttf", "Courier_New_Bold.ttf", "LiberationMono-Bold.ttf", "FreeMonoBold.ttf", "DejaVuSansMono-Bold.ttf"],
    ),
    (
        "Consolas",
        &["consola.ttf", "Inconsolata-Regular.ttf", "DejaVuSansMono.ttf", "LiberationMono-Regular.ttf"],
        &["consolab.ttf", "Inconsolata-Bold.ttf", "DejaVuSansMono-Bold.ttf", "LiberationMono-Bold.ttf"],
    ),
    (
        "Lucida Console",
        &["lucon.ttf", "DejaVuSansMono.ttf", "LiberationMono-Regular.ttf"],
        &["lucon.ttf", "DejaVuSansMono-Bold.ttf", "LiberationMono-Bold.ttf"],
    ),
    (
        "Cascadia Mono",
        &["CascadiaMono.ttf", "CascadiaMono-Regular.ttf", "DejaVuSansMono.ttf"],
        &["CascadiaMono.ttf", "CascadiaMono-Bold.ttf", "DejaVuSansMono-Bold.ttf"],
    ),
];

/// Die Ordner, in denen das Betriebssystem seine Schriften ablegt.
fn font_dirs() -> Vec<PathBuf> {
    if cfg!(windows) {
        let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| "C:\\Windows".into());
        return vec![win.join("Fonts")];
    }
    let mut dirs_found: Vec<PathBuf> = dirs::font_dir().into_iter().collect();
    if let Some(home) = dirs::home_dir() {
        dirs_found.push(home.join(".fonts"));
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    dirs_found.extend(data_dirs.split(':').filter(|d| !d.is_empty()).map(|d| Path::new(d).join("fonts")));
    dirs_found
}

/// Alle Schriftdateien des Systems, einmal eingelesen. Linux sortiert sie in Unterordner.
fn font_index() -> &'static HashMap<String, PathBuf> {
    static INDEX: OnceLock<HashMap<String, PathBuf>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut map = HashMap::new();
        for dir in font_dirs() {
            for e in walkdir::WalkDir::new(dir).follow_links(true).max_depth(6).into_iter().flatten() {
                if e.file_type().is_file() {
                    let name = e.file_name().to_string_lossy().to_lowercase();
                    map.entry(name).or_insert_with(|| e.path().to_path_buf());
                }
            }
        }
        map
    })
}

/// Die erste brauchbare Schrift aus der Liste. Unlesbare Dateien werden übersprungen.
fn system_font(names: &[&str]) -> Option<Vec<u8>> {
    names.iter().find_map(|name| {
        let bytes = std::fs::read(font_index().get(&name.to_lowercase())?).ok()?;
        ab_glyph::FontRef::try_from_slice(&bytes).is_ok().then_some(bytes)
    })
}

/// Lädt Schriften aus dem System; fehlt eine, bleibt die eingebaute Ersatzschrift.
pub fn setup_fonts(ctx: &Context, editor_font: &str, retro: bool) {
    let mut defs = FontDefinitions::default();
    let add = |defs: &mut FontDefinitions, key: &str, files: &[&str]| -> bool {
        match system_font(files) {
            Some(bytes) => {
                defs.font_data.insert(key.to_string(), Arc::new(FontData::from_owned(bytes)));
                true
            }
            None => false,
        }
    };
    let mut fallbacks: Vec<String> = Vec::new();
    let sym: &[&str] = &["seguisym.ttf", "DejaVuSans.ttf", "NotoSansSymbols2-Regular.ttf", "Symbola.ttf"];
    let cjk: &[&str] = &["msyh.ttc", "NotoSansCJK-Regular.ttc", "wqy-microhei.ttc", "wqy-zenhei.ttc", "DroidSansFallbackFull.ttf"];
    for (key, files) in [("sym", sym), ("cjk", cjk)] {
        if add(&mut defs, key, files) {
            fallbacks.push(key.to_string());
        }
    }
    let mono_default = defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    let mut prop = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    // Der klassische Look nimmt Microsoft Sans Serif, die TrueType-Fassung von MS Sans Serif.
    let ui_files: &[&str] = if retro {
        &["micross.ttf", "tahoma.ttf", "segoeui.ttf", "LiberationSans-Regular.ttf", "DejaVuSans.ttf"]
    } else {
        &["segoeui.ttf", "NotoSans-Regular.ttf", "LiberationSans-Regular.ttf", "DejaVuSans.ttf"]
    };
    if add(&mut defs, "ui", ui_files) {
        prop.insert(0, "ui".to_string());
    }
    prop.extend(fallbacks.iter().cloned());
    // Symbolschrift: Segoe Fluent Icons (Windows 11), sonst Segoe MDL2 Assets (Windows 10).
    let icon_font = if add(&mut defs, icons::FAMILY, &["SegoeIcons.ttf"]) {
        icons::IconFont::Fluent
    } else if add(&mut defs, icons::FAMILY, &["segmdl2.ttf"]) {
        icons::IconFont::Mdl2
    } else {
        icons::IconFont::None
    };
    if icon_font != icons::IconFont::None {
        let mut fam = vec![icons::FAMILY.to_string()];
        fam.extend(prop.iter().cloned());
        defs.families.insert(FontFamily::Name(icons::FAMILY.into()), fam);
    }
    icons::set_icon_font(icon_font);
    defs.families.insert(FontFamily::Proportional, prop);

    let choice = FONT_CHOICES.iter().find(|c| c.0 == editor_font).unwrap_or(&FONT_CHOICES[0]);
    let mut regular = Vec::new();
    let mut bold = Vec::new();
    if add(&mut defs, "ed", choice.1) {
        regular.push("ed".to_string());
        if add(&mut defs, "edb", choice.2) {
            bold.push("edb".to_string());
        } else {
            bold.push("ed".to_string());
        }
    }
    regular.extend(mono_default.iter().cloned());
    bold.extend(mono_default.iter().cloned());
    regular.extend(fallbacks.iter().cloned());
    bold.extend(fallbacks.iter().cloned());
    defs.families.insert(FontFamily::Name(editor::EDITOR_FAMILY.into()), regular);
    defs.families.insert(FontFamily::Name(editor::EDITOR_BOLD_FAMILY.into()), bold);
    ctx.set_fonts(defs);
}

#[derive(Clone, Copy)]
pub struct Chrome {
    pub window: Color32,
    pub menu: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub tab_inactive: Color32,
    pub tab_active: Color32,
    pub tab_text: Color32,
    pub accent: Color32,
    pub line: Color32,
    pub hover: Color32,
    pub hover_line: Color32,
    /// Schrift auf hervorgehobenen Einträgen.
    pub hover_text: Color32,
}

pub fn chrome(t: ThemeId) -> Chrome {
    crate::theme::chrome(t)
}

/// Ein Eintrag der Tastenkürzel-Tabelle.
pub struct Shortcut {
    pub keys: &'static str,
    pub label: &'static str,
    ctrl: bool,
    shift: bool,
    alt: bool,
    key: Key,
    editor_only: bool,
    action: fn() -> Action,
}

macro_rules! sc {
    ($keys:expr, $label:expr, $c:expr, $s:expr, $a:expr, $key:ident, $ed:expr, $act:expr) => {
        Shortcut { keys: $keys, label: $label, ctrl: $c, shift: $s, alt: $a, key: Key::$key, editor_only: $ed, action: || $act }
    };
}

pub fn shortcuts() -> Vec<Shortcut> {
    use Action as A;
    vec![
        sc!("Strg+N", "Neu", true, false, false, N, false, A::New),
        sc!("Strg+O", "Öffnen", true, false, false, O, false, A::Open),
        sc!("Strg+S", "Speichern", true, false, false, S, false, A::Save),
        sc!("Strg+Alt+S", "Speichern unter", true, false, true, S, false, A::SaveAs),
        sc!("Strg+Umschalt+S", "Alle speichern", true, true, false, S, false, A::SaveAll),
        sc!("Strg+W", "Schließen", true, false, false, W, false, A::Close),
        sc!("Strg+Umschalt+W", "Alle schließen", true, true, false, W, false, A::CloseAll),
        sc!("Strg+P", "Drucken", true, false, false, P, false, A::Print),
        sc!("Strg+Tab", "Nächster Tab", true, false, false, Tab, false, A::NextTab),
        sc!("Strg+Umschalt+Tab", "Vorheriger Tab", true, true, false, Tab, false, A::PrevTab),
        sc!("Strg+Z", "Rückgängig", true, false, false, Z, true, A::Cmd(Cmd::Undo)),
        sc!("Strg+Y", "Wiederholen", true, false, false, Y, true, A::Cmd(Cmd::Redo)),
        sc!("Strg+Umschalt+Z", "Wiederholen", true, true, false, Z, true, A::Cmd(Cmd::Redo)),
        sc!("Strg+A", "Alles markieren", true, false, false, A, true, A::Cmd(Cmd::SelectAll)),
        sc!("Strg+D", "Zeile duplizieren", true, false, false, D, true, A::Cmd(Cmd::DuplicateLines)),
        sc!("Strg+L", "Zeile löschen", true, false, false, L, true, A::Cmd(Cmd::DeleteLines)),
        sc!("Strg+J", "Zeilen verbinden", true, false, false, J, true, A::Cmd(Cmd::JoinLines)),
        sc!("Strg+Umschalt+Hoch", "Zeile nach oben", true, true, false, ArrowUp, true, A::Cmd(Cmd::MoveLinesUp)),
        sc!("Strg+Umschalt+Runter", "Zeile nach unten", true, true, false, ArrowDown, true, A::Cmd(Cmd::MoveLinesDown)),
        sc!("Strg+U", "Kleinbuchstaben", true, false, false, U, true, A::Cmd(Cmd::Case(CaseKind::Lower))),
        sc!("Strg+Umschalt+U", "GROSSBUCHSTABEN", true, true, false, U, true, A::Cmd(Cmd::Case(CaseKind::Upper))),
        sc!("Strg+Q", "Kommentar umschalten", true, false, false, Q, true, A::Cmd(Cmd::Comment(None))),
        sc!("Strg+K", "Zeile kommentieren", true, false, false, K, true, A::Cmd(Cmd::Comment(Some(true)))),
        sc!("Strg+Umschalt+K", "Kommentar entfernen", true, true, false, K, true, A::Cmd(Cmd::Comment(Some(false)))),
        sc!("Strg+Umschalt+Q", "Blockkommentar", true, true, false, Q, true, A::Cmd(Cmd::BlockComment)),
        sc!("Strg+Leertaste", "Wort vervollständigen", true, false, false, Space, true, A::Autocomplete),
        sc!("Alt+C", "Zahlenfolge einfügen", false, false, true, C, true, A::ColumnEditor),
        sc!("Strg+F", "Suchen", true, false, false, F, false, A::ShowFind(FindTab::Find)),
        sc!("Strg+H", "Ersetzen", true, false, false, H, false, A::ShowFind(FindTab::Replace)),
        sc!("Strg+Umschalt+F", "In Dateien suchen", true, true, false, F, false, A::ShowFind(FindTab::Files)),
        sc!("Strg+M", "Treffer hervorheben", true, false, false, M, false, A::ShowFind(FindTab::Mark)),
        sc!("F3", "Weitersuchen", false, false, false, F3, false, A::FindNext),
        sc!("Umschalt+F3", "Rückwärts suchen", false, true, false, F3, false, A::FindPrev),
        sc!("Strg+F3", "Markierten Text suchen", true, false, false, F3, false, A::FindSelectedNext),
        sc!("Strg+G", "Gehe zu", true, false, false, G, false, A::GoToDialog),
        sc!("Strg+B", "Zur passenden Klammer", true, false, false, B, true, A::MatchBrace),
        sc!("Strg+F2", "Lesezeichen setzen/entfernen", true, false, false, F2, false, A::BookmarkToggle),
        sc!("F2", "Nächstes Lesezeichen", false, false, false, F2, false, A::BookmarkNext),
        sc!("Umschalt+F2", "Vorheriges Lesezeichen", false, true, false, F2, false, A::BookmarkPrev),
        sc!("F11", "Vollbild", false, false, false, F11, false, A::ToggleFullscreen),
        sc!("Alt+0", "Alles einklappen", false, false, true, Num0, false, A::FoldAll),
        sc!("Alt+Umschalt+0", "Alles ausklappen", false, true, true, Num0, false, A::UnfoldAll),
        sc!("Strg+Alt+F", "Aktuelle Ebene ein-/ausklappen", true, false, true, F, false, A::FoldCurrent),
        sc!("Strg+Plus", "Vergrößern", true, false, false, Plus, false, A::ZoomIn),
        sc!("Strg+=", "Vergrößern", true, false, false, Equals, false, A::ZoomIn),
        sc!("Strg+Minus", "Verkleinern", true, false, false, Minus, false, A::ZoomOut),
        sc!("Strg+0", "Normale Größe", true, false, false, Num0, false, A::ZoomReset),
    ]
}

/// Knöpfe ohne Hintergrund und Rahmen, solange die Maus nicht darüber ist.
fn flat(ui: &mut Ui) {
    let w = &mut ui.style_mut().visuals.widgets;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_stroke = Stroke::NONE;
}

fn mi(ui: &mut Ui, pending: &mut Vec<Action>, label: &str, keys: &str, action: Action) {
    mi_en(ui, pending, label, keys, true, action);
}

fn mi_en(ui: &mut Ui, pending: &mut Vec<Action>, label: &str, keys: &str, enabled: bool, action: Action) {
    let mut b = Button::new(label);
    if !keys.is_empty() {
        b = b.shortcut_text(keys);
    }
    if ui.add_enabled(enabled, b).clicked() {
        pending.push(action);
        ui.close_menu();
    }
}

fn check(ui: &mut Ui, pending: &mut Vec<Action>, label: &str, on: bool, keys: &str, action: Action) {
    // Eingeschaltetes ist farbig hinterlegt, so bleiben alle Einträge bündig.
    let mut b = Button::new(label).selected(on);
    if !keys.is_empty() {
        b = b.shortcut_text(keys);
    }
    if ui.add(b).clicked() {
        pending.push(action);
        ui.close_menu();
    }
}

impl App {
    pub fn apply_style(&self, ctx: &Context) {
        let theme = self.settings.theme;
        let c = chrome(theme);
        let pal = self.palette();
        let retro = theme.retro();
        let mut v = if theme.is_dark() { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = c.window;
        v.window_fill = c.window;
        v.menu_corner_radius = 0.into();
        v.window_corner_radius = 2.into();
        v.widgets.noninteractive.fg_stroke.color = c.text;
        v.widgets.inactive.fg_stroke.color = c.text;
        v.widgets.inactive.corner_radius = 2.into();
        v.widgets.hovered.corner_radius = 2.into();
        v.widgets.active.corner_radius = 2.into();
        v.widgets.open.corner_radius = 2.into();
        v.widgets.hovered.weak_bg_fill = c.hover;
        v.widgets.hovered.bg_fill = c.hover;
        v.widgets.hovered.fg_stroke.color = c.hover_text;
        v.widgets.active.fg_stroke.color = c.text;
        v.widgets.open.fg_stroke.color = c.text;
        v.extreme_bg_color = pal.bg;
        v.faint_bg_color = c.tab_inactive;
        if !theme.is_dark() {
            // Kästchen und Eingabefelder klar erkennbar auf dem Fensterhintergrund.
            v.widgets.inactive.bg_fill = pal.bg;
            v.widgets.inactive.bg_stroke = Stroke::new(1.0, c.line.gamma_multiply(0.85));
            v.widgets.inactive.weak_bg_fill = c.tab_inactive.lerp_to_gamma(c.window, 0.6);
            v.widgets.hovered.bg_stroke = Stroke::new(1.0, c.hover_line);
        }
        v.selection.bg_fill = c.hover_line;
        v.selection.stroke.color = c.hover_text;
        if retro {
            // Klassisch: eckig, graue Knopfflächen mit schwarzem Rand, Auswahl dunkelblau.
            let zero = egui::CornerRadius::ZERO;
            v.menu_corner_radius = zero;
            v.window_corner_radius = zero;
            for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
                w.corner_radius = zero;
            }
            v.widgets.inactive.weak_bg_fill = c.window;
            v.widgets.inactive.bg_fill = Color32::WHITE;
            v.widgets.inactive.bg_stroke = Stroke::new(1.0, crate::theme::W98_DARK);
            v.widgets.hovered.bg_stroke = Stroke::new(1.0, crate::theme::W98_DARK);
            v.widgets.active.weak_bg_fill = crate::theme::W98_LIGHT;
            v.widgets.active.bg_fill = crate::theme::W98_LIGHT;
            v.widgets.active.bg_stroke = Stroke::new(1.0, crate::theme::W98_DARK);
            v.widgets.open.weak_bg_fill = c.hover;
            v.widgets.open.fg_stroke.color = c.hover_text;
            v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, crate::theme::W98_SHADOW);
            v.window_stroke = Stroke::new(1.0, crate::theme::W98_DARK);
            v.window_shadow = egui::Shadow::NONE;
            v.popup_shadow = egui::Shadow::NONE;
            v.extreme_bg_color = Color32::WHITE;
        }
        ctx.set_visuals(v);
        let size = if retro { 13.0 } else { 14.0 };
        ctx.style_mut(|s| {
            s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(size));
            s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(size));
            s.text_styles.insert(egui::TextStyle::Small, FontId::proportional(size - 2.0));
            s.text_styles.insert(egui::TextStyle::Heading, FontId::proportional(size + 1.0));
            s.spacing.button_padding = vec2(8.0, 3.0);
            s.spacing.menu_margin = 4.0.into();
            s.interaction.selectable_labels = false;
        });
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
    }

    pub fn handle_shortcuts(&mut self, ctx: &Context) {
        // Nur der Rest der Tasten bleibt für das Schreibfeld übrig.
        let editor_focus = self.editor_focused;
        let table = shortcuts();
        ctx.input_mut(|i| {
            let mut fired = Vec::new();
            i.events.retain(|e| {
                if let egui::Event::Key { key, pressed: true, modifiers, .. } = e {
                    for s in &table {
                        if s.key == *key
                            && s.ctrl == (modifiers.ctrl || modifiers.command)
                            && s.shift == modifiers.shift
                            && s.alt == modifiers.alt
                            && (!s.editor_only || editor_focus)
                        {
                            fired.push((s.action)());
                            return false;
                        }
                    }
                }
                true
            });
            self.pending.extend(fired);
        });
    }

    pub fn draw_main(&mut self, ctx: &Context) {
        let c = chrome(self.settings.theme);
        egui::TopBottomPanel::top("menu")
            .frame(egui::Frame::NONE.fill(c.menu).inner_margin(egui::Margin { left: 3, right: 3, top: 2, bottom: 2 }))
            .show(ctx, |ui| self.menu_bar(ui));
        egui::TopBottomPanel::top("tools")
            .exact_height(32.0)
            .frame(egui::Frame::NONE.fill(c.window).inner_margin(egui::Margin { left: 6, right: 4, top: 3, bottom: 3 }))
            .show_separator_line(false)
            .show(ctx, |ui| self.toolbar(ui));
        egui::TopBottomPanel::bottom("status")
            .exact_height(26.0)
            .frame(egui::Frame::NONE.fill(c.window))
            .show_separator_line(false)
            .show(ctx, |ui| self.status_bar(ui));
        // Suchleiste und Dateisuche liegen als Leisten am Rand, darum vor dem Schreibfeld.
        self.draw_find(ctx);
        self.results_panel(ctx);
        self.side_panels(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(c.window).inner_margin(egui::Margin { left: 5, right: 2, top: 0, bottom: 0 }))
            .show(ctx, |ui| self.draw_panes(ui));
    }

    fn draw_panes(&mut self, ui: &mut Ui) {
        let full = ui.max_rect();
        let split = self.is_split();
        if split {
            let sep = 6.0;
            let x = full.left() + full.width() * self.split_frac;
            let left = Rect::from_min_max(full.min, pos2(x - sep * 0.5, full.bottom()));
            let right = Rect::from_min_max(pos2(x + sep * 0.5, full.top()), full.max);
            let handle = Rect::from_min_max(pos2(x - sep * 0.5, full.top()), pos2(x + sep * 0.5, full.bottom()));
            let r = ui.interact(handle, ui.id().with("split"), Sense::drag());
            if r.hovered() || r.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if r.dragged() {
                self.split_frac = (self.split_frac + r.drag_delta().x / full.width()).clamp(0.15, 0.85);
            }
            self.draw_pane(ui, 0, left);
            self.draw_pane(ui, 1, right);
        } else {
            self.draw_pane(ui, 0, full);
        }
    }

    fn draw_pane(&mut self, ui: &mut Ui, pane: usize, rect: Rect) {
        let tab_h = 30.0;
        let tab_rect = Rect::from_min_size(rect.min, vec2(rect.width(), tab_h));
        let edit_outer = Rect::from_min_max(pos2(rect.left(), rect.top() + tab_h), rect.max);
        self.tab_bar(ui, pane, tab_rect);
        if self.panes[pane].tabs.is_empty() {
            return;
        }
        let theme_id = self.settings.theme;
        let c = chrome(theme_id);
        let edit_rect = if theme_id.retro() {
            // Vertiefte Doppelkante wie bei einem klassischen Eingabefeld.
            theme::bevel(ui.painter(), edit_outer, false);
            edit_outer.shrink(2.0)
        } else {
            ui.painter().rect_stroke(edit_outer, 0.0, Stroke::new(1.0, c.line), StrokeKind::Inside);
            edit_outer.shrink(1.0)
        };
        let pal = self.palette();
        let p = &mut self.panes[pane];
        let i = p.active.min(p.tabs.len() - 1);
        let view = &mut p.tabs[i];
        let Some(doc) = self.docs.get_mut(&view.doc) else { return };
        let Some(metrics) = self.metrics.as_mut() else { return };
        doc.hl.set_theme(theme_id);
        let mut child = ui.new_child(UiBuilder::new().max_rect(edit_rect).id_salt(("pane", pane)));
        let (out, resp) = editor::show(&mut child, doc, view, &self.settings.ed, metrics, &pal);
        if out.focused {
            self.active_pane = pane;
        }
        if pane == self.active_pane {
            self.editor_focused = out.focused;
        }
        if let Some(clip) = out.clipboard {
            ui.ctx().copy_text(clip);
        }
        if out.zoom > 1.001 {
            self.pending.push(Action::ZoomIn);
        } else if out.zoom < 0.999 {
            self.pending.push(Action::ZoomOut);
        }
        if let Some((top, x)) = out.scrolled {
            if self.is_split() && (self.sync_v || self.sync_h) {
                let other = &mut self.panes[1 - pane];
                let a = other.active;
                if let Some(o) = other.tabs.get_mut(a) {
                    if self.sync_v {
                        o.top_row = top;
                    }
                    if self.sync_h {
                        o.scroll_x = x;
                    }
                }
            }
        }
        let pending = &mut self.pending;
        resp.context_menu(|ui| {
            ui.set_min_width(190.0);
            mi(ui, pending, "Ausschneiden", "Strg+X", Action::Cmd(Cmd::Cut));
            mi(ui, pending, "Kopieren", "Strg+C", Action::Cmd(Cmd::Copy));
            mi(ui, pending, "Einfügen", "Strg+V", Action::Paste);
            mi(ui, pending, "Löschen", "Entf", Action::Cmd(Cmd::DeleteSelection));
            ui.separator();
            mi(ui, pending, "Alles markieren", "Strg+A", Action::Cmd(Cmd::SelectAll));
            ui.separator();
            mi(ui, pending, "GROSSBUCHSTABEN", "Strg+Umschalt+U", Action::Cmd(Cmd::Case(CaseKind::Upper)));
            mi(ui, pending, "Kleinbuchstaben", "Strg+U", Action::Cmd(Cmd::Case(CaseKind::Lower)));
            mi(ui, pending, "Kommentar umschalten", "Strg+Q", Action::Cmd(Cmd::Comment(None)));
        });
    }

    fn tab_bar(&mut self, ui: &mut Ui, pane: usize, rect: Rect) {
        let c = chrome(self.settings.theme);
        let retro = self.settings.theme.retro();
        let font = FontId::proportional(if self.settings.theme.retro() { 13.0 } else { 14.0 });
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, c.window);
        let ctx = ui.ctx().clone();
        let scroll_w = 46.0;
        let names: Vec<(String, bool, bool, u64)> = self.panes[pane]
            .tabs
            .iter()
            .map(|v| {
                let d = &self.docs[&v.doc];
                (d.name(), d.is_modified(), d.read_only, v.uid)
            })
            .collect();
        let widths: Vec<f32> = names
            .iter()
            .map(|(n, modified, read_only, _)| {
                let w = ctx.fonts(|f| f.layout_no_wrap(n.clone(), font.clone(), Color32::WHITE).size().x);
                let marks = if *read_only { 16.0 } else { 0.0 } + if *modified { 13.0 } else { 0.0 };
                (w + marks + 40.0).max(96.0)
            })
            .collect();
        let total: f32 = widths.iter().sum::<f32>() + widths.len() as f32 * 2.0;
        let overflow = total > rect.width();
        let bar = if overflow {
            Rect::from_min_max(rect.min, pos2(rect.right() - scroll_w, rect.bottom()))
        } else {
            rect
        };
        let max_scroll = (total - bar.width()).max(0.0);
        let bar_resp = ui.interact(bar, ui.id().with(("tabs", pane)), Sense::hover());
        if bar_resp.hovered() {
            let d = ui.input(|i| i.smooth_scroll_delta);
            self.panes[pane].scroll -= d.y + d.x;
        }
        self.panes[pane].scroll = self.panes[pane].scroll.clamp(0.0, max_scroll);
        let scroll = self.panes[pane].scroll;
        let active = self.panes[pane].active;
        let tp = painter.with_clip_rect(bar);
        let mut x = rect.left() - scroll;
        let pointer = ui.input(|i| i.pointer.hover_pos());
        for (i, (name, modified, read_only, uid)) in names.iter().enumerate() {
            let w = widths[i];
            let is_active = i == active;
            let tr = Rect::from_min_max(pos2(x, rect.top() + if is_active { 0.0 } else { 2.0 }), pos2(x + w, rect.bottom()));
            x += w + 2.0;
            if tr.right() < bar.left() || tr.left() > bar.right() {
                continue;
            }
            let id = ui.id().with(("tab", *uid));
            let resp = ui.interact(tr.intersect(bar), id, Sense::click_and_drag());
            let hover_tab = pointer.is_some_and(|p| tr.contains(p));
            let fill = if is_active {
                c.tab_active
            } else if hover_tab {
                c.tab_inactive.gamma_multiply(1.1)
            } else {
                c.tab_inactive
            };
            tp.rect_filled(tr, 0.0, fill);
            if retro {
                // Karteireiter: links und oben hell, rechts Schatten, unten offen.
                let (l, t, r, b) = (tr.left() + 0.5, tr.top() + 0.5, tr.right() - 0.5, tr.bottom());
                tp.line_segment([pos2(l, b), pos2(l, t)], Stroke::new(1.0, theme::W98_WHITE));
                tp.line_segment([pos2(l, t), pos2(r, t)], Stroke::new(1.0, theme::W98_WHITE));
                tp.line_segment([pos2(r, t), pos2(r, b)], Stroke::new(1.0, theme::W98_DARK));
                tp.line_segment([pos2(r - 1.0, t + 1.0), pos2(r - 1.0, b)], Stroke::new(1.0, theme::W98_SHADOW));
            } else if is_active {
                // Akzent unten in der Farbe des Designs.
                tp.rect_filled(Rect::from_min_max(pos2(tr.left(), tr.bottom() - 2.0), tr.max), 0.0, c.accent);
            }
            let text_col = if is_active { c.text } else { c.tab_text };
            let mut tx = tr.left() + 10.0;
            if *read_only {
                let lock = Rect::from_min_size(pos2(tx - 2.0, tr.center().y - 8.0), vec2(16.0, 16.0));
                icons::paint_lock(&tp, lock, c.text_dim);
                tx += 16.0;
            }
            if *modified {
                tp.circle_filled(pos2(tx + 3.5, tr.center().y + 0.5), 3.5, if retro { text_col } else { c.accent });
                tx += 13.0;
            }
            tp.text(
                pos2(tx, tr.center().y),
                Align2::LEFT_CENTER,
                name,
                font.clone(),
                text_col,
            );
            let close = Rect::from_center_size(pos2(tr.right() - 12.0, tr.center().y), vec2(16.0, 16.0));
            let over_close = pointer.is_some_and(|p| close.contains(p));
            if hover_tab {
                if over_close {
                    tp.rect_filled(close, 2.0, Color32::from_rgb(0xD8, 0x50, 0x40));
                }
                let col = if over_close { Color32::WHITE } else { c.text_dim };
                if !icons::paint_glyph(&tp, close, icons::GLYPH_CLOSE, 9.0, col) {
                    let k = close.center();
                    let st = Stroke::new(1.3, col);
                    tp.line_segment([k + vec2(-3.5, -3.5), k + vec2(3.5, 3.5)], st);
                    tp.line_segment([k + vec2(3.5, -3.5), k + vec2(-3.5, 3.5)], st);
                }
            }
            if resp.clicked() {
                self.pending.push(if over_close { Action::CloseTab(pane, i) } else { Action::ActivateTab(pane, i) });
            }
            if resp.middle_clicked() {
                self.pending.push(Action::CloseTab(pane, i));
            }
            if resp.secondary_clicked() {
                self.pending.push(Action::ActivateTab(pane, i));
            }
            if resp.drag_started() {
                self.tab_drag = Some((pane, i));
            }
            if resp.dragged() {
                if let (Some((dp, di)), Some(p)) = (self.tab_drag, ui.input(|inp| inp.pointer.hover_pos())) {
                    if dp == pane {
                        // Mit dem Nachbarn tauschen, sobald die Maus über ihm steht.
                        let mut cx = rect.left() - scroll;
                        for (j, wj) in widths.iter().enumerate() {
                            let mid = cx + wj * 0.5;
                            if (j > di && p.x > mid && j == di + 1) || (j < di && p.x < mid && j + 1 == di) {
                                self.panes[pane].tabs.swap(di, j);
                                if self.panes[pane].active == di {
                                    self.panes[pane].active = j;
                                } else if self.panes[pane].active == j {
                                    self.panes[pane].active = di;
                                }
                                self.tab_drag = Some((pane, j));
                                break;
                            }
                            cx += wj + 2.0;
                        }
                    }
                }
            }
            if resp.drag_stopped() {
                self.tab_drag = None;
            }
            let pending = &mut self.pending;
            resp.context_menu(|ui| {
                ui.set_min_width(210.0);
                mi(ui, pending, "Schließen", "Strg+W", Action::CloseTab(pane, i));
                mi(ui, pending, "Alle anderen schließen", "", Action::CloseOthersOf(pane, i));
                ui.separator();
                mi(ui, pending, "Speichern", "Strg+S", Action::Save);
                mi(ui, pending, "Speichern unter …", "", Action::SaveAs);
                mi(ui, pending, "Umbenennen …", "", Action::Rename);
                mi(ui, pending, "Neu laden", "", Action::Reload);
                ui.separator();
                mi(ui, pending, "Vollständigen Pfad kopieren", "", Action::CopyFullPath);
                mi(ui, pending, "Dateinamen kopieren", "", Action::CopyFileName);
                mi(ui, pending, "Ordnerpfad kopieren", "", Action::CopyDirPath);
                ui.separator();
                mi(ui, pending, "In andere Ansicht verschieben", "", Action::MoveToOther);
                mi(ui, pending, "In anderer Ansicht öffnen", "", Action::CloneToOther);
            });
        }
        let free = Rect::from_min_max(pos2(x.max(bar.left()), rect.top()), bar.max);
        if free.width() > 4.0 && ui.interact(free, ui.id().with(("tabs_free", pane)), Sense::click()).double_clicked() {
            self.pending.push(Action::New);
        }
        if overflow {
            let bw = 22.0;
            let right = rect.right();
            for (k, label) in [(1.0_f32, "◀"), (0.0, "▶")] {
                let br = Rect::from_min_size(pos2(right - bw * (k + 1.0) - 1.0, rect.top() + 4.0), vec2(bw, rect.height() - 8.0));
                let r = ui.interact(br, ui.id().with(("tabscroll", pane, label)), Sense::click());
                painter.rect_filled(br, 0.0, if r.hovered() { c.hover } else { c.menu });
                painter.rect_stroke(br, 0.0, Stroke::new(1.0, c.line), StrokeKind::Inside);
                painter.text(br.center(), Align2::CENTER_CENTER, label, FontId::proportional(9.0), c.text);
                if r.clicked() {
                    self.panes[pane].scroll += if k > 0.5 { -120.0 } else { 120.0 };
                }
            }
        }
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        let width = ui.available_width();
        let narrow = self.menu_width + 70.0 > width;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let bar = egui::menu::bar(ui, |ui| {
                ui.spacing_mut().button_padding = vec2(9.0, 4.0);
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                self.menus(ui);
                if !narrow {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.window_buttons(ui));
                }
            });
            if !narrow {
                self.menu_width = self.measure_menus(ui.ctx());
            }
            let _ = bar;
            if narrow {
                self.menu_width = self.measure_menus(ui.ctx());
                ui.allocate_ui_with_layout(vec2(width, 20.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    flat(ui);
                    self.window_buttons(ui);
                });
            }
        });
    }

    fn measure_menus(&self, ctx: &Context) -> f32 {
        let names = ["Datei", "Bearbeiten", "Ansicht", "Suchen", "Dokument", "Werkzeuge", "Hilfe"];
        let font = FontId::proportional(if self.settings.theme.retro() { 13.0 } else { 14.0 });
        names
            .iter()
            .map(|n| ctx.fonts(|f| f.layout_no_wrap((*n).to_string(), font.clone(), Color32::WHITE).size().x) + 18.0)
            .sum()
    }

    /// Dokumentliste und Tab-schließen ganz rechts.
    fn window_buttons(&mut self, ui: &mut Ui) {
        let c = chrome(self.settings.theme);
        let (r, resp) = ui.allocate_exact_size(vec2(26.0, 20.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r, 0.0, c.hover);
        }
        let x_col = Color32::from_rgb(0x50, 0x70, 0xA0);
        if !icons::paint_glyph(ui.painter(), r, icons::GLYPH_CLOSE, 12.0, x_col) {
            let k = r.center();
            let st = Stroke::new(1.3, x_col);
            ui.painter().line_segment([k + vec2(-4.5, -4.5), k + vec2(4.5, 4.5)], st);
            ui.painter().line_segment([k + vec2(4.5, -4.5), k + vec2(-4.5, 4.5)], st);
        }
        if resp.on_hover_text("Aktuellen Tab schließen").clicked() {
            self.pending.push(Action::Close);
        }
        let names: Vec<(usize, usize, String)> = (0..2)
            .flat_map(|p| {
                self.panes[p]
                    .tabs
                    .iter()
                    .enumerate()
                    .map(move |(i, v)| (p, i, v.doc))
                    .collect::<Vec<_>>()
            })
            .map(|(p, i, id)| (p, i, self.docs[&id].name()))
            .collect();
        let pending = &mut self.pending;
        ui.menu_button(icons::rich(icons::GLYPH_CHEVRON_DOWN, "▼", 12.0), |ui| {
            ui.set_min_width(180.0);
            for (p, i, name) in &names {
                mi(ui, pending, name, "", Action::ActivateTab(*p, *i));
            }
        });
    }

    fn menus(&mut self, ui: &mut Ui) {
        let (o, doc_has_path, undo_ok, redo_ok, read_only, monitoring) = {
            let d = self.cur_doc();
            (
                self.settings.ed.clone(),
                d.path.is_some(),
                !d.undo.is_empty(),
                !d.redo.is_empty(),
                d.read_only,
                d.monitor,
            )
        };
        let recent = self.settings.recent.clone();
        let (cur_enc, cur_eol, cur_lang) = {
            let d = self.cur_doc();
            (d.enc, d.eol, d.hl.language.clone())
        };
        let doc_names: Vec<(usize, usize, String)> = (0..2)
            .flat_map(|p| self.panes[p].tabs.iter().enumerate().map(move |(i, v)| (p, i, v.doc)).collect::<Vec<_>>())
            .map(|(p, i, id)| (p, i, self.docs[&id].name()))
            .collect();
        let (show_map, show_funcs, show_folder, sync_v, sync_h, on_top, fullscreen, cur_theme) =
            (self.show_map, self.show_funcs, self.show_folder, self.sync_v, self.sync_h, self.on_top, self.fullscreen, self.settings.theme);
        let p = &mut self.pending;

        ui.menu_button("Datei", |ui| {
            ui.set_min_width(250.0);
            mi(ui, p, "Neu", "Strg+N", Action::New);
            mi(ui, p, "Öffnen …", "Strg+O", Action::Open);
            ui.menu_button("Zuletzt geöffnet", |ui| {
                ui.set_min_width(300.0);
                if recent.is_empty() {
                    ui.add_enabled(false, Button::new("(noch keine)"));
                }
                for r in &recent {
                    mi(ui, p, r, "", Action::OpenPath(r.into()));
                }
            });
            mi_en(ui, p, "Neu laden", "", doc_has_path, Action::Reload);
            ui.separator();
            mi(ui, p, "Speichern", "Strg+S", Action::Save);
            mi(ui, p, "Speichern unter …", "Strg+Alt+S", Action::SaveAs);
            mi(ui, p, "Kopie speichern unter …", "", Action::SaveCopy);
            mi(ui, p, "Alle speichern", "Strg+Umschalt+S", Action::SaveAll);
            mi(ui, p, "Umbenennen …", "", Action::Rename);
            ui.separator();
            mi(ui, p, "Schließen", "Strg+W", Action::Close);
            mi(ui, p, "Alle schließen", "Strg+Umschalt+W", Action::CloseAll);
            mi(ui, p, "Alle anderen schließen", "", Action::CloseOthers);
            ui.separator();
            let (files, shell) = if cfg!(windows) { ("Im Explorer zeigen", "Eingabeaufforderung hier öffnen") } else { ("Im Dateimanager zeigen", "Terminal hier öffnen") };
            mi_en(ui, p, files, "", doc_has_path, Action::OpenExplorer);
            mi_en(ui, p, shell, "", doc_has_path, Action::OpenTerminal);
            mi(ui, p, "Projektordner wählen …", "", Action::PickWorkspace);
            mi(ui, p, "Drucken …", "Strg+P", Action::Print);
            ui.separator();
            mi(ui, p, "Einstellungen …", "", Action::Preferences);
            ui.separator();
            mi(ui, p, "Beenden", "Alt+F4", Action::Exit);
        });

        ui.menu_button("Bearbeiten", |ui| {
            ui.set_min_width(250.0);
            mi_en(ui, p, "Rückgängig", "Strg+Z", undo_ok && !read_only, Action::Cmd(Cmd::Undo));
            mi_en(ui, p, "Wiederholen", "Strg+Y", redo_ok && !read_only, Action::Cmd(Cmd::Redo));
            ui.separator();
            mi(ui, p, "Ausschneiden", "Strg+X", Action::Cmd(Cmd::Cut));
            mi(ui, p, "Kopieren", "Strg+C", Action::Cmd(Cmd::Copy));
            mi(ui, p, "Einfügen", "Strg+V", Action::Paste);
            mi(ui, p, "Löschen", "Entf", Action::Cmd(Cmd::DeleteSelection));
            mi(ui, p, "Alles markieren", "Strg+A", Action::Cmd(Cmd::SelectAll));
            ui.separator();
            ui.menu_button("Zeilen", |ui| {
                ui.set_min_width(240.0);
                mi(ui, p, "Zeile duplizieren", "Strg+D", Action::Cmd(Cmd::DuplicateLines));
                mi(ui, p, "Zeile löschen", "Strg+L", Action::Cmd(Cmd::DeleteLines));
                mi(ui, p, "Zeile nach oben", "Strg+Umschalt+Hoch", Action::Cmd(Cmd::MoveLinesUp));
                mi(ui, p, "Zeile nach unten", "Strg+Umschalt+Runter", Action::Cmd(Cmd::MoveLinesDown));
                mi(ui, p, "Zeilen verbinden", "Strg+J", Action::Cmd(Cmd::JoinLines));
                ui.separator();
                mi(ui, p, "Aufsteigend sortieren", "", Action::Cmd(Cmd::SortLines { descending: false, ignore_case: false }));
                mi(ui, p, "Absteigend sortieren", "", Action::Cmd(Cmd::SortLines { descending: true, ignore_case: false }));
                mi(ui, p, "Aufsteigend (ohne Groß/Klein)", "", Action::Cmd(Cmd::SortLines { descending: false, ignore_case: true }));
                ui.separator();
                mi(ui, p, "Doppelte Zeilen entfernen", "", Action::Cmd(Cmd::DedupLines));
                mi(ui, p, "Leere Zeilen entfernen", "", Action::Cmd(Cmd::RemoveEmptyLines { blank_too: false }));
                mi(ui, p, "Leere Zeilen und Leerzeilen mit Leerraum entfernen", "", Action::Cmd(Cmd::RemoveEmptyLines { blank_too: true }));
            });
            ui.menu_button("Groß-/Kleinschreibung", |ui| {
                mi(ui, p, "GROSSBUCHSTABEN", "Strg+Umschalt+U", Action::Cmd(Cmd::Case(CaseKind::Upper)));
                mi(ui, p, "Kleinbuchstaben", "Strg+U", Action::Cmd(Cmd::Case(CaseKind::Lower)));
                mi(ui, p, "Wortanfänge Groß", "", Action::Cmd(Cmd::Case(CaseKind::Proper)));
                mi(ui, p, "iNVERTIEREN", "", Action::Cmd(Cmd::Case(CaseKind::Invert)));
            });
            ui.menu_button("Einzug", |ui| {
                mi(ui, p, "Einzug vergrößern", "Tab", Action::Cmd(Cmd::Indent));
                mi(ui, p, "Einzug verkleinern", "Umschalt+Tab", Action::Cmd(Cmd::Outdent));
            });
            ui.menu_button("Leerraum", |ui| {
                mi(ui, p, "Leerraum am Zeilenende entfernen", "", Action::Cmd(Cmd::TrimTrailing));
                mi(ui, p, "Leerraum am Zeilenanfang entfernen", "", Action::Cmd(Cmd::TrimLeading));
                mi(ui, p, "Tabs in Leerzeichen", "", Action::Cmd(Cmd::TabsToSpaces));
                mi(ui, p, "Führende Leerzeichen in Tabs", "", Action::Cmd(Cmd::LeadingSpacesToTabs));
            });
            ui.menu_button("Kommentar", |ui| {
                ui.set_min_width(240.0);
                mi(ui, p, "Zeilenkommentar umschalten", "Strg+Q", Action::Cmd(Cmd::Comment(None)));
                mi(ui, p, "Zeile kommentieren", "Strg+K", Action::Cmd(Cmd::Comment(Some(true))));
                mi(ui, p, "Kommentar entfernen", "Strg+Umschalt+K", Action::Cmd(Cmd::Comment(Some(false))));
                mi(ui, p, "Blockkommentar", "Strg+Umschalt+Q", Action::Cmd(Cmd::BlockComment));
            });
            ui.menu_button("In Zwischenablage", |ui| {
                mi(ui, p, "Vollständiger Dateipfad", "", Action::CopyFullPath);
                mi(ui, p, "Dateiname", "", Action::CopyFileName);
                mi(ui, p, "Ordnerpfad", "", Action::CopyDirPath);
            });
            ui.separator();
            mi(ui, p, "Wort vervollständigen", "Strg+Leertaste", Action::Autocomplete);
            mi(ui, p, "Zahlenfolge einfügen …", "Alt+C", Action::ColumnEditor);
            mi(ui, p, "Datum und Uhrzeit einfügen", "", Action::InsertDateTime);
        });

        ui.menu_button("Ansicht", |ui| {
            ui.set_min_width(250.0);
            ui.menu_button("Design", |ui| {
                ui.set_min_width(170.0);
                for t in ThemeId::ALL {
                    check(ui, p, t.label(), cur_theme == t, "", Action::SetTheme(t));
                }
            });
            ui.menu_button("Zoom", |ui| {
                mi(ui, p, "Vergrößern", "Strg+Plus", Action::ZoomIn);
                mi(ui, p, "Verkleinern", "Strg+Minus", Action::ZoomOut);
                mi(ui, p, "Normale Größe", "Strg+0", Action::ZoomReset);
            });
            check(ui, p, "Vollbild", fullscreen, "F11", Action::ToggleFullscreen);
            check(ui, p, "Immer im Vordergrund", on_top, "", Action::ToggleOnTop);
            ui.separator();
            check(ui, p, "Zeilenumbruch", o.wrap, "", Action::ToggleWrap);
            check(ui, p, "Zeilennummern", o.line_numbers, "", Action::ToggleLineNumbers);
            check(ui, p, "Einzugslinien", o.indent_guides, "", Action::ToggleGuides);
            ui.menu_button("Unsichtbare Zeichen", |ui| {
                check(ui, p, "Leerzeichen und Tabs", o.show_ws, "", Action::ToggleWs);
                check(ui, p, "Zeilenenden", o.show_eol, "", Action::ToggleEol);
                check(ui, p, "Alle", o.show_ws && o.show_eol, "", Action::ToggleAllSymbols);
            });
            ui.menu_button("Einklappen", |ui| {
                ui.set_min_width(240.0);
                mi(ui, p, "Aktuelle Ebene umschalten", "Strg+Alt+F", Action::FoldCurrent);
                mi(ui, p, "Alles einklappen", "Alt+0", Action::FoldAll);
                mi(ui, p, "Alles ausklappen", "Alt+Umschalt+0", Action::UnfoldAll);
            });
            ui.separator();
            check(ui, p, "Übersicht", show_map, "", Action::ToggleMap);
            check(ui, p, "Gliederung", show_funcs, "", Action::ToggleFuncs);
            check(ui, p, "Projektordner", show_folder, "", Action::ToggleFolder);
            ui.separator();
            ui.menu_button("Geteilte Ansicht", |ui| {
                ui.set_min_width(230.0);
                mi(ui, p, "Tab auf die andere Seite", "", Action::MoveToOther);
                mi(ui, p, "Tab auf beiden Seiten zeigen", "", Action::CloneToOther);
                ui.separator();
                check(ui, p, "Senkrecht gemeinsam scrollen", sync_v, "", Action::ToggleSyncV);
                check(ui, p, "Waagerecht gemeinsam scrollen", sync_h, "", Action::ToggleSyncH);
            });
            ui.menu_button("Tabs", |ui| {
                ui.set_min_width(190.0);
                mi(ui, p, "Nächster Tab", "Strg+Tab", Action::NextTab);
                mi(ui, p, "Vorheriger Tab", "Strg+Umschalt+Tab", Action::PrevTab);
                ui.separator();
                for (pane, i, name) in &doc_names {
                    mi(ui, p, name, "", Action::ActivateTab(*pane, *i));
                }
            });
        });

        ui.menu_button("Suchen", |ui| {
            ui.set_min_width(250.0);
            mi(ui, p, "Suchen …", "Strg+F", Action::ShowFind(FindTab::Find));
            mi(ui, p, "Ersetzen …", "Strg+H", Action::ShowFind(FindTab::Replace));
            mi(ui, p, "Weitersuchen", "F3", Action::FindNext);
            mi(ui, p, "Rückwärts suchen", "Umschalt+F3", Action::FindPrev);
            mi(ui, p, "Markierten Text suchen", "Strg+F3", Action::FindSelectedNext);
            mi(ui, p, "In Dateien suchen …", "Strg+Umschalt+F", Action::ShowFind(FindTab::Files));
            ui.separator();
            mi(ui, p, "Treffer hervorheben …", "Strg+M", Action::ShowFind(FindTab::Mark));
            mi(ui, p, "Hervorhebungen entfernen", "", Action::ClearMarks);
            ui.menu_button("Lesezeichen", |ui| {
                ui.set_min_width(230.0);
                mi(ui, p, "Setzen/entfernen", "Strg+F2", Action::BookmarkToggle);
                mi(ui, p, "Nächstes", "F2", Action::BookmarkNext);
                mi(ui, p, "Vorheriges", "Umschalt+F2", Action::BookmarkPrev);
                mi(ui, p, "Alle entfernen", "", Action::BookmarksClear);
                mi(ui, p, "Umkehren", "", Action::BookmarksInvert);
                ui.separator();
                mi(ui, p, "Zeilen mit Lesezeichen kopieren", "", Action::BookmarksCopyLines);
                mi(ui, p, "Zeilen mit Lesezeichen löschen", "", Action::BookmarksDeleteLines);
            });
            ui.separator();
            mi(ui, p, "Gehe zu …", "Strg+G", Action::GoToDialog);
            mi(ui, p, "Zur passenden Klammer", "Strg+B", Action::MatchBrace);
        });

        ui.menu_button("Dokument", |ui| {
            ui.set_min_width(230.0);
            ui.menu_button("Sprache", |ui| language_menu(ui, p, cur_lang.as_deref()));
            ui.menu_button("Kodierung", |ui| encoding_menu(ui, p, cur_enc));
            ui.menu_button("Umwandeln in", |ui| {
                ui.set_min_width(200.0);
                for e in [Enc::Utf8, Enc::Utf8Bom, Enc::Utf16Le, Enc::Utf16Be, Enc::Ansi] {
                    mi(ui, p, e.label().as_str(), "", Action::Convert(e));
                }
            });
            ui.menu_button("Zeilenende", |ui| eol_menu(ui, p, cur_eol));
            ui.separator();
            check(ui, p, "Schreibschutz", read_only, "", Action::ToggleReadOnly);
            check(ui, p, "Änderungen fortlaufend anzeigen", monitoring, "", Action::ToggleMonitor);
            ui.separator();
            mi(ui, p, "Dokumentinfo …", "", Action::Summary);
        });

        ui.menu_button("Werkzeuge", |ui| {
            ui.set_min_width(180.0);
            ui.menu_button("Prüfsumme", |ui| {
                ui.set_min_width(160.0);
                ui.label(RichText::new("von Markierung oder Text").small().weak());
                mi(ui, p, "MD5", "", Action::Hash(HashKind::Md5));
                mi(ui, p, "SHA-1", "", Action::Hash(HashKind::Sha1));
                mi(ui, p, "SHA-256", "", Action::Hash(HashKind::Sha256));
                mi(ui, p, "SHA-512", "", Action::Hash(HashKind::Sha512));
            });
        });

        ui.menu_button("Hilfe", |ui| {
            ui.set_min_width(170.0);
            mi(ui, p, "Tastenkürzel …", "", Action::Shortcuts);
            mi(ui, p, "Über Quillpad …", "", Action::About);
        });
    }
}

/// Auswahl der Sprache, nach Anfangsbuchstaben gruppiert. Für Menü und Statuszeile.
fn language_menu(ui: &mut Ui, p: &mut Vec<Action>, cur: Option<&str>) {
    ui.set_min_width(170.0);
    check(ui, p, "Nur Text", cur.is_none(), "", Action::SetLanguage(None));
    ui.separator();
    let names = &highlight::engine().names;
    let mut letters: Vec<char> = names.iter().filter_map(|n| n.chars().next()).map(|c| c.to_ascii_uppercase()).collect();
    letters.dedup();
    for l in letters {
        ui.menu_button(l.to_string(), |ui| {
            ui.set_min_width(170.0);
            for n in names.iter().filter(|n| n.chars().next().map(|c| c.to_ascii_uppercase()) == Some(l)) {
                check(ui, p, n, cur == Some(n.as_str()), "", Action::SetLanguage(Some(n.clone())));
            }
        });
    }
}

/// Kodierung, mit der die Datei gelesen wird. Für Menü und Statuszeile.
fn encoding_menu(ui: &mut Ui, p: &mut Vec<Action>, cur: Enc) {
    ui.set_min_width(200.0);
    ui.label(RichText::new("Datei neu einlesen als").small().weak());
    for e in [Enc::Utf8, Enc::Utf8Bom, Enc::Utf16Le, Enc::Utf16Be, Enc::Ansi] {
        check(ui, p, e.label().as_str(), cur == e, "", Action::Encode(e));
    }
    ui.menu_button("Weitere Zeichensätze", |ui| {
        let list = fileio::legacy_list();
        let mut groups: Vec<&str> = list.iter().map(|x| x.0).collect();
        groups.dedup();
        for g in groups {
            ui.menu_button(g, |ui| {
                for (_, name, enc) in list.iter().filter(|x| x.0 == g) {
                    let e = Enc::Legacy(enc);
                    check(ui, p, name, cur == e, "", Action::Encode(e));
                }
            });
        }
    });
}

/// Zeilenende des Dokuments. Für Menü und Statuszeile.
fn eol_menu(ui: &mut Ui, p: &mut Vec<Action>, cur: Eol) {
    ui.set_min_width(170.0);
    for e in [Eol::Crlf, Eol::Lf, Eol::Cr] {
        check(ui, p, e.label(), cur == e, "", Action::SetEol(e));
    }
}

impl App {
    fn toolbar(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let (modified, undo_ok, redo_ok, has_sel, any_modified, read_only, monitor) = {
            let d = self.cur_doc();
            (
                d.is_modified(),
                !d.undo.is_empty(),
                !d.redo.is_empty(),
                self.cur_view().sels.sels.iter().any(|s| !s.is_empty()),
                self.docs.values().any(|x| x.is_modified()),
                d.read_only,
                d.monitor,
            )
        };
        let o = self.settings.ed.clone();
        let theme_id = self.settings.theme;
        let c = chrome(theme_id);
        let (dark, retro) = (theme_id.is_dark(), theme_id.retro());
        let split = self.is_split();
        let (sync_v, sync_h, show_map, show_funcs, show_folder) =
            (self.sync_v, self.sync_h, self.show_map, self.show_funcs, self.show_folder);
        let p = &mut self.pending;
        let sep = |ui: &mut Ui| {
            let (r, _) = ui.allocate_exact_size(vec2(8.0, 22.0), Sense::hover());
            let x = r.center().x.round() - 0.5;
            ui.painter().line_segment([pos2(x, r.top() + 2.0), pos2(x, r.bottom() - 2.0)], Stroke::new(1.0, c.line));
            if retro {
                // Eingeprägte Trennlinie: Schatten und Licht nebeneinander.
                ui.painter().line_segment([pos2(x + 1.0, r.top() + 2.0), pos2(x + 1.0, r.bottom() - 2.0)], Stroke::new(1.0, theme::W98_WHITE));
            }
        };
        let tool = |ui: &mut Ui, icon: Icon, tip: &str, enabled: bool, toggled: bool, action: Action, p: &mut Vec<Action>| {
            let (r, resp) = ui.allocate_exact_size(vec2(27.0, 26.0), if enabled { Sense::click() } else { Sense::hover() });
            let pressed = resp.is_pointer_button_down_on() && enabled;
            let mut icon_r = Rect::from_center_size(r.center(), vec2(18.0, 18.0));
            if retro {
                // Flache Knöpfe: erhaben beim Überfahren, vertieft wenn gedrückt oder eingeschaltet.
                if toggled || pressed {
                    ui.painter().rect_filled(r.shrink(1.0), 0.0, theme::W98_LIGHT);
                    theme::bevel_thin(ui.painter(), r.shrink(1.0), false);
                    icon_r = icon_r.translate(vec2(1.0, 1.0));
                } else if resp.hovered() && enabled {
                    theme::bevel_thin(ui.painter(), r.shrink(1.0), true);
                }
            } else if toggled {
                ui.painter().rect(r.shrink(1.0), 0.0, c.hover, Stroke::new(1.0, c.hover_line), StrokeKind::Inside);
            } else if resp.hovered() && enabled {
                ui.painter().rect(r.shrink(1.0), 0.0, c.hover, Stroke::new(1.0, c.hover_line), StrokeKind::Inside);
            }
            icons::paint(ui.painter(), icon_r, icon, enabled, dark, retro);
            if resp.on_hover_text(tip).clicked() {
                p.push(action);
            }
        };
        ui.horizontal_centered(|ui| {
            tool(ui, Icon::New, "Neu (Strg+N)", true, false, Action::New, p);
            tool(ui, Icon::Open, "Öffnen (Strg+O)", true, false, Action::Open, p);
            tool(ui, Icon::Save, "Speichern (Strg+S)", modified, false, Action::Save, p);
            tool(ui, Icon::SaveAll, "Alle speichern", any_modified, false, Action::SaveAll, p);
            tool(ui, Icon::Close, "Schließen (Strg+W)", true, false, Action::Close, p);
            tool(ui, Icon::CloseAll, "Alle schließen", true, false, Action::CloseAll, p);
            tool(ui, Icon::Print, "Drucken (Strg+P)", true, false, Action::Print, p);
            sep(ui);
            tool(ui, Icon::Cut, "Ausschneiden", has_sel && !read_only, false, Action::Cmd(Cmd::Cut), p);
            tool(ui, Icon::Copy, "Kopieren", has_sel, false, Action::Cmd(Cmd::Copy), p);
            tool(ui, Icon::Paste, "Einfügen", !read_only, false, Action::Paste, p);
            sep(ui);
            tool(ui, Icon::Undo, "Rückgängig (Strg+Z)", undo_ok && !read_only, false, Action::Cmd(Cmd::Undo), p);
            tool(ui, Icon::Redo, "Wiederholen (Strg+Y)", redo_ok && !read_only, false, Action::Cmd(Cmd::Redo), p);
            sep(ui);
            tool(ui, Icon::Find, "Suchen (Strg+F)", true, false, Action::ShowFind(FindTab::Find), p);
            tool(ui, Icon::Replace, "Ersetzen (Strg+H)", true, false, Action::ShowFind(FindTab::Replace), p);
            sep(ui);
            tool(ui, Icon::ZoomIn, "Vergrößern", true, false, Action::ZoomIn, p);
            tool(ui, Icon::ZoomOut, "Verkleinern", true, false, Action::ZoomOut, p);
            sep(ui);
            tool(ui, Icon::SyncV, "Senkrecht gemeinsam scrollen", split, sync_v, Action::ToggleSyncV, p);
            tool(ui, Icon::SyncH, "Waagerecht gemeinsam scrollen", split, sync_h, Action::ToggleSyncH, p);
            sep(ui);
            tool(ui, Icon::Wrap, "Zeilenumbruch", true, o.wrap, Action::ToggleWrap, p);
            tool(ui, Icon::ShowAll, "Alle Zeichen anzeigen", true, o.show_ws && o.show_eol, Action::ToggleAllSymbols, p);
            ui.spacing_mut().button_padding = vec2(2.0, 2.0);
            flat(ui);
            ui.menu_button(icons::rich(icons::GLYPH_CHEVRON_DOWN, "▾", 9.0), |ui| {
                ui.set_min_width(180.0);
                check(ui, p, "Leerzeichen und Tabs", o.show_ws, "", Action::ToggleWs);
                check(ui, p, "Zeilenenden", o.show_eol, "", Action::ToggleEol);
                check(ui, p, "Alle Zeichen", o.show_ws && o.show_eol, "", Action::ToggleAllSymbols);
            });
            tool(ui, Icon::Guides, "Einzugslinien", true, o.indent_guides, Action::ToggleGuides, p);
            sep(ui);
            tool(ui, Icon::Split, "Tab auf beiden Seiten zeigen", true, split, Action::CloneToOther, p);
            tool(ui, Icon::Map, "Übersicht", true, show_map, Action::ToggleMap, p);
            tool(ui, Icon::Funcs, "Gliederung", true, show_funcs, Action::ToggleFuncs, p);
            tool(ui, Icon::Folder, "Projektordner", true, show_folder, Action::ToggleFolder, p);
            sep(ui);
            tool(ui, Icon::Monitor, "Änderungen fortlaufend anzeigen", true, monitor, Action::ToggleMonitor, p);
        });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        let theme_id = self.settings.theme;
        let c = chrome(theme_id);
        let retro = theme_id.retro();
        let rect = ui.max_rect();
        if !retro {
            ui.painter().line_segment([pos2(rect.left(), rect.top()), pos2(rect.right(), rect.top())], Stroke::new(1.0, c.line));
        }
        let (d, v) = (self.cur_doc(), self.cur_view());
        let main = v.sels.main();
        let line = d.text.char_to_line(main.head);
        let col = main.head - d.line_start(line) + 1;
        let sel_chars: usize = v.sels.sels.iter().map(|s| s.end() - s.start()).sum();
        let mut pos = format!("Zeile {}, Spalte {}", group(line + 1), group(col));
        if sel_chars > 0 {
            pos.push_str(&format!("   ({} markiert)", group(sel_chars)));
        }
        let cur_lang = d.hl.language.clone();
        let lang = cur_lang.clone().unwrap_or_else(|| "Nur Text".to_string());
        let counts = format!("{} Zeichen, {} Zeilen", group(d.len()), group(d.lines()));
        let (cur_eol, cur_enc) = (d.eol, d.enc);
        let mode = if v.overwrite { "ÜB" } else { "EINF" };
        let size = if retro { 13.0 } else { 14.0 };
        let txt = |s: &str| RichText::new(s).size(size).color(c.text);

        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink2(vec2(8.0, 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let ui = &mut child;
        flat(ui);
        ui.spacing_mut().item_spacing = vec2(if retro { 8.0 } else { 0.0 }, 0.0);
        ui.spacing_mut().button_padding = vec2(4.0, 1.0);
        // Klassisch steht jedes Feld vertieft für sich, sonst trennt ein Punkt.
        let frame = |ui: &Ui, r: Rect| {
            if retro {
                let f = Rect::from_min_max(pos2(r.left() - 3.0, rect.top() + 3.0), pos2(r.right() + 3.0, rect.bottom() - 2.0));
                theme::bevel_thin(ui.painter(), Rect::from_min_max(f.min.round(), f.max.round()), false);
            }
        };
        let dot = |ui: &mut Ui| {
            if !retro {
                ui.add_space(6.0);
                ui.label(RichText::new("·").size(size).color(c.text_dim));
                ui.add_space(6.0);
            }
        };
        let r = ui.label(txt(&pos)).rect;
        frame(ui, r);
        let pending = &mut self.pending;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Reine Textfelder bekommen denselben Innenabstand wie die Knöpfe daneben.
            ui.add_space(4.0);
            let r = ui.label(txt(mode)).on_hover_text(if mode == "ÜB" { "Überschreiben (Einfg)" } else { "Einfügen (Einfg)" }).rect;
            ui.add_space(4.0);
            frame(ui, r);
            dot(ui);
            ui.add_space(4.0);
            let r = ui.label(txt(&counts)).rect;
            ui.add_space(4.0);
            frame(ui, r);
            dot(ui);
            let r = ui
                .menu_button(txt(cur_eol.short()), |ui| eol_menu(ui, pending, cur_eol))
                .response
                .on_hover_text(format!("Zeilenende: {}", cur_eol.label()))
                .rect;
            frame(ui, r);
            dot(ui);
            let r = ui
                .menu_button(txt(&cur_enc.label()), |ui| {
                    encoding_menu(ui, pending, cur_enc);
                    ui.separator();
                    ui.menu_button("Umwandeln in", |ui| {
                        ui.set_min_width(200.0);
                        for e in [Enc::Utf8, Enc::Utf8Bom, Enc::Utf16Le, Enc::Utf16Be, Enc::Ansi] {
                            mi(ui, pending, e.label().as_str(), "", Action::Convert(e));
                        }
                    });
                })
                .response
                .on_hover_text("Kodierung")
                .rect;
            frame(ui, r);
            dot(ui);
            let r = ui
                .menu_button(txt(&lang), |ui| language_menu(ui, pending, cur_lang.as_deref()))
                .response
                .on_hover_text("Sprache für die Farben")
                .rect;
            frame(ui, r);
        });
    }
}

/// Zahl mit Tausenderpunkten, z. B. 12.345.
fn group(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn numbers_get_thousand_dots() {
        assert_eq!(super::group(0), "0");
        assert_eq!(super::group(999), "999");
        assert_eq!(super::group(1000), "1.000");
        assert_eq!(super::group(1234567), "1.234.567");
    }
}
