//! Suchen und Ersetzen: Fenster, Schritte im Dokument und die Suche über Dateien.
use crate::app::{App, Dialog};
use crate::doc::{DocId, Document, Kind, SelSet};
use crate::fileio::{self, Eol};
use crate::search::{self, Mode, Query};
use crate::settings::Settings;
use crate::ui_panels::Hit;
use egui::{Context, RichText};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindTab {
    Find,
    Replace,
    Files,
    Mark,
}

/// Einträge im Suchverlauf.
pub const FIND_HISTORY_MAX: usize = 10;
const NOT_ANSI_MESSAGE: &str = "Ersetzen: In einem ANSI-Dokument lässt sich nicht durch Nicht-ANSI-Text ersetzen.";

pub struct FindUi {
    pub open: bool,
    pub tab: FindTab,
    pub what: String,
    pub with: String,
    pub history: Vec<String>,
    pub mode: Mode,
    pub case: bool,
    pub whole: bool,
    pub wrap: bool,
    pub up: bool,
    pub dot_nl: bool,
    pub in_selection: bool,
    pub dir: String,
    pub filters: String,
    pub subfolders: bool,
    pub hidden: bool,
    pub bookmark_lines: bool,
    pub purge: bool,
    pub message: String,
    pub message_err: bool,
    pub focus_what: bool,
}

impl FindUi {
    pub fn new(settings: &Settings) -> FindUi {
        FindUi {
            open: false,
            tab: FindTab::Find,
            what: String::new(),
            with: String::new(),
            history: settings.find_history.clone(),
            mode: Mode::Normal,
            case: false,
            whole: false,
            wrap: true,
            up: false,
            dot_nl: false,
            in_selection: false,
            dir: String::new(),
            filters: "*.*".into(),
            subfolders: true,
            hidden: false,
            bookmark_lines: false,
            purge: true,
            message: String::new(),
            message_err: false,
            focus_what: false,
        }
    }

    pub fn query(&self) -> Query {
        Query {
            pattern: self.what.clone(),
            mode: self.mode,
            case: self.case,
            whole: self.whole,
            dot_nl: self.dot_nl,
        }
    }

    fn say(&mut self, text: impl Into<String>, err: bool) {
        self.message = text.into();
        self.message_err = err;
    }

    fn remember(&mut self) {
        let w = self.what.clone();
        if w.is_empty() {
            return;
        }
        self.history.retain(|x| *x != w);
        self.history.insert(0, w);
        self.history.truncate(FIND_HISTORY_MAX);
    }
}

pub enum JobMsg {
    File { path: PathBuf, hits: Vec<Hit> },
    Replaced { count: usize },
    Failed { path: PathBuf, error: String },
    Done { files: usize, skipped: usize },
}

pub struct FindJob {
    pub rx: Receiver<JobMsg>,
    pub cancel: Arc<AtomicBool>,
    pub hits: usize,
    pub files_hit: usize,
    pub replacing: bool,
    pub replaced: usize,
    pub failed: usize,
    pub first_error: Option<String>,
}

/// Macht aus Byte-Treffern eine Trefferliste mit Zeile, Spalte und Zeilentext.
pub fn hits_in(text: &str, ranges: &[(usize, usize)]) -> Vec<Hit> {
    let mut out = Vec::with_capacity(ranges.len());
    let bytes = text.as_bytes();
    let mut line = 0usize;
    let mut line_start = 0usize;
    let mut scanned = 0usize;
    for &(s, e) in ranges {
        for (i, b) in bytes[scanned..s].iter().enumerate() {
            if *b == b'\n' {
                line += 1;
                line_start = scanned + i + 1;
            }
        }
        scanned = s;
        let line_end = text[s..].find('\n').map(|n| s + n).unwrap_or(text.len());
        let full = &text[line_start..line_end];
        let col = text[line_start..s].chars().count();
        let len = text[s..e].chars().count();
        let shown: String = full.chars().take(400).collect();
        out.push(Hit { line, col, len, text: shown });
    }
    out
}

/// Eine Treffer-Position im Dokument als Zeichenbereich.
fn to_chars(doc: &Document, m: (usize, usize)) -> (usize, usize) {
    (doc.text.byte_to_char(m.0), doc.text.byte_to_char(m.1))
}

fn walk_files(dir: &str, filters: &str, subfolders: bool, hidden: bool) -> impl Iterator<Item = PathBuf> {
    let mut w = walkdir::WalkDir::new(dir).follow_links(false);
    if !subfolders {
        w = w.max_depth(1);
    }
    let filter = search::FileFilter::parse(filters);
    let for_dirs = filter.clone();
    w.into_iter()
        .filter_entry(move |e| {
            if e.depth() == 0 {
                return true;
            }
            let name = e.file_name().to_string_lossy();
            (hidden || !name.starts_with('.')) && (!e.file_type().is_dir() || for_dirs.dir_ok(&name, e.depth()))
        })
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(move |e| filter.file_ok(&e.file_name().to_string_lossy()))
        .map(|e| e.into_path())
}

/// Fenster und Suchfeld: Markierungen bis zu dieser Länge füllen das Suchfeld, ab dieser Länge
/// wird „Nur in der Markierung“ von selbst angehakt.
/// Längere Markierungen sind eher ein Bereich als ein Suchbegriff.
const FILL_FIND_THRESHOLD: usize = 500;

fn looks_binary(bytes: &[u8]) -> bool {
    if matches!(fileio::detect_enc(bytes), fileio::Enc::Utf16Le | fileio::Enc::Utf16Be) {
        return false;
    }
    bytes[..bytes.len().min(8192)].contains(&0)
}

impl App {
    pub fn show_find(&mut self, tab: FindTab) {
        let main = self.cur_view().sels.main();
        let selected = main.end() - main.start();
        if selected > 0 && selected <= FILL_FIND_THRESHOLD {
            let t = self.cur_doc().slice(main.start(), main.end());
            if !t.contains('\n') {
                self.find.what = t;
            }
        } else if selected == 0 {
            // Ohne Markierung füllt das Wort am Cursor das Suchfeld.
            let word = self.ed().map(|e| e.word_at(main.head));
            if let Some((a, b)) = word {
                let t = self.cur_doc().slice(a, b);
                if t.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    self.find.what = t;
                }
            }
        }
        // Eine Markierung über mehrere Zeilen ist ein Bereich: dann nur darin suchen.
        self.find.in_selection = selected > 0 && self.cur_doc().slice(main.start(), main.end()).contains('\n');
        if self.find.dir.is_empty() {
            self.find.dir = self
                .cur_doc()
                .path
                .as_ref()
                .and_then(|p| p.parent())
                .map(|p| p.display().to_string())
                .or_else(|| dirs::document_dir().map(|p| p.display().to_string()))
                .unwrap_or_default();
        }
        self.find.tab = tab;
        self.find.open = true;
        self.find.focus_what = true;
        self.find.message.clear();
    }

    /// Sucht den nächsten Treffer und markiert ihn. false = keiner gefunden.
    pub fn find_step(&mut self, backward: bool) -> bool {
        self.sync_views();
        let backward = backward ^ self.find.up;
        if self.find.what.is_empty() {
            self.show_find(FindTab::Find);
            return false;
        }
        self.find.remember();
        let re = match self.find.query().compile() {
            Ok(r) => r,
            Err(e) => {
                self.find.say(e, true);
                return false;
            }
        };
        let text = self.cur_doc().full_text();
        let sel = self.cur_view().sels.main();
        let (from_start, from_end) = {
            let d = self.cur_doc();
            (d.text.char_to_byte(sel.start()), d.text.char_to_byte(sel.end()))
        };
        let wrap = self.find.wrap;
        let mut wrapped = false;
        let found = if backward {
            let mut last = None;
            let mut all_last = None;
            for m in re.find_iter(&text) {
                if m.end() <= from_start && !(m.start() == from_start && m.end() == from_end) {
                    last = Some((m.start(), m.end()));
                }
                all_last = Some((m.start(), m.end()));
            }
            last.or_else(|| {
                if wrap && all_last.is_some() {
                    wrapped = true;
                }
                if wrap { all_last } else { None }
            })
        } else {
            let mut start = from_end;
            // Leerer Treffer an derselben Stelle: ein Zeichen weitergehen, sonst bliebe die Suche stehen.
            if let Some(m) = re.find_at(&text, start) {
                if m.start() == m.end() && m.start() == start {
                    start = next_char(&text, start);
                }
            }
            let hit = if start <= text.len() { re.find_at(&text, start) } else { None };
            hit.map(|m| (m.start(), m.end())).or_else(|| {
                if wrap {
                    wrapped = true;
                    re.find(&text).map(|m| (m.start(), m.end()))
                } else {
                    None
                }
            })
        };
        match found {
            Some(m) => {
                let (a, b) = to_chars(self.cur_doc(), m);
                self.select_range(a, b);
                let message = if wrapped { "Am Dokumentende angekommen, von vorn weitergesucht." } else { "" };
                self.find.say(message, false);
                true
            }
            None => {
                self.find.say(format!("Nicht gefunden: „{}“", self.find.what), true);
                false
            }
        }
    }

    /// Ersetzt die aktuelle Markierung, falls sie ein Treffer ist, und springt zum nächsten.
    pub fn replace_one(&mut self) {
        self.sync_views();
        if self.cur_doc().read_only {
            self.find.say("Das Dokument ist schreibgeschützt.", true);
            return;
        }
        let q = self.find.query();
        let re = match q.compile() {
            Ok(r) => r,
            Err(e) => return self.find.say(e, true),
        };
        let text = self.cur_doc().full_text();
        let sel = self.cur_view().sels.main();
        let (bs, be) = {
            let d = self.cur_doc();
            (d.text.char_to_byte(sel.start()), d.text.char_to_byte(sel.end()))
        };
        let is_hit = !sel.is_empty() && re.find_at(&text, bs).is_some_and(|m| m.start() == bs && m.end() == be);
        if is_hit && self.replacement_not_ansi(self.cur_id()) {
            return self.find.say(NOT_ANSI_MESSAGE, true);
        }
        if is_hit {
            let new = q.expand(&re, &text, (bs, be), &self.find.with);
            let before = self.cur_view().sels.clone();
            let d = self.cur_doc_mut();
            d.begin(Kind::Other, &before);
            d.splice(sel.start(), sel.end(), &new);
            let after = SelSet::single(sel.start() + new.chars().count());
            d.commit(&after);
            let rev = d.rev;
            let v = self.cur_view_mut();
            v.sels = after;
            v.seen_rev = rev;
        }
        self.find_step(false);
    }

    /// In ANSI-Dokumenten darf der neue Text nur Zeichen enthalten, die ANSI darstellen kann.
    fn replacement_not_ansi(&self, id: DocId) -> bool {
        self.docs.get(&id).is_some_and(|d| d.enc == fileio::Enc::Ansi)
            && fileio::encode(&self.find.with, fileio::Enc::Ansi, Eol::Lf).1
    }

    /// Ersetzt alle Treffer in einem Dokument als einen Rückgängig-Schritt.
    fn replace_all_in(&mut self, id: DocId, only: Option<(usize, usize)>) -> Result<usize, String> {
        let q = self.find.query();
        let re = q.compile()?;
        if self.replacement_not_ansi(id) {
            return Err(NOT_ANSI_MESSAGE.into());
        }
        let with = self.find.with.clone();
        let doc = self.docs.get_mut(&id).ok_or("Dokument nicht mehr offen")?;
        if doc.read_only {
            return Err("Alle ersetzen: Das Dokument ist schreibgeschützt.".into());
        }
        let text = doc.full_text();
        let mut ranges = search::find_all(&re, &text);
        if let Some((a, b)) = only {
            let (ba, bb) = (doc.text.char_to_byte(a), doc.text.char_to_byte(b));
            ranges.retain(|m| m.0 >= ba && m.1 <= bb);
        }
        if ranges.is_empty() {
            return Ok(0);
        }
        let before = SelSet::single(0);
        doc.begin(Kind::Other, &before);
        for &(s, e) in ranges.iter().rev() {
            let new = q.expand(&re, &text, (s, e), &with);
            let (a, b) = (doc.text.byte_to_char(s), doc.text.byte_to_char(e));
            doc.splice(a, b, &new);
        }
        doc.commit(&before);
        Ok(ranges.len())
    }

    pub fn replace_all_current(&mut self) {
        let id = self.cur_id();
        let main = self.cur_view().sels.main();
        let only = (self.find.in_selection && !main.is_empty()).then(|| (main.start(), main.end()));
        let result = self.replace_all_in(id, only);
        self.sync_views();
        match result {
            Ok(n) => {
                self.find.remember();
                self.find.say(format!("{n} Treffer ersetzt."), n == 0);
                // Ist von der Markierung nichts übrig, gilt die Suche wieder im ganzen Dokument.
                if self.cur_view().sels.main().is_empty() {
                    self.find.in_selection = false;
                }
            }
            Err(e) => self.find.say(e, true),
        }
    }

    pub fn replace_all_open(&mut self) {
        let ids: Vec<DocId> = self.docs.keys().copied().collect();
        let mut total = 0;
        let mut docs = 0;
        for id in ids {
            // Schreibgeschützte Dokumente werden übersprungen.
            if self.docs[&id].read_only {
                continue;
            }
            match self.replace_all_in(id, None) {
                Ok(0) => {}
                Ok(n) => {
                    total += n;
                    docs += 1;
                }
                Err(e) => return self.find.say(e, true),
            }
        }
        self.find.say(format!("{total} Treffer in {} ersetzt.", crate::ui_panels::docs_label(docs)), total == 0);
    }

    pub fn count_matches(&mut self) -> usize {
        match self.find.query().compile() {
            Ok(re) => {
                self.find.remember();
                let n = re.find_iter(&self.cur_doc().full_text()).count();
                self.find.say(format!("{n} Treffer."), n == 0);
                n
            }
            Err(e) => {
                self.find.say(e, true);
                0
            }
        }
    }

    pub fn find_all_docs(&mut self, only_current: bool) {
        let re = match self.find.query().compile() {
            Ok(r) => r,
            Err(e) => return self.find.say(e, true),
        };
        self.find.remember();
        let mut ids: Vec<DocId> = if only_current { vec![self.cur_id()] } else { self.docs.keys().copied().collect() };
        ids.sort();
        self.results.begin(format!("Suche nach „{}“", self.find.what));
        let (mut total, mut files) = (0, 0);
        for id in ids {
            let d = &self.docs[&id];
            let text = d.full_text();
            let hits = hits_in(&text, &search::find_all(&re, &text));
            if !hits.is_empty() {
                total += hits.len();
                files += 1;
                self.results.add_file(Some(id), d.path.clone(), d.name(), hits);
            }
        }
        self.results.finish(total, files);
        self.find.say(format!("{total} Treffer in {}.", crate::ui_panels::docs_label(files)), total == 0);
    }

    pub fn mark_all(&mut self) -> usize {
        let re = match self.find.query().compile() {
            Ok(r) => r,
            Err(e) => {
                self.find.say(e, true);
                return 0;
            }
        };
        self.find.remember();
        let (purge, bookmark) = (self.find.purge, self.find.bookmark_lines);
        let text = self.cur_doc().full_text();
        let ranges = search::find_all(&re, &text);
        let d = self.cur_doc_mut();
        if purge {
            d.marks.clear();
        }
        for &m in &ranges {
            let (a, b) = to_chars(d, m);
            d.marks.push((a, b));
            if bookmark {
                let line = d.text.char_to_line(a);
                d.bookmarks.insert(line);
            }
        }
        d.marks.sort();
        d.marks.dedup();
        let n = ranges.len();
        self.find.say(format!("{n} Treffer markiert."), n == 0);
        n
    }

    /// Sucht im Hintergrund, damit das Fenster bedienbar bleibt.
    pub fn start_file_job(&mut self, replace: Option<String>) {
        if let Some(job) = self.find_job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        let q = self.find.query();
        let re = match q.compile() {
            Ok(r) => r,
            Err(e) => return self.find.say(e, true),
        };
        let dir = self.find.dir.trim().to_string();
        if !std::path::Path::new(&dir).is_dir() {
            return self.find.say("Dieser Ordner existiert nicht.", true);
        }
        self.find.remember();
        let (filters, sub, hidden) = (self.find.filters.clone(), self.find.subfolders, self.find.hidden);
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let replacing = replace.is_some();
        // Beim Suchen zählt in offenen Dateien der Text im Editor, auch wenn er ungespeichert ist.
        let unsaved: HashMap<PathBuf, String> = if replacing {
            HashMap::new()
        } else {
            self.docs
                .values()
                .filter(|d| d.is_modified())
                .filter_map(|d| Some((d.path.clone()?, d.full_text())))
                .collect()
        };
        self.results.begin(format!("Suche nach „{}“ in {dir}", self.find.what));
        self.find.say("Suche läuft …", false);
        std::thread::spawn(move || {
            let mut files = 0usize;
            let mut skipped = 0usize;
            for path in walk_files(&dir, &filters, sub, hidden) {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                files += 1;
                let open_text = (!unsaved.is_empty())
                    .then(|| std::fs::canonicalize(&path).ok().map(crate::app::strip_verbatim))
                    .flatten()
                    .and_then(|p| unsaved.get(&p).cloned());
                let l = match open_text {
                    Some(text) => fileio::Loaded { text, enc: fileio::Enc::Utf8, eol: Eol::Lf },
                    None => {
                        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                        // Im Hintergrund kann nicht nachgefragt werden: überspringen und in der Meldung zählen.
                        if crate::app::huge_warning_needed(size) {
                            skipped += 1;
                            continue;
                        }
                        let Ok(bytes) = std::fs::read(&path) else { continue };
                        // Beim Ersetzen bleiben Binärdateien unberührt, damit nichts zerstört wird.
                        if replace.is_some() && looks_binary(&bytes) {
                            continue;
                        }
                        fileio::decode(&bytes, Eol::Crlf)
                    }
                };
                let ranges = search::find_all(&re, &l.text);
                if ranges.is_empty() {
                    continue;
                }
                let hits = hits_in(&l.text, &ranges);
                if let Some(with) = &replace {
                    let (new, count) = search::replace_all(&q, &re, &l.text, with);
                    let (out, lossy) = fileio::encode(&new, l.enc, l.eol);
                    let result = if lossy {
                        Err("Zeichen passen nicht in die Kodierung der Datei".to_string())
                    } else {
                        fileio::write_atomic(&path, &out).map_err(|e| e.to_string())
                    };
                    match result {
                        Ok(()) => {
                            let _ = tx.send(JobMsg::Replaced { count });
                        }
                        Err(error) => {
                            let _ = tx.send(JobMsg::Failed { path: path.clone(), error });
                            continue;
                        }
                    }
                }
                let _ = tx.send(JobMsg::File { path, hits });
            }
            let _ = tx.send(JobMsg::Done { files, skipped });
        });
        self.find_job = Some(FindJob {
            rx,
            cancel,
            hits: 0,
            files_hit: 0,
            replacing,
            replaced: 0,
            failed: 0,
            first_error: None,
        });
    }

    pub fn poll_find_job(&mut self, ctx: &Context) {
        let Some(mut job) = self.find_job.take() else { return };
        let mut finished = None;
        for _ in 0..512 {
            match job.rx.try_recv() {
                Ok(JobMsg::File { path, hits }) => {
                    job.hits += hits.len();
                    job.files_hit += 1;
                    let name = path.display().to_string();
                    self.results.add_file(None, Some(path), name, hits);
                }
                Ok(JobMsg::Replaced { count }) => job.replaced += count,
                Ok(JobMsg::Failed { path, error }) => {
                    job.failed += 1;
                    job.first_error.get_or_insert(format!("{}: {error}", path.display()));
                }
                Ok(JobMsg::Done { files, skipped }) => finished = Some((files, skipped)),
                Err(_) => break,
            }
        }
        ctx.request_repaint();
        match finished {
            Some((files, skipped)) => {
                self.results.finish(job.hits, job.files_hit);
                // Mit Treffern schließt sich das Suchfenster und gibt den Blick auf die Liste frei.
                if job.hits > 0 && job.failed == 0 && !job.replacing {
                    self.find.open = false;
                }
                let mut msg = if job.replacing {
                    format!("{} Treffer in {} von {files} Dateien ersetzt.", job.replaced, job.files_hit)
                } else {
                    format!("{} Treffer in {} von {files} Dateien.", job.hits, job.files_hit)
                };
                if let Some(e) = &job.first_error {
                    msg.push_str(&format!(" {} Dateien nicht geändert: {e}", job.failed));
                }
                if skipped > 0 {
                    msg.push_str(&format!(" {skipped} Dateien ab 2 GB übersprungen."));
                }
                let err = job.hits == 0 || job.failed > 0;
                self.find.say(msg, err);
            }
            None => self.find_job = Some(job),
        }
    }

    pub fn draw_find(&mut self, ctx: &Context) {
        if !self.find.open {
            return;
        }
        let mut open = true;
        let mut do_next = false;
        let mut do_prev = false;
        let mut do_count = false;
        let mut do_all_cur = false;
        let mut do_all_open = false;
        let mut do_replace = false;
        let mut do_replace_all = false;
        let mut do_replace_open = false;
        let mut do_files = false;
        let mut do_files_replace = false;
        let mut do_mark = false;
        let mut do_clear_marks = false;
        let mut do_pick_dir = false;
        let mut do_cancel = false;
        let job_running = self.find_job.is_some();
        egui::Window::new(crate::theme::window_title(self.settings.theme, "Suchen"))
            .id(egui::Id::new("find-window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .default_pos(egui::pos2(220.0, 150.0))
            .show(ctx, |ui| {
                let f = &mut self.find;
                ui.horizontal(|ui| {
                    for (tab, label) in [
                        (FindTab::Find, "Suchen"),
                        (FindTab::Replace, "Ersetzen"),
                        (FindTab::Files, "In Dateien suchen"),
                        (FindTab::Mark, "Markieren"),
                    ] {
                        ui.selectable_value(&mut f.tab, tab, label);
                    }
                });
                ui.separator();
                ui.columns(2, |cols| {
                    let ui = &mut cols[0];
                    ui.set_min_width(330.0);
                    egui::Grid::new("find-fields").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                        ui.label("Suchen nach:");
                        let r = ui.add(egui::TextEdit::singleline(&mut f.what).desired_width(220.0).id(egui::Id::new("find-what")));
                        if f.focus_what {
                            f.focus_what = false;
                            r.request_focus();
                        }
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            match f.tab {
                                FindTab::Find | FindTab::Replace => do_next = true,
                                FindTab::Files => do_files = true,
                                FindTab::Mark => do_mark = true,
                            }
                            r.request_focus();
                        }
                        ui.end_row();
                        if !f.history.is_empty() {
                            ui.label("");
                            let mut picked = None;
                            egui::ComboBox::from_id_salt("find-history")
                                .selected_text("Letzte Suchen")
                                .width(220.0)
                                .show_ui(ui, |ui| {
                                    for h in &f.history {
                                        let shown: String = h.chars().take(40).collect();
                                        if ui.selectable_label(false, shown).clicked() {
                                            picked = Some(h.clone());
                                        }
                                    }
                                });
                            if let Some(h) = picked {
                                f.what = h;
                            }
                            ui.end_row();
                        }
                        if matches!(f.tab, FindTab::Replace | FindTab::Files) {
                            ui.label("Ersetzen durch:");
                            ui.add(egui::TextEdit::singleline(&mut f.with).desired_width(220.0));
                            ui.end_row();
                        }
                        if f.tab == FindTab::Files {
                            ui.label("Filter:");
                            ui.add(egui::TextEdit::singleline(&mut f.filters).desired_width(220.0).hint_text("*.txt;*.rs"));
                            ui.end_row();
                            ui.label("Ordner:");
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut f.dir).desired_width(186.0));
                                if ui.button("…").on_hover_text("Ordner wählen").clicked() {
                                    do_pick_dir = true;
                                }
                            });
                            ui.end_row();
                        }
                    });
                    ui.add_space(4.0);
                    ui.checkbox(&mut f.whole, "Nur ganze Wörter");
                    ui.checkbox(&mut f.case, "Groß-/Kleinschreibung beachten");
                    if f.tab == FindTab::Files {
                        ui.checkbox(&mut f.subfolders, "Unterordner einbeziehen");
                        ui.checkbox(&mut f.hidden, "Versteckte Ordner einbeziehen");
                    } else {
                        ui.checkbox(&mut f.wrap, "Am Ende von vorn beginnen");
                    }
                    if f.tab == FindTab::Replace {
                        ui.checkbox(&mut f.in_selection, "Nur in der Markierung");
                    }
                    if f.tab == FindTab::Mark {
                        ui.checkbox(&mut f.bookmark_lines, "Zeile mit Lesezeichen versehen");
                        ui.checkbox(&mut f.purge, "Vorherige Markierungen entfernen");
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new("Suchmodus").strong());
                    ui.radio_value(&mut f.mode, Mode::Normal, "Normal");
                    ui.radio_value(&mut f.mode, Mode::Extended, "Erweitert (\\n, \\t, \\0, \\x…)");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut f.mode, Mode::Regex, "Regulärer Ausdruck");
                        ui.add_enabled(f.mode == Mode::Regex, egui::Checkbox::new(&mut f.dot_nl, ". findet \\n"));
                    });
                    if matches!(f.tab, FindTab::Find | FindTab::Replace) {
                        ui.horizontal(|ui| {
                            ui.label("Richtung:");
                            ui.radio_value(&mut f.up, true, "Aufwärts");
                            ui.radio_value(&mut f.up, false, "Abwärts");
                        });
                    }
                    let ui = &mut cols[1];
                    let wide = |ui: &mut egui::Ui, text: &str| ui.add_sized([ui.available_width(), 24.0], egui::Button::new(text)).clicked();
                    match f.tab {
                        FindTab::Find => {
                            do_next |= wide(ui, "Weitersuchen");
                            do_prev |= wide(ui, "Rückwärts suchen");
                            do_count |= wide(ui, "Zählen");
                            do_all_cur |= wide(ui, "Alle im aktuellen Dokument");
                            do_all_open |= wide(ui, "In allen Tabs");
                        }
                        FindTab::Replace => {
                            do_next |= wide(ui, "Weitersuchen");
                            do_replace |= wide(ui, "Ersetzen");
                            do_replace_all |= wide(ui, "Alle ersetzen");
                            do_replace_open |= wide(ui, "In allen Tabs ersetzen");
                        }
                        FindTab::Files => {
                            do_files |= wide(ui, "Alle suchen");
                            do_files_replace |= wide(ui, "In Dateien ersetzen");
                            if job_running {
                                do_cancel |= wide(ui, "Abbrechen");
                            }
                        }
                        FindTab::Mark => {
                            do_mark |= wide(ui, "Alle markieren");
                            do_clear_marks |= wide(ui, "Markierungen entfernen");
                            do_next |= wide(ui, "Weitersuchen");
                        }
                    }
                });
                if !f.message.is_empty() {
                    ui.separator();
                    let color = if f.message_err { egui::Color32::from_rgb(0xC0, 0x30, 0x30) } else { ui.visuals().text_color() };
                    ui.colored_label(color, &f.message);
                }
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !self.editor_focused {
            open = false;
        }
        if !open {
            self.find.open = false;
            self.focus_editor();
        }
        if do_next {
            self.find_step(false);
        }
        if do_prev {
            self.find_step(true);
        }
        if do_count {
            self.count_matches();
        }
        if do_all_cur {
            self.find_all_docs(true);
        }
        if do_all_open {
            self.find_all_docs(false);
        }
        if do_replace {
            self.replace_one();
        }
        if do_replace_all {
            self.replace_all_current();
        }
        if do_replace_open {
            self.dialog = Some(Dialog::ConfirmReplaceOpen);
        }
        if do_files {
            self.start_file_job(None);
        }
        if do_files_replace {
            // Schreibt in Dateien auf der Festplatte: vorher nachfragen.
            if self.find.query().compile().is_ok() {
                let hint = format!(
                    "Alle Treffer von „{}“ durch „{}“ ersetzen, in allen passenden Dateien in\n{}?\nDas lässt sich nicht rückgängig machen.",
                    self.find.what, self.find.with, self.find.dir
                );
                self.dialog = Some(Dialog::ConfirmReplaceFiles { count_hint: hint });
            } else {
                self.start_file_job(None);
            }
        }
        if do_mark {
            self.mark_all();
        }
        if do_clear_marks {
            self.cur_doc_mut().marks.clear();
        }
        if do_pick_dir {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                self.find.dir = dir.display().to_string();
            }
        }
        if do_cancel {
            if let Some(job) = &self.find_job {
                job.cancel.store(true, Ordering::Relaxed);
            }
        }
    }
}

fn next_char(text: &str, at: usize) -> usize {
    let mut i = at + 1;
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}
