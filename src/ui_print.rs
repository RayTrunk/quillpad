//! Druckfenster: links die Vorschau einer Seite, rechts die Optionen. Schrift, Drucker,
//! Papier und Seiten kommen aus Hintergrund-Threads (`print::job`); der UI-Thread wartet
//! nie, außer auf den modalen Treiberdialog unter Windows.
use crate::app::{App, Dialog};
use crate::doc::DocId;
use crate::i18n::Lang;
use crate::print::font::PrintFont;
use crate::print::job::{self, LayoutReq, COLOR_MAX_LINES};
use crate::print::{self, Geom, LayoutOpts, Page, Paper, Printer, MM};
use crate::settings::PrintSettings;
use crate::t;
use egui::{vec2, Align2, Button, Color32, Context, Key, Ui};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;

mod preview;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageSel {
    All,
    Range(u32, u32),
    Selection,
}

/// Ergebnis von Drucken oder Als PDF: Druckername bzw. Pfad, sonst die Fehlermeldung.
pub type OutRx = Receiver<Result<String, String>>;

/// Was die Schaltflächen im Fenster ausgelöst haben.
enum Choice {
    Stay,
    Cancel,
    Print,
    Pdf,
}

pub struct PrintUi {
    /// Kopie der Druckeinstellungen; erst Drucken oder Als PDF übernimmt sie.
    pub(crate) opts: PrintSettings,
    doc: DocId,
    /// None, solange die Liste noch gelesen wird.
    printers: Option<Result<Vec<Printer>, String>>,
    printers_rx: Option<Receiver<Result<Vec<Printer>, String>>>,
    pub(crate) printer: Option<String>,
    paper: Option<Paper>,
    paper_rx: Option<Receiver<Paper>>,
    copies: u32,
    pub(crate) sel: PageSel,
    /// Zuletzt eingegebener Bereich, solange „alle“ oder „Markierung“ gewählt ist.
    last_range: (u32, u32),
    pub(crate) pages: Option<Vec<Page>>,
    /// Geometrie der Seiten in `pages`.
    geom: Option<Geom>,
    /// Laufender Seitenaufbau und die Geometrie, für die er rechnet.
    rx: Option<(Receiver<Vec<Page>>, Geom)>,
    page_idx: usize,
    /// (doc.rev, Hash der Optionen) des zuletzt gestarteten Seitenaufbaus.
    built_for: Option<(u64, u64)>,
    font: Option<Arc<PrintFont>>,
    font_rx: Option<Receiver<Option<PrintFont>>>,
    /// Beim ersten Bild den Fokus vom Editor holen, sonst blättert Bild-auf/ab auch dort.
    focus: bool,
}

/// Seitenbereich `a..=b` auf 1..=n begrenzt, als Indizes; None bei null Seiten.
fn clamp_range(a: u32, b: u32, n: usize) -> Option<(usize, usize)> {
    if n == 0 {
        return None;
    }
    let a = (a as usize).clamp(1, n);
    Some((a - 1, (b as usize).clamp(a, n)))
}

/// Papiergröße des Druckers; jeder Rand ist der eingestellte, mindestens der nicht bedruckbare.
fn page_geom(paper: &Paper, margin_mm: f32) -> Geom {
    let (g, m) = (paper.geom_full, margin_mm * MM);
    Geom { w: g.w, h: g.h, left: g.left.max(m), top: g.top.max(m), right: g.right.max(m), bottom: g.bottom.max(m) }
}

/// Heutiges Datum für die Kopfzeile, im Format der Oberflächensprache.
fn today() -> String {
    let fmt = match crate::i18n::lang() {
        Lang::De => "%d.%m.%Y",
        Lang::Fr => "%d/%m/%Y",
        Lang::En => "%b %-d, %Y",
    };
    chrono::Local::now().format(fmt).to_string()
}

impl PrintUi {
    /// Alles da: Schrift, Drucker, Papier und Seiten zum letzten Stand.
    pub(crate) fn ready(&self) -> bool {
        self.pages.is_some()
            && self.rx.is_none()
            && self.font_rx.is_none()
            && self.printers_rx.is_none()
            && self.paper_rx.is_none()
    }

    /// Die Seiten, die gedruckt werden: nach dem Umbruch auf den Bereich gefiltert,
    /// damit „Seite x von N“ die Gesamtzahl behält.
    pub(crate) fn shown(&self) -> &[Page] {
        let pages = self.pages.as_deref().unwrap_or(&[]);
        match self.sel {
            PageSel::Range(a, b) => match clamp_range(a, b, pages.len()) {
                Some((i, j)) => &pages[i..j],
                None => &[],
            },
            _ => pages,
        }
    }

    fn printer_list(&self) -> &[Printer] {
        match &self.printers {
            Some(Ok(v)) => v,
            _ => &[],
        }
    }

    /// Papier zum gewählten Drucker neu lesen (startet ggf. `lpoptions` oder `CreateIC`).
    fn request_paper(&mut self, ctx: &Context) {
        let choice = self.opts.paper.clone();
        match self.printer.clone() {
            Some(p) => self.paper_rx = Some(job::spawn(move || print::paper_for(&p, choice.as_deref()), ctx.clone())),
            None => {
                self.paper_rx = None;
                self.paper = Some(choice.as_deref().and_then(print::paper_named).unwrap_or_else(print::default_paper));
            }
        }
    }

    /// Holt fertige Ergebnisse ab. false = keine Druckschrift gefunden.
    fn poll(&mut self, ctx: &Context) -> bool {
        if let Some(rx) = &self.font_rx {
            match rx.try_recv() {
                Ok(Some(f)) => {
                    self.font = Some(Arc::new(f));
                    self.font_rx = None;
                }
                Ok(None) | Err(TryRecvError::Disconnected) => return false,
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &self.printers_rx {
            if let Some(list) = recv(rx) {
                self.printers_rx = None;
                let pick = list.as_ref().ok().and_then(|v| {
                    let saved = self.opts.printer.as_deref();
                    v.iter()
                        .find(|p| Some(p.name.as_str()) == saved)
                        .or_else(|| v.iter().find(|p| p.is_default))
                        .or_else(|| v.first())
                        .map(|p| p.name.clone())
                });
                self.printers = Some(list);
                self.printer = pick;
                self.request_paper(ctx);
            }
        }
        if let Some(rx) = &self.paper_rx {
            if let Some(p) = recv(rx) {
                self.paper = Some(p);
                self.paper_rx = None;
            }
        }
        if let Some((rx, g)) = &self.rx {
            if let Some(pages) = recv(rx) {
                self.geom = Some(*g);
                self.pages = Some(pages);
                self.rx = None;
            }
        }
        true
    }
}

/// Wert aus dem Kanal, falls schon da. Ein abgerissener Kanal (Thread abgestürzt) liefert nichts;
/// dann bleibt das Fenster beim Hinweis „wird berechnet“, statt abzustürzen.
fn recv<T>(rx: &Receiver<T>) -> Option<T> {
    rx.try_recv().ok()
}

impl App {
    /// Genau eine nicht leere Auswahl und keine Rechteckauswahl.
    pub fn can_print_selection(&self) -> bool {
        let v = self.cur_view();
        v.box_sel.is_none() && v.sels.sels.len() == 1 && !v.sels.sels[0].is_empty()
    }

    fn print_range(&self, doc: DocId) -> Option<(usize, usize)> {
        if self.cur_id() != doc || !self.can_print_selection() {
            return None;
        }
        let s = self.cur_view().sels.sels[0];
        Some((s.start(), s.end()))
    }

    pub fn open_print(&mut self) {
        let ctx = self.ctx.clone();
        let font_name = self.settings.font.clone();
        let ui = PrintUi {
            opts: self.settings.print.clamped(),
            doc: self.cur_id(),
            printers: None,
            printers_rx: Some(job::spawn(print::printers, ctx.clone())),
            printer: None,
            paper: None,
            paper_rx: None,
            copies: 1,
            sel: PageSel::All,
            last_range: (1, 1),
            pages: None,
            geom: None,
            rx: None,
            page_idx: 0,
            built_for: None,
            font: None,
            font_rx: Some(job::spawn(move || PrintFont::load(&font_name), ctx)),
            focus: true,
        };
        self.dialog = Some(Dialog::Print(Box::new(ui)));
    }

    /// Meldet das Ergebnis eines Druckauftrags oder einer PDF, sobald kein anderes Fenster offen ist.
    pub fn poll_print(&mut self) {
        if self.dialog.is_some() {
            return;
        }
        let Some((rx, pdf)) = &self.print_rx else { return };
        let pdf = *pdf;
        let res = match rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(t!("print.lost")),
        };
        self.print_rx = None;
        let text = match res {
            Ok(path) if pdf => t!("print.pdf_saved", path = path),
            Ok(printer) => t!("print.sent", printer = printer),
            Err(e) if e == "no-cups" => t!("print.no_cups"),
            Err(e) => t!("msg.print_failed", error = e),
        };
        self.message(&t!("app.print"), text);
    }

    /// Startet den Seitenaufbau neu, wenn sich Text, Optionen oder Papier geändert haben.
    fn want_layout(&self, p: &mut PrintUi, ctx: &Context) {
        let (Some(font), Some(paper), Some(d)) = (&p.font, &p.paper, self.docs.get(&p.doc)) else { return };
        let o = p.opts.clamped();
        let geom = page_geom(paper, o.margin_mm);
        let color = o.color && d.lines() <= COLOR_MAX_LINES;
        let range = if p.sel == PageSel::Selection { self.print_range(p.doc) } else { None };
        let (name, date, tab) = (d.name(), today(), self.settings.ed.tab_size);
        let mut h = DefaultHasher::new();
        (o.font_size_pt.to_bits(), o.line_numbers, o.header_footer, color, range, tab).hash(&mut h);
        (&name, &date, &d.hl.language).hash(&mut h);
        [geom.w, geom.h, geom.left, geom.top, geom.right, geom.bottom].map(f32::to_bits).hash(&mut h);
        let key = (d.rev, h.finish());
        if p.built_for == Some(key) {
            return;
        }
        p.built_for = Some(key);
        let opts = LayoutOpts {
            size: o.font_size_pt,
            line_numbers: o.line_numbers,
            header_footer: o.header_footer,
            tab_size: tab,
            header_left: name,
            header_right: date,
            // {page} und {total} bleiben stehen und werden von `paginate` ersetzt.
            footer: t!("print.footer"),
        };
        let req = LayoutReq { text: d.text.clone(), language: d.hl.language.clone(), range, opts, geom, color, font: font.clone() };
        // Ein laufender alter Aufbau wird verworfen: sein Ergebnis geht an einen geschlossenen Kanal.
        p.rx = Some((job::spawn_layout(req, ctx.clone()), geom));
    }

    /// Zeichnet das Druckfenster; false = schließen.
    pub fn draw_print(&mut self, ctx: &Context, p: &mut PrintUi) -> bool {
        if !p.poll(ctx) {
            self.message(&t!("app.print"), t!("print.no_font"));
            return false;
        }
        if !self.docs.contains_key(&p.doc) {
            return false;
        }
        if p.sel == PageSel::Selection && self.print_range(p.doc).is_none() {
            p.sel = PageSel::All;
        }
        self.want_layout(p, ctx);
        let max_h = (ctx.screen_rect().height() - 200.0).clamp(160.0, 520.0);
        let title = crate::theme::window_title(self.settings.theme, &t!("print.title"));
        let mut choice = Choice::Stay;
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| self.print_preview(ui, p, max_h));
                    ui.add_space(14.0);
                    ui.vertical(|ui| choice = self.print_options(ui, p, ctx));
                });
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            choice = Choice::Cancel;
        }
        match choice {
            Choice::Stay => true,
            Choice::Cancel => {
                self.focus_editor();
                false
            }
            Choice::Print => {
                self.start_output(p, ctx, None);
                false
            }
            Choice::Pdf => {
                let name = self.docs[&p.doc].name();
                let picked = rfd::FileDialog::new().set_file_name(format!("{name}.pdf")).add_filter("PDF", &["pdf"]).save_file();
                match picked {
                    Some(path) => {
                        self.start_output(p, ctx, Some(path));
                        false
                    }
                    None => true,
                }
            }
        }
    }

    /// Übernimmt die Einstellungen und startet Druck (`pdf` = None) oder PDF im Hintergrund.
    fn start_output(&mut self, p: &PrintUi, ctx: &Context, pdf: Option<std::path::PathBuf>) {
        let mut s = p.opts.clamped();
        s.printer = p.printer.clone();
        self.settings.print = s;
        self.focus_editor();
        let (Some(g), Some(font), Some(paper)) = (p.geom, p.font.clone(), p.paper.clone()) else { return };
        let pages = p.shown().to_vec();
        let title = self.docs[&p.doc].name();
        let is_pdf = pdf.is_some();
        let f: Box<dyn FnOnce() -> Result<String, String> + Send> = match pdf {
            Some(path) => Box::new(move || {
                let bytes = print::pdf::write_pdf(&pages, &g, &font, &title)?;
                std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
                Ok(path.display().to_string())
            }),
            None => {
                let Some(printer) = p.printer.clone() else { return };
                // Unter Windows gibt es keine Duplex-Wahl; der Treiber (oder „Eigenschaften…“) entscheidet.
                let (copies, duplex) = (p.copies, p.opts.duplex && cfg!(not(windows)));
                Box::new(move || {
                    print::print_pages(&printer, &pages, &g, &font, &title, copies, duplex, &paper).map(|()| printer)
                })
            }
        };
        self.print_rx = Some((job::spawn_output(f, ctx.clone()), is_pdf));
    }

    /// Die Optionen rechts und die Schaltflächen.
    fn print_options(&mut self, ui: &mut Ui, p: &mut PrintUi, ctx: &Context) -> Choice {
        let too_large = self.docs.get(&p.doc).is_some_and(|d| d.lines() > COLOR_MAX_LINES);
        let can_sel = self.can_print_selection() && self.cur_id() == p.doc;
        let has_printer = p.printer.is_some();
        let mut paper_changed = false;
        egui::Grid::new("print-opts").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label(t!("print.printer"));
            let current = p.printer.clone().unwrap_or_else(|| if p.printers_rx.is_some() { "…".into() } else { "—".into() });
            ui.add_enabled_ui(has_printer, |ui| {
                egui::ComboBox::from_id_salt("print-printer").width(220.0).selected_text(current).show_ui(ui, |ui| {
                    for pr in p.printer_list().to_vec() {
                        if ui.selectable_label(p.printer.as_deref() == Some(pr.name.as_str()), &pr.name).clicked() {
                            p.printer = Some(pr.name);
                            paper_changed = true;
                        }
                    }
                });
            });
            ui.end_row();
            if p.printers_rx.is_none() && !has_printer {
                ui.label("");
                ui.colored_label(Color32::from_rgb(0xC0, 0x30, 0x30), t!("print.no_printer"));
                ui.end_row();
            }

            ui.label(t!("print.copies"));
            ui.add(egui::DragValue::new(&mut p.copies).range(1..=99));
            ui.end_row();

            ui.label(t!("print.pages"));
            ui.vertical(|ui| {
                if ui.radio(p.sel == PageSel::All, t!("print.all")).clicked() {
                    p.sel = PageSel::All;
                }
                ui.horizontal(|ui| {
                    let on = matches!(p.sel, PageSel::Range(..));
                    if ui.radio(on, t!("print.range")).clicked() && !on {
                        p.sel = PageSel::Range(p.last_range.0, p.last_range.1);
                    }
                    let total = p.pages.as_ref().map_or(1, |v| v.len().max(1)) as u32;
                    let (mut a, mut b) = match p.sel {
                        PageSel::Range(a, b) => (a, b),
                        _ => p.last_range,
                    };
                    ui.add_enabled_ui(on, |ui| {
                        ui.add(egui::DragValue::new(&mut a).range(1..=total));
                        ui.label(t!("print.to"));
                        ui.add(egui::DragValue::new(&mut b).range(1..=total));
                    });
                    if on {
                        p.sel = PageSel::Range(a, b);
                        p.last_range = (a, b);
                    }
                });
                let r = ui.add_enabled(can_sel, egui::RadioButton::new(p.sel == PageSel::Selection, t!("print.selection")));
                if r.clicked() {
                    p.sel = PageSel::Selection;
                }
            });
            ui.end_row();

            ui.label("");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    if too_large {
                        let mut off = false;
                        ui.add_enabled(false, egui::Checkbox::new(&mut off, t!("print.colors")));
                    } else {
                        ui.checkbox(&mut p.opts.color, t!("print.colors"));
                    }
                    ui.checkbox(&mut p.opts.line_numbers, t!("print.line_numbers"));
                });
                if too_large {
                    ui.label(egui::RichText::new(t!("print.too_large_for_colors")).small());
                }
                ui.checkbox(&mut p.opts.header_footer, t!("print.header_footer"));
            });
            ui.end_row();

            ui.label(t!("print.font_size"));
            ui.add(egui::DragValue::new(&mut p.opts.font_size_pt).range(6.0..=24.0).speed(0.25).suffix(" pt"));
            ui.end_row();
            ui.label(t!("print.margins"));
            ui.add(egui::DragValue::new(&mut p.opts.margin_mm).range(5.0..=40.0).speed(0.25).suffix(" mm"));
            ui.end_row();

            ui.label(t!("print.paper"));
            paper_changed |= paper_row(ui, p);
            ui.end_row();
        });
        if paper_changed {
            p.request_paper(ctx);
        }
        #[cfg(not(windows))]
        ui.checkbox(&mut p.opts.duplex, t!("print.duplex"));
        #[cfg(windows)]
        if ui.add_enabled(has_printer, Button::new(t!("print.properties"))).clicked() {
            // Modaler Treiberdialog im UI-Thread; er verarbeitet seine Nachrichten selbst.
            if let Some(name) = &p.printer {
                if print::properties(name, self.hwnd) {
                    p.request_paper(ctx);
                }
            }
        }

        ui.add_space(16.0);
        let mut choice = Choice::Stay;
        ui.horizontal(|ui| {
            let ready = p.ready() && !p.shown().is_empty();
            if ui.add_enabled(ready, Button::new(t!("print.as_pdf"))).clicked() {
                choice = Choice::Pdf;
            }
            if ui.button(t!("dialog.cancel")).clicked() {
                choice = Choice::Cancel;
            }
            if ui.add_enabled(ready && has_printer, Button::new(t!("print.print"))).clicked() {
                choice = Choice::Print;
            }
        });
        choice
    }
}

/// Papier: unter Linux wählbar (vom Drucker oder fest), unter Windows nur angezeigt,
/// dort bestimmt es der Treiber. true = Wahl geändert.
fn paper_row(ui: &mut Ui, p: &mut PrintUi) -> bool {
    let name = p.paper.as_ref().map_or_else(|| "…".to_string(), |x| x.name.clone());
    if cfg!(windows) {
        ui.label(name);
        return false;
    }
    let mut changed = false;
    let shown = match &p.opts.paper {
        None => format!("{} ({name})", t!("print.paper_auto")),
        Some(n) => n.clone(),
    };
    egui::ComboBox::from_id_salt("print-paper").selected_text(shown).show_ui(ui, |ui| {
        if ui.selectable_label(p.opts.paper.is_none(), t!("print.paper_auto")).clicked() {
            p.opts.paper = None;
            changed = true;
        }
        for &(n, _, _) in print::PAPERS.iter() {
            if ui.selectable_label(p.opts.paper.as_deref() == Some(n), n).clicked() {
                p.opts.paper = Some(n.to_string());
                changed = true;
            }
        }
    });
    changed
}
