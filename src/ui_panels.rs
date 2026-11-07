//! Ergebnisliste, Seitenleisten und alle Dialoge.
use crate::app::{Action, App, Dialog, APP_NAME};
use crate::doc::{DocId, SelSet};
use crate::fileio::Eol;
use crate::highlight;
use crate::t;
use crate::ui_chrome::{self, chrome};
use egui::{pos2, vec2, Align2, Color32, Context, FontFamily, FontId, RichText, Sense, Stroke, StrokeKind, Ui};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub line: usize,
    pub col: usize,
    pub len: usize,
    pub text: String,
}

pub enum ResRow {
    Search { title: String, summary: String, collapsed: bool },
    File { name: String, count: usize, collapsed: bool },
    Hit { doc: Option<DocId>, path: Option<PathBuf>, hit: Hit },
}

#[derive(Default)]
pub struct Results {
    pub open: bool,
    pub rows: Vec<ResRow>,
    pub visible: Vec<usize>,
    pub dirty: bool,
    current: Option<usize>,
}

const MAX_ROWS: usize = 500_000;

pub fn files_label(n: usize) -> String {
    if n == 1 {
        t!("results.files_one")
    } else {
        t!("results.files_many", n = n)
    }
}

pub fn docs_label(n: usize) -> String {
    if n == 1 {
        t!("results.docs_one")
    } else {
        t!("results.docs_many", n = n)
    }
}

impl Results {
    pub fn begin(&mut self, title: String) {
        self.open = true;
        // Neuere Suchen stehen oben, ältere bleiben eingeklappt erhalten.
        for r in self.rows.iter_mut() {
            if let ResRow::Search { collapsed, .. } = r {
                *collapsed = true;
            }
        }
        self.rows.insert(0, ResRow::Search { title, summary: t!("results.running"), collapsed: false });
        self.current = Some(0);
        self.dirty = true;
        if self.rows.len() > MAX_ROWS {
            self.rows.truncate(MAX_ROWS);
        }
    }

    pub fn add_file(&mut self, doc: Option<DocId>, path: Option<PathBuf>, name: String, hits: Vec<Hit>) {
        let mut at = 1;
        while at < self.rows.len() && !matches!(self.rows[at], ResRow::Search { .. }) {
            at += 1;
        }
        let count = hits.len();
        let mut new: Vec<ResRow> = Vec::with_capacity(count + 1);
        new.push(ResRow::File { name, count, collapsed: false });
        for hit in hits {
            new.push(ResRow::Hit { doc, path: path.clone(), hit });
        }
        self.rows.splice(at..at, new);
        self.dirty = true;
    }

    pub fn finish(&mut self, hits: usize, files: usize) {
        if let Some(ResRow::Search { summary, .. }) = self.rows.get_mut(0) {
            *summary = t!("results.summary", hits = hits, files = files_label(files));
        }
        self.dirty = true;
    }

    fn rebuild(&mut self) {
        self.visible.clear();
        let (mut hide_all, mut hide_hits) = (false, false);
        for (i, r) in self.rows.iter().enumerate() {
            match r {
                ResRow::Search { collapsed, .. } => {
                    hide_all = *collapsed;
                    hide_hits = false;
                    self.visible.push(i);
                }
                ResRow::File { collapsed, .. } => {
                    hide_hits = *collapsed;
                    if !hide_all {
                        self.visible.push(i);
                    }
                }
                ResRow::Hit { .. } => {
                    if !hide_all && !hide_hits {
                        self.visible.push(i);
                    }
                }
            }
        }
        self.dirty = false;
    }
}

fn toggle_label(ui: &mut Ui, collapsed: bool, text: RichText) -> bool {
    let arrow = if collapsed {
        crate::icons::rich(crate::icons::GLYPH_CHEVRON_RIGHT, "▶", 9.0)
    } else {
        crate::icons::rich(crate::icons::GLYPH_CHEVRON_DOWN, "▼", 9.0)
    };
    ui.add(egui::Label::new(arrow).sense(Sense::click())).clicked()
        | ui.add(egui::Label::new(text).sense(Sense::click())).clicked()
}

impl App {
    pub fn results_panel(&mut self, ctx: &Context) {
        if !self.results.open {
            return;
        }
        let c = chrome(self.settings.theme);
        let mut jump: Option<(Option<DocId>, Option<PathBuf>, usize, usize, usize)> = None;
        egui::TopBottomPanel::bottom("results")
            .resizable(true)
            .default_height(170.0)
            .min_height(60.0)
            .frame(egui::Frame::NONE.fill(c.window).inner_margin(4))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t!("results.title")).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✖").on_hover_text(t!("panel.close")).clicked() {
                            self.results.open = false;
                        }
                        if ui.small_button(t!("results.clear")).clicked() {
                            self.results.rows.clear();
                            self.results.dirty = true;
                        }
                    });
                });
                if self.results.dirty {
                    self.results.rebuild();
                }
                let row_h = 19.0;
                let pal = self.palette();
                let total = self.results.visible.len();
                egui::Frame::NONE.fill(pal.bg).show(ui, |ui| {
                    egui::ScrollArea::both().auto_shrink([false, false]).show_rows(ui, row_h, total, |ui, range| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                        for vi in range {
                            let idx = self.results.visible[vi];
                            ui.horizontal(|ui| {
                                ui.set_height(row_h);
                                match &mut self.results.rows[idx] {
                                    ResRow::Search { title, summary, collapsed } => {
                                        let label = RichText::new(format!("{title} ({summary})")).strong().color(Color32::from_rgb(0x30, 0x60, 0xA0));
                                        if toggle_label(ui, *collapsed, label) {
                                            *collapsed = !*collapsed;
                                            self.results.dirty = true;
                                        }
                                    }
                                    ResRow::File { name, count, collapsed } => {
                                        ui.add_space(12.0);
                                        let label = RichText::new(t!("results.file_hits", name = name, count = count)).color(Color32::from_rgb(0x20, 0x80, 0x40));
                                        if toggle_label(ui, *collapsed, label) {
                                            *collapsed = !*collapsed;
                                            self.results.dirty = true;
                                        }
                                    }
                                    ResRow::Hit { doc, path, hit } => {
                                        ui.add_space(28.0);
                                        let mono = FontId::new(13.0, FontFamily::Name(crate::editor::EDITOR_FAMILY.into()));
                                        let chars: Vec<char> = hit.text.chars().collect();
                                        let a = hit.col.min(chars.len());
                                        let b = (hit.col + hit.len).min(chars.len());
                                        let mut job = egui::text::LayoutJob::default();
                                        let fmt = |color: Color32, bg: Color32| egui::TextFormat { font_id: mono.clone(), color, background: bg, ..Default::default() };
                                        job.append(&format!("{}  ", t!("results.line", line = hit.line + 1)), 0.0, fmt(pal.gutter_fg, Color32::TRANSPARENT));
                                        job.append(&chars[..a].iter().collect::<String>(), 0.0, fmt(pal.fg, Color32::TRANSPARENT));
                                        job.append(&chars[a..b].iter().collect::<String>(), 0.0, fmt(pal.fg, pal.smart_bg));
                                        job.append(&chars[b..].iter().collect::<String>(), 0.0, fmt(pal.fg, Color32::TRANSPARENT));
                                        let r = ui.add(egui::Label::new(job).sense(Sense::click()).wrap_mode(egui::TextWrapMode::Extend));
                                        if r.clicked() {
                                            jump = Some((*doc, path.clone(), hit.line, hit.col, hit.len));
                                        }
                                    }
                                }
                            });
                        }
                    });
                });
            });
        if let Some((doc, path, line, col, len)) = jump {
            self.goto_hit(doc, path, line, col, len);
        }
    }

    pub fn goto_hit(&mut self, doc: Option<DocId>, path: Option<PathBuf>, line: usize, col: usize, len: usize) {
        let ok = match doc.filter(|d| self.docs.contains_key(d)) {
            Some(d) => self.activate_doc(d),
            None => path.and_then(|p| self.open_path(&p)).is_some(),
        };
        if !ok {
            return;
        }
        let d = self.cur_doc();
        let line = line.min(d.lines() - 1);
        let n = d.line_len(line);
        let a = d.line_start(line) + col.min(n);
        let b = (a + len).min(d.len());
        self.select_range(a, b);
        self.focus_editor();
    }

    pub fn side_panels(&mut self, ctx: &Context) {
        let c = chrome(self.settings.theme);
        if self.show_folder {
            let root = self.settings.workspace.clone();
            let mut close = false;
            egui::SidePanel::left("folder")
                .default_width(210.0)
                .min_width(120.0)
                .frame(egui::Frame::NONE.fill(c.window).inner_margin(4))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(t!("panel.folder")).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✖").on_hover_text(t!("panel.close")).clicked() {
                                close = true;
                            }
                            if ui.small_button("⟳").on_hover_text(t!("panel.rescan")).clicked() {
                                self.folder_cache.clear();
                            }
                            if ui.small_button("…").on_hover_text(t!("panel.pick_folder")).clicked() {
                                self.pending.push(Action::PickWorkspace);
                            }
                        });
                    });
                    ui.separator();
                    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| match root {
                        Some(r) => self.folder_ui(ui, &PathBuf::from(&r), true),
                        None => {
                            ui.label(t!("panel.no_folder"));
                        }
                    });
                });
            if close {
                self.show_folder = false;
            }
        }
        if self.show_map {
            egui::SidePanel::right("docmap")
                .default_width(110.0)
                .min_width(60.0)
                .max_width(240.0)
                .frame(egui::Frame::NONE.fill(c.window).inner_margin(2))
                .show(ctx, |ui| self.doc_map(ui));
        }
        if self.show_funcs {
            let mut close = false;
            egui::SidePanel::right("funcs")
                .default_width(190.0)
                .min_width(110.0)
                .frame(egui::Frame::NONE.fill(c.window).inner_margin(4))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(t!("panel.outline")).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✖").clicked() {
                                close = true;
                            }
                        });
                    });
                    ui.separator();
                    self.func_list(ui);
                });
            if close {
                self.show_funcs = false;
            }
        }
    }

    fn folder_ui(&mut self, ui: &mut Ui, dir: &std::path::Path, top: bool) {
        let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dir.display().to_string());
        let dir_buf = dir.to_path_buf();
        egui::CollapsingHeader::new(name).id_salt(&dir_buf).default_open(top).show(ui, |ui| {
            if !self.folder_cache.contains_key(&dir_buf) {
                let mut list: Vec<(String, bool, PathBuf)> = std::fs::read_dir(&dir_buf)
                    .map(|rd| {
                        rd.flatten()
                            .map(|e| {
                                let p = e.path();
                                (e.file_name().to_string_lossy().into_owned(), p.is_dir(), p)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                list.sort_by_key(|(n, is_dir, _)| (!*is_dir, n.to_lowercase()));
                self.folder_cache.insert(dir_buf.clone(), list);
            }
            let entries = self.folder_cache.get(&dir_buf).cloned().unwrap_or_default();
            for (n, is_dir, p) in entries {
                if is_dir {
                    self.folder_ui(ui, &p, false);
                } else if ui.selectable_label(false, n).clicked() {
                    self.pending.push(Action::OpenPath(p));
                }
            }
        });
    }

    fn func_list(&mut self, ui: &mut Ui) {
        let id = self.cur_id();
        let d = self.cur_doc();
        let (rev, lang, idle) = (d.rev, d.hl.language.clone(), d.last_edit.elapsed().as_millis() > 300);
        let stale = self.func_cache.as_ref().map(|c| (c.0, c.1)) != Some((id, rev));
        if stale && (idle || self.func_cache.as_ref().map(|c| c.0) != Some(id)) {
            let pats: Vec<regex::Regex> = highlight::function_patterns(lang.as_deref())
                .into_iter()
                .filter_map(|p| regex::RegexBuilder::new(p).multi_line(true).build().ok())
                .collect();
            let text = self.cur_doc().full_text();
            let mut out: Vec<(usize, String)> = Vec::new();
            for re in &pats {
                for c in re.captures_iter(&text) {
                    if let Some(m) = c.get(1) {
                        out.push((m.start(), m.as_str().trim().to_string()));
                    }
                }
            }
            out.sort();
            out.dedup();
            let d = self.cur_doc();
            let list = out.into_iter().map(|(b, name)| (name, d.text.byte_to_line(b))).collect();
            self.func_cache = Some((id, rev, list));
        }
        let Some((_, _, list)) = self.func_cache.clone() else { return };
        if list.is_empty() {
            ui.label(if lang.is_some() { t!("panel.no_functions") } else { t!("panel.no_functions_plain") });
            return;
        }
        let mut goto = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (name, line) in &list {
                if ui.selectable_label(false, name).on_hover_text(t!("panel.line", line = line + 1)).clicked() {
                    goto = Some(*line);
                }
            }
        });
        if let Some(l) = goto {
            self.goto_line(l);
        }
    }

    fn doc_map(&mut self, ui: &mut Ui) {
        let pal = self.palette();
        let rect = ui.max_rect();
        let (rect, resp) = ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, pal.bg);
        let (lines, top, page) = {
            let v = self.cur_view();
            (self.cur_doc().lines(), v.top_row, v.page_rows.max(1))
        };
        let row_h = 2.0_f32;
        let fits = (rect.height() / row_h) as usize;
        // Lange Dokumente: Karte läuft proportional mit.
        let map_first = if lines > fits {
            let scroll_max = (lines.saturating_sub(page)).max(1) as f64;
            ((top / scroll_max) * (lines - fits) as f64).round().max(0.0) as usize
        } else {
            0
        };
        {
            let d = self.cur_doc();
            for i in 0..fits.min(lines.saturating_sub(map_first)) {
                let l = map_first + i;
                let line = d.text.line(l);
                let lead = line.chars().take_while(|c| *c == ' ' || *c == '\t').map(|c| if c == '\t' { 4 } else { 1 }).sum::<usize>();
                let len = d.line_len(l);
                if len == 0 || lead >= len {
                    continue;
                }
                let y = rect.top() + i as f32 * row_h;
                let x0 = rect.left() + 2.0 + lead as f32 * 0.8;
                let x1 = (x0 + (len - lead.min(len)) as f32 * 0.8).min(rect.right() - 1.0);
                painter.rect_filled(
                    egui::Rect::from_min_max(pos2(x0, y), pos2(x1, y + row_h - 0.8)),
                    0.0,
                    pal.fg.gamma_multiply(0.35),
                );
            }
        }
        let vy0 = rect.top() + (top as f32 - map_first as f32) * row_h;
        let vr = egui::Rect::from_min_size(pos2(rect.left(), vy0), vec2(rect.width(), page as f32 * row_h));
        painter.rect(vr, 0.0, Color32::from_rgba_unmultiplied(0x60, 0x80, 0xFF, 40), Stroke::new(1.0, Color32::from_rgb(0x60, 0x80, 0xC0)), StrokeKind::Inside);
        if let Some(p) = resp.interact_pointer_pos() {
            let target = map_first as f64 + ((p.y - rect.top()) / row_h) as f64 - page as f64 / 2.0;
            self.cur_view_mut().top_row = target.max(0.0);
        }
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, chrome(self.settings.theme).line), StrokeKind::Inside);
    }

    pub fn draw_dialogs(&mut self, ctx: &Context) {
        let Some(mut dialog) = self.dialog.take() else { return };
        let mut keep = true;
        let theme_id = self.settings.theme;
        let center = |title: &str| {
            egui::Window::new(crate::theme::window_title(theme_id, title))
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, -40.0))
        };
        let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        match &mut dialog {
            Dialog::Message { title, text } => {
                center(title).show(ctx, |ui| {
                    ui.set_max_width(460.0);
                    ui.label(text.as_str());
                    ui.add_space(8.0);
                    if ui.button(t!("dialog.ok")).clicked() || esc {
                        keep = false;
                    }
                });
            }
            Dialog::ConfirmClose { doc } => {
                let id = *doc;
                let name = self.docs.get(&id).map(|d| d.name()).unwrap_or_default();
                let mut answer: Option<Option<bool>> = None;
                center(&t!("dialog.save_title")).show(ctx, |ui| {
                    ui.label(t!("dialog.save_changes", name = name));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(t!("dialog.save")).clicked() {
                            answer = Some(Some(true));
                        }
                        if ui.button(t!("dialog.dont_save")).clicked() {
                            answer = Some(Some(false));
                        }
                        if ui.button(t!("dialog.cancel")).clicked() || esc {
                            answer = Some(None);
                        }
                    });
                });
                if let Some(a) = answer {
                    self.confirm_close_answer(id, a);
                    return;
                }
            }
            Dialog::GoTo { input, by_offset, first } => {
                let (cur_line, lines, len) = {
                    let d = self.cur_doc();
                    (d.text.char_to_line(self.cur_view().sels.main().head) + 1, d.lines(), d.len())
                };
                let mut go = false;
                center(&t!("dialog.goto_title")).show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.radio_value(by_offset, false, t!("dialog.goto_line"));
                        ui.radio_value(by_offset, true, t!("dialog.goto_offset"));
                    });
                    ui.label(if *by_offset {
                        t!("dialog.goto_offset_prompt", max = len + 1)
                    } else {
                        t!("dialog.goto_line_prompt", current = cur_line, max = lines)
                    });
                    let r = ui.add(egui::TextEdit::singleline(input).desired_width(180.0));
                    if *first {
                        *first = false;
                        r.request_focus();
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        go = true;
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        go |= ui.button(t!("dialog.goto_go")).clicked();
                        if ui.button(t!("dialog.cancel")).clicked() || esc {
                            keep = false;
                        }
                    });
                });
                if go {
                    if let Ok(n) = input.trim().parse::<usize>() {
                        if *by_offset {
                            let p = n.saturating_sub(1).min(len);
                            let v = self.cur_view_mut();
                            v.sels = SelSet::single(p);
                            v.want_reveal = true;
                            v.want_focus = true;
                        } else {
                            self.goto_line(n.saturating_sub(1));
                        }
                        keep = false;
                    }
                }
            }
            Dialog::Update { release, auto } => {
                let mut open_page = false;
                center(&t!("update.title")).show(ctx, |ui| {
                    ui.set_max_width(460.0);
                    ui.label(RichText::new(t!("update.available", app = APP_NAME, version = release.version)).strong());
                    ui.label(t!("update.current", version = crate::update::VERSION));
                    let notes = release.notes.trim();
                    if !notes.is_empty() {
                        ui.add_space(6.0);
                        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                            ui.label(notes);
                        });
                    }
                    ui.add_space(6.0);
                    ui.checkbox(auto, t!("update.auto"));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button(t!("update.download")).clicked() {
                            open_page = true;
                            keep = false;
                        }
                        if ui.button(t!("update.later")).clicked() || esc {
                            keep = false;
                        }
                    });
                });
                self.settings.update_check = *auto;
                if open_page {
                    crate::app::open_external(std::path::Path::new(&release.url));
                }
            }
            Dialog::About => {
                center(&t!("about.title", app = APP_NAME)).show(ctx, |ui| {
                    ui.heading(APP_NAME);
                    ui.label(t!("about.tagline"));
                    ui.label(t!("about.version", version = env!("CARGO_PKG_VERSION")));
                    ui.label("© 2026 R. Trunk");
                    ui.add_space(4.0);
                    ui.label(RichText::new(t!("about.license")).small());
                    ui.horizontal(|ui| {
                        ui.hyperlink_to(t!("about.github"), crate::update::REPO_URL);
                        ui.label("·");
                        ui.hyperlink_to(t!("about.license_text"), "https://www.gnu.org/licenses/gpl-3.0.html");
                    });
                    ui.add_space(8.0);
                    if ui.button(t!("dialog.ok")).clicked() || esc {
                        keep = false;
                    }
                });
            }
            Dialog::Summary { text } => {
                center(&t!("dialog.summary_title")).show(ctx, |ui| {
                    ui.label(text.as_str());
                    ui.add_space(8.0);
                    if ui.button(t!("dialog.ok")).clicked() || esc {
                        keep = false;
                    }
                });
            }
            Dialog::Hash { title, value } => {
                let v = value.clone();
                center(&t!("dialog.hash_title", name = title)).show(ctx, |ui| {
                    ui.set_max_width(520.0);
                    ui.add(egui::Label::new(RichText::new(&v).monospace()).wrap());
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(t!("dialog.copy")).clicked() {
                            ctx.copy_text(v.clone());
                        }
                        if ui.button(t!("dialog.close")).clicked() || esc {
                            keep = false;
                        }
                    });
                });
            }
            Dialog::Prefs => {
                let mut font_changed = None;
                let mut style_changed = false;
                let mut size = self.settings.font_size;
                center(&t!("prefs.title")).show(ctx, |ui| {
                    ui.set_min_width(390.0);
                    let s = &mut self.settings;
                    egui::Grid::new("prefs").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                        ui.label(t!("prefs.font"));
                        egui::ComboBox::from_id_salt("pref-font").selected_text(s.font.clone()).show_ui(ui, |ui| {
                            for (name, _, _) in ui_chrome::FONT_CHOICES {
                                if ui.selectable_label(s.font == name, name).clicked() {
                                    s.font = name.to_string();
                                    font_changed = Some(name.to_string());
                                }
                            }
                        });
                        ui.end_row();
                        ui.label(t!("prefs.font_size"));
                        ui.add(egui::DragValue::new(&mut size).range(6.0..=72.0).speed(0.25));
                        ui.end_row();
                        ui.label(t!("prefs.tab_width"));
                        ui.add(egui::DragValue::new(&mut s.ed.tab_size).range(1..=16));
                        ui.end_row();
                        ui.label(t!("prefs.new_docs"));
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("pref-eol")
                                .selected_text(Eol::from_name(&s.default_eol).label())
                                .show_ui(ui, |ui| {
                                    for e in [Eol::Crlf, Eol::Lf, Eol::Cr] {
                                        if ui.selectable_label(s.default_eol == e.name(), e.label()).clicked() {
                                            s.default_eol = e.name().to_string();
                                        }
                                    }
                                });
                        });
                        ui.end_row();
                    });
                    ui.add_space(6.0);
                    ui.checkbox(&mut s.ed.use_spaces, t!("prefs.use_spaces"));
                    ui.checkbox(&mut s.ed.auto_indent, t!("prefs.auto_indent"));
                    ui.checkbox(&mut s.ed.auto_close, t!("prefs.auto_close"));
                    ui.checkbox(&mut s.ed.smart_highlight, t!("prefs.smart_highlight"));
                    ui.checkbox(&mut s.ed.highlight_line, t!("prefs.highlight_line"));
                    ui.checkbox(&mut s.ed.brace_match, t!("prefs.brace_match"));
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut s.ed.auto_complete, t!("prefs.auto_complete"));
                        ui.add(egui::DragValue::new(&mut s.ed.auto_complete_from).range(1..=9));
                    });
                    ui.checkbox(&mut s.auto_reload, t!("prefs.auto_reload"));
                    ui.horizontal(|ui| {
                        ui.label(crate::t!("common.language"));
                        let system = crate::t!("common.language_system", name = crate::i18n::system_lang().native_name());
                        let current = s.language.map(|l| l.native_name().to_string()).unwrap_or_else(|| system.clone());
                        egui::ComboBox::from_id_salt("language")
                            .selected_text(current)
                            .width(190.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut s.language, None, system);
                                for l in crate::i18n::Lang::ALL {
                                    ui.selectable_value(&mut s.language, Some(l), l.native_name());
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label(t!("prefs.theme"));
                        egui::ComboBox::from_id_salt("theme")
                            .selected_text(s.theme.label())
                            .width(170.0)
                            .show_ui(ui, |ui| {
                                for t in crate::theme::ThemeId::ALL {
                                    style_changed |= ui.selectable_value(&mut s.theme, t, t.label()).changed();
                                }
                            });
                    });
                    ui.checkbox(&mut s.remember_session, t!("prefs.remember_session"));
                    ui.checkbox(&mut s.update_check, t!("prefs.update_check"));
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut s.large_file_enabled, t!("prefs.large_file"));
                        ui.add(egui::DragValue::new(&mut s.large_file_mb).range(1..=4096).suffix(t!("prefs.megabytes")));
                    });
                    ui.add_space(8.0);
                    if ui.button(t!("dialog.close")).clicked() || esc {
                        keep = false;
                    }
                });
                if size != self.settings.font_size {
                    self.change_font_size(size);
                }
                // Neue Sprache gilt sofort.
                crate::i18n::set_lang(self.settings.language.unwrap_or_else(crate::i18n::system_lang));
                if let Some(f) = font_changed {
                    ui_chrome::setup_fonts(ctx, &f, self.settings.theme.retro());
                    self.rebuild_metrics = true;
                }
                if style_changed {
                    self.style_dirty = true;
                }
                if !keep {
                    self.focus_editor();
                }
            }
            Dialog::Shortcuts => {
                center(&t!("dialog.shortcuts_title")).show(ctx, |ui| {
                    ui.set_min_width(400.0);
                    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        egui::Grid::new("keys").num_columns(2).striped(true).spacing([18.0, 3.0]).show(ui, |ui| {
                            for s in ui_chrome::shortcuts() {
                                ui.label(s.label_text());
                                ui.label(RichText::new(s.keys_text()).weak());
                                ui.end_row();
                            }
                        });
                    });
                    ui.add_space(6.0);
                    if ui.button(t!("dialog.close")).clicked() || esc {
                        keep = false;
                    }
                });
            }
            Dialog::Column { text, numbers, start, step, repeat, leading, format } => {
                let mut go = false;
                center(&t!("dialog.column_title")).show(ctx, |ui| {
                    ui.radio_value(numbers, false, t!("dialog.column_text"));
                    ui.add_enabled(!*numbers, egui::TextEdit::singleline(text).desired_width(240.0));
                    ui.add_space(4.0);
                    ui.radio_value(numbers, true, t!("dialog.column_numbers"));
                    ui.add_enabled_ui(*numbers, |ui| {
                        egui::Grid::new("col").num_columns(2).show(ui, |ui| {
                            ui.label(t!("dialog.column_start"));
                            ui.add(egui::TextEdit::singleline(start).desired_width(80.0));
                            ui.end_row();
                            ui.label(t!("dialog.column_step"));
                            ui.add(egui::TextEdit::singleline(step).desired_width(80.0));
                            ui.end_row();
                            ui.label(t!("dialog.column_repeat"));
                            ui.add(egui::TextEdit::singleline(repeat).desired_width(80.0));
                            ui.end_row();
                        });
                        ui.horizontal(|ui| {
                            ui.label(t!("dialog.column_pad"));
                            ui.radio_value(leading, 0, t!("dialog.column_pad_none"));
                            ui.radio_value(leading, 1, t!("dialog.column_pad_zeros"));
                            ui.radio_value(leading, 2, t!("dialog.column_pad_spaces"));
                        });
                        ui.horizontal(|ui| {
                            ui.radio_value(format, 0, t!("dialog.column_dec"));
                            ui.radio_value(format, 1, t!("dialog.column_hex"));
                            ui.radio_value(format, 2, t!("dialog.column_hex_upper"));
                            ui.radio_value(format, 3, t!("dialog.column_oct"));
                            ui.radio_value(format, 4, t!("dialog.column_bin"));
                        });
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        go = ui.button(t!("dialog.ok")).clicked();
                        if ui.button(t!("dialog.cancel")).clicked() || esc {
                            keep = false;
                        }
                    });
                });
                if go {
                    if *numbers {
                        let first: i64 = start.trim().parse().unwrap_or(0);
                        let inc: i64 = step.trim().parse().unwrap_or(1);
                        let rep: usize = repeat.trim().parse().unwrap_or(1);
                        let (leading, format) = (*leading, *format);
                        self.column_insert(|i, n| column_number(first, inc, rep, i, n, leading, format));
                    } else {
                        let t = text.clone();
                        self.column_insert(move |_, _| t.clone());
                    }
                    keep = false;
                    self.focus_editor();
                }
            }
            Dialog::ReloadAsk { doc } => {
                let id = *doc;
                let name = self.docs.get(&id).map(|d| d.name()).unwrap_or_default();
                let modified = self.docs.get(&id).is_some_and(|d| d.is_modified());
                center(&t!("dialog.reload_title")).show(ctx, |ui| {
                    ui.label(t!("dialog.reload_text", name = name));
                    if modified {
                        ui.colored_label(Color32::from_rgb(0xC0, 0x30, 0x30), t!("dialog.reload_loss"));
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(t!("dialog.reload")).clicked() {
                            self.reload_doc(id);
                            keep = false;
                        }
                        if ui.button(t!("dialog.keep")).clicked() || esc {
                            // Der Text weicht jetzt von der Datei ab und gilt als ungespeichert.
                            if let Some(d) = self.docs.get_mut(&id) {
                                d.extra_dirty = true;
                            }
                            keep = false;
                        }
                    });
                });
            }
            Dialog::DeletedAsk { doc } => {
                let id = *doc;
                let name = self.docs.get(&id).map(|d| d.name()).unwrap_or_default();
                center(&t!("dialog.deleted_title")).show(ctx, |ui| {
                    ui.label(t!("dialog.deleted_text", name = name));
                    ui.label(t!("dialog.deleted_hint"));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(t!("dialog.keep_in_editor")).clicked() || esc {
                            keep = false;
                        }
                        if ui.button(t!("dialog.close")).clicked() {
                            keep = false;
                            self.close_doc_everywhere(id);
                        }
                    });
                });
            }
            Dialog::Rename { name, first } => {
                let mut go = false;
                center(&t!("dialog.rename_title")).show(ctx, |ui| {
                    ui.label(t!("dialog.rename_label"));
                    let r = ui.add(egui::TextEdit::singleline(name).desired_width(300.0));
                    if *first {
                        *first = false;
                        r.request_focus();
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        go = true;
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        go |= ui.button(t!("dialog.rename")).clicked();
                        if ui.button(t!("dialog.cancel")).clicked() || esc {
                            keep = false;
                        }
                    });
                });
                if go && !name.trim().is_empty() {
                    let new_name = name.trim().to_string();
                    keep = false;
                    if let Err(e) = self.rename_current(&new_name) {
                        self.message(&t!("dialog.rename_title"), e);
                        return;
                    }
                }
            }
            Dialog::LossySave { doc, path } => {
                let (id, p) = (*doc, path.clone());
                let enc = self.docs.get(&id).map(|d| d.enc.label()).unwrap_or_default();
                center(&t!("dialog.lossy_title")).show(ctx, |ui| {
                    ui.label(t!("dialog.lossy_text", enc = enc));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(t!("dialog.save_as_utf8")).clicked() {
                            if let Some(d) = self.docs.get_mut(&id) {
                                d.enc = crate::fileio::Enc::Utf8;
                            }
                            keep = false;
                            self.write_doc(id, &p, false);
                        }
                        if ui.button(t!("dialog.cancel")).clicked() || esc {
                            keep = false;
                            self.closing.clear();
                            self.exit_after_close = false;
                        }
                    });
                });
            }
            Dialog::ConfirmReplaceOpen => {
                center(&t!("dialog.replace_open_title")).show(ctx, |ui| {
                    ui.label(t!("dialog.replace_open_text"));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(t!("dialog.replace")).clicked() {
                            keep = false;
                            self.replace_all_open();
                        }
                        if ui.button(t!("dialog.cancel")).clicked() || esc {
                            keep = false;
                        }
                    });
                });
            }
            Dialog::HugeOpen { path } => {
                let p = path.clone();
                center(&t!("dialog.huge_title")).show(ctx, |ui| {
                    ui.label(t!("dialog.huge_text"));
                    ui.label(t!("dialog.huge_question"));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(t!("dialog.yes")).clicked() {
                            keep = false;
                            self.open_path_checked(&p, true);
                        }
                        if ui.button(t!("dialog.no")).clicked() || esc {
                            keep = false;
                        }
                    });
                });
            }
            Dialog::ConfirmReplaceFiles { count_hint } => {
                let hint = count_hint.clone();
                center(&t!("dialog.replace_files_title")).show(ctx, |ui| {
                    ui.set_max_width(480.0);
                    ui.label(hint);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(t!("dialog.replace")).clicked() {
                            let with = self.find.with.clone();
                            self.start_file_job(Some(with));
                            keep = false;
                        }
                        if ui.button(t!("dialog.cancel")).clicked() || esc {
                            keep = false;
                        }
                    });
                });
            }
        }
        if keep && self.dialog.is_none() {
            self.dialog = Some(dialog);
        }
    }

    pub fn rename_current(&mut self, new_name: &str) -> Result<(), String> {
        let id = self.cur_id();
        let d = self.docs.get_mut(&id).ok_or_else(|| t!("dialog.rename_no_doc"))?;
        match d.path.clone() {
            None => {
                d.title = new_name.to_string();
                Ok(())
            }
            Some(old) => {
                let new = old.with_file_name(new_name);
                if new.exists() {
                    return Err(t!("dialog.rename_exists", name = new_name));
                }
                std::fs::rename(&old, &new).map_err(|e| t!("dialog.rename_failed", error = e))?;
                d.path = Some(new.clone());
                d.disk_mtime = crate::app::mtime(&new);
                Ok(())
            }
        }
    }
}

/// Eine Zahl für „Zahlenfolge einfügen“. Format: 0 dezimal, 1 hex, 2 HEX, 3 oktal, 4 binär; Auffüllen: 0 nichts, 1 Nullen, 2 Leerzeichen, bis zur Breite der längsten Zahl.
pub fn column_number(first: i64, inc: i64, repeat: usize, i: usize, count: usize, leading: u8, format: u8) -> String {
    let value = |k: usize| first + inc * (k / repeat.max(1)) as i64;
    let text = |v: i64| {
        let abs = v.unsigned_abs();
        let body = match format {
            1 => format!("{abs:x}"),
            2 => format!("{abs:X}"),
            3 => format!("{abs:o}"),
            4 => format!("{abs:b}"),
            _ => abs.to_string(),
        };
        (if v < 0 { "-" } else { "" }, body)
    };
    let width = [value(0), value(count.saturating_sub(1))]
        .iter()
        .map(|v| {
            let (sign, body) = text(*v);
            sign.len() + body.len()
        })
        .max()
        .unwrap_or(0);
    let (sign, body) = text(value(i));
    let plain = sign.len() + body.len();
    match leading {
        1 => format!("{sign}{}{body}", "0".repeat(width - plain)),
        2 => format!("{}{sign}{body}", " ".repeat(width - plain)),
        _ => format!("{sign}{body}"),
    }
}

