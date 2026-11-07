//! Suchen und Ersetzen: Suchleiste, Schritte im Dokument und die Suche über Dateien.
use crate::app::{App, Dialog};
use crate::doc::{DocId, Document, Kind, SelSet};
use crate::fileio::{self, Eol};
use crate::search::{self, Mode, Query};
use crate::settings::Settings;
use crate::ui_panels::Hit;
use crate::icons;
use crate::ui_chrome::chrome;
use crate::t;
use egui::{vec2, Context, RichText, TextEdit, Ui};
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

fn not_ansi_message() -> String {
    t!("find.not_ansi")
}

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
    /// Zwischengespeicherte Treffer für den Zähler in der Suchleiste.
    pub live: Option<LiveCount>,
}

/// Treffer im aktuellen Tab, gültig für genau diesen Suchbegriff, diese Optionen und diesen Textstand.
pub struct LiveCount {
    key: (String, Mode, bool, bool, bool, DocId, u64),
    matches: Vec<(usize, usize)>,
    error: Option<String>,
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
            live: None,
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
                let message = if wrapped { t!("find.wrapped") } else { String::new() };
                self.find.say(message, false);
                true
            }
            None => {
                self.find.say(t!("find.not_found", what = self.find.what), true);
                false
            }
        }
    }

    /// Ersetzt die aktuelle Markierung, falls sie ein Treffer ist, und springt zum nächsten.
    pub fn replace_one(&mut self) {
        self.sync_views();
        if self.cur_doc().read_only {
            self.find.say(t!("find.read_only"), true);
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
            return self.find.say(not_ansi_message(), true);
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
            return Err(not_ansi_message());
        }
        let with = self.find.with.clone();
        let doc = self.docs.get_mut(&id).ok_or_else(|| t!("find.doc_closed"))?;
        if doc.read_only {
            return Err(t!("find.replace_all_read_only"));
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
                self.find.say(t!("find.replaced", n = n), n == 0);
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
        self.find.say(t!("find.replaced_in_docs", n = total, docs = crate::ui_panels::docs_label(docs)), total == 0);
    }

    pub fn count_matches(&mut self) -> usize {
        match self.find.query().compile() {
            Ok(re) => {
                self.find.remember();
                let n = re.find_iter(&self.cur_doc().full_text()).count();
                self.find.say(t!("find.counted", n = n), n == 0);
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
        self.results.begin(t!("find.results_title", what = self.find.what));
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
        self.find.say(t!("find.found_in_docs", n = total, docs = crate::ui_panels::docs_label(files)), total == 0);
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
        self.find.say(t!("find.marked", n = n), n == 0);
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
            return self.find.say(t!("files.no_dir"), true);
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
        self.results.begin(t!("files.results_title", what = self.find.what, dir = dir));
        self.find.say(t!("files.running"), false);
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
                        Err(t!("files.lossy"))
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
                let mut msg = if job.replacing {
                    t!("files.replaced_summary", n = job.replaced, hit = job.files_hit, files = files)
                } else {
                    t!("files.summary", n = job.hits, hit = job.files_hit, files = files)
                };
                if let Some(e) = &job.first_error {
                    msg.push(' ');
                    msg.push_str(&t!("files.not_changed", n = job.failed, error = e));
                }
                if skipped > 0 {
                    msg.push(' ');
                    msg.push_str(&t!("files.skipped", n = skipped));
                }
                let err = job.hits == 0 || job.failed > 0;
                self.find.say(msg, err);
            }
            None => self.find_job = Some(job),
        }
    }

    /// Treffer im aktuellen Tab für die Anzeige „3 von 12“. Gezählt wird nur neu, wenn sich
    /// Suchbegriff, Optionen oder Text ändern; sehr große Dokumente werden nicht live gezählt.
    pub(crate) fn live_count(&mut self) -> Option<Result<(Option<usize>, usize), String>> {
        if self.find.what.is_empty() {
            return None;
        }
        let id = self.cur_id();
        let (rev, len) = {
            let d = self.cur_doc();
            (d.rev, d.len())
        };
        if len > LIVE_COUNT_MAX_CHARS {
            return None;
        }
        let f = &self.find;
        let key = (f.what.clone(), f.mode, f.case, f.whole, f.dot_nl, id, rev);
        if self.find.live.as_ref().map(|l| &l.key) != Some(&key) {
            let (matches, error) = match self.find.query().compile() {
                Ok(re) => (search::find_all(&re, &self.cur_doc().full_text()), None),
                Err(e) => (Vec::new(), Some(e)),
            };
            self.find.live = Some(LiveCount { key, matches, error });
        }
        let live = self.find.live.as_ref()?;
        if let Some(e) = &live.error {
            return Some(Err(e.clone()));
        }
        let main = self.cur_view().sels.main();
        let d = self.cur_doc();
        let sel = (d.text.char_to_byte(main.start()), d.text.char_to_byte(main.end()));
        let cur = live.matches.iter().position(|m| *m == sel).map(|i| i + 1);
        Some(Ok((cur, live.matches.len())))
    }

    /// Suchleiste oder Seitenleiste „In Dateien suchen“. Muss vor dem Schreibfeld gezeichnet werden.
    pub fn draw_find(&mut self, ctx: &Context) {
        if !self.find.open {
            return;
        }
        let acts = if self.find.tab == FindTab::Files { self.files_panel(ctx) } else { self.find_bar(ctx) };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !self.editor_focused {
            self.find.open = false;
            self.focus_editor();
        }
        for a in acts {
            match a {
                FindAct::Next => {
                    self.find_step(false);
                }
                FindAct::Prev => {
                    self.find_step(true);
                }
                FindAct::Count => {
                    self.count_matches();
                }
                FindAct::Replace => self.replace_one(),
                FindAct::ReplaceAll => self.replace_all_current(),
                FindAct::ReplaceOpen => self.dialog = Some(Dialog::ConfirmReplaceOpen),
                FindAct::ListCurrent => self.find_all_docs(true),
                FindAct::ListOpen => self.find_all_docs(false),
                FindAct::MarkAll => {
                    self.mark_all();
                }
                FindAct::ClearMarks => self.cur_doc_mut().marks.clear(),
                FindAct::ShowFiles => self.show_find(FindTab::Files),
                FindAct::FilesFind => self.start_file_job(None),
                FindAct::FilesReplace => {
                    // Schreibt in Dateien auf der Festplatte: vorher nachfragen.
                    if self.find.query().compile().is_ok() {
                        let hint = t!("files.confirm_replace", what = self.find.what, with = self.find.with, dir = self.find.dir);
                        self.dialog = Some(Dialog::ConfirmReplaceFiles { count_hint: hint });
                    } else {
                        self.start_file_job(None);
                    }
                }
                FindAct::PickDir => {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.find.dir = dir.display().to_string();
                    }
                }
                FindAct::Cancel => {
                    if let Some(job) = &self.find_job {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                }
                FindAct::Close => {
                    self.find.open = false;
                    self.focus_editor();
                }
            }
        }
    }

    /// Schmale Leiste über dem Schreibfeld: Suchfeld mit Schaltern, Zähler, Pfeile, Optionen.
    /// Darunter je nach Modus eine Zeile zum Ersetzen oder zum Hervorheben.
    fn find_bar(&mut self, ctx: &Context) -> Vec<FindAct> {
        let c = chrome(self.settings.theme);
        let live = self.live_count();
        let mut act = Vec::new();
        egui::TopBottomPanel::top("findbar")
            .frame(egui::Frame::NONE.fill(c.window).inner_margin(egui::Margin { left: 6, right: 8, top: 4, bottom: 4 }))
            .show(ctx, |ui| {
                let f = &mut self.find;
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                let field_w = 280.0;
                ui.horizontal(|ui| {
                    let (glyph, fallback, tip) = if f.tab == FindTab::Find {
                        (icons::GLYPH_CHEVRON_RIGHT, "▶", t!("find.show_replace"))
                    } else {
                        (icons::GLYPH_CHEVRON_DOWN, "▼", t!("find.hide_replace"))
                    };
                    if icon_button(ui, glyph, fallback, &tip) {
                        f.tab = if f.tab == FindTab::Find { FindTab::Replace } else { FindTab::Find };
                    }
                    let r = ui.add(TextEdit::singleline(&mut f.what).desired_width(field_w).hint_text(t!("find.hint")).id(egui::Id::new("find-what")));
                    if f.focus_what {
                        f.focus_what = false;
                        r.request_focus();
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        act.push(if ui.input(|i| i.modifiers.shift) { FindAct::Prev } else { FindAct::Next });
                        r.request_focus();
                    }
                    option_toggles(ui, f);
                    let (text, err) = match &live {
                        None => (String::new(), false),
                        Some(Ok((Some(i), n))) => (t!("find.position", i = i, n = n), false),
                        Some(Ok((None, 0))) => (t!("find.no_matches"), true),
                        Some(Ok((None, n))) => (t!("find.matches", n = n), false),
                        Some(Err(_)) => (t!("find.invalid"), true),
                    };
                    let color = if err { ERROR_RED } else { c.text_dim };
                    let r = ui.add_sized([110.0, 20.0], egui::Label::new(RichText::new(text).color(color)));
                    if let Some(Err(e)) = &live {
                        r.on_hover_text(e);
                    }
                    if icon_button(ui, GLYPH_ARROW_UP, "↑", &t!("find.prev")) {
                        act.push(FindAct::Prev);
                    }
                    if icon_button(ui, GLYPH_ARROW_DOWN, "↓", &t!("find.next")) {
                        act.push(FindAct::Next);
                    }
                    ui.menu_button(icons::rich(GLYPH_MORE, "⋯", 14.0), |ui| {
                        ui.set_min_width(260.0);
                        ui.checkbox(&mut f.wrap, t!("find.wrap"));
                        ui.checkbox(&mut f.in_selection, t!("find.in_selection"));
                        ui.add_enabled(f.mode == Mode::Regex, egui::Checkbox::new(&mut f.dot_nl, t!("find.dot_nl")));
                        ui.separator();
                        if ui.button(t!("find.count")).on_hover_text(t!("find.count_tip")).clicked() {
                            act.push(FindAct::Count);
                            ui.close_menu();
                        }
                        if ui.button(t!("find.list_current")).clicked() {
                            act.push(FindAct::ListCurrent);
                            ui.close_menu();
                        }
                        if ui.button(t!("find.list_open")).clicked() {
                            act.push(FindAct::ListOpen);
                            ui.close_menu();
                        }
                        if ui.button(t!("find.mark_menu")).clicked() {
                            f.tab = FindTab::Mark;
                            ui.close_menu();
                        }
                        if ui.button(t!("find.files_menu")).clicked() {
                            act.push(FindAct::ShowFiles);
                            ui.close_menu();
                        }
                        if !f.history.is_empty() {
                            ui.separator();
                            ui.menu_button(t!("find.history"), |ui| {
                                ui.set_min_width(200.0);
                                let mut picked = None;
                                for h in &f.history {
                                    let shown: String = h.chars().take(40).collect();
                                    if ui.button(shown).clicked() {
                                        picked = Some(h.clone());
                                        ui.close_menu();
                                    }
                                }
                                if let Some(h) = picked {
                                    f.what = h;
                                    f.focus_what = true;
                                }
                            });
                        }
                    })
                    .response
                    .on_hover_text(t!("find.more"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, icons::GLYPH_CLOSE, "✕", &t!("find.close")) {
                            act.push(FindAct::Close);
                        }
                        if !f.message.is_empty() {
                            let color = if f.message_err { ERROR_RED } else { c.text };
                            ui.add(egui::Label::new(RichText::new(&f.message).color(color)).truncate());
                        }
                    });
                });
                // Zweite Zeile, eingerückt unter das Suchfeld.
                let indent = 28.0;
                match f.tab {
                    FindTab::Replace => {
                        ui.horizontal(|ui| {
                            ui.add_space(indent);
                            let r = ui.add(TextEdit::singleline(&mut f.with).desired_width(field_w).hint_text(t!("find.replace_hint")));
                            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                act.push(FindAct::Replace);
                                r.request_focus();
                            }
                            if ui.button(t!("find.replace")).on_hover_text(t!("find.replace_tip")).clicked() {
                                act.push(FindAct::Replace);
                            }
                            if ui.button(t!("find.replace_all")).clicked() {
                                act.push(FindAct::ReplaceAll);
                            }
                            if ui.button(t!("find.replace_open")).on_hover_text(t!("find.replace_open_tip")).clicked() {
                                act.push(FindAct::ReplaceOpen);
                            }
                            ui.checkbox(&mut f.in_selection, t!("find.in_selection"));
                        });
                    }
                    FindTab::Mark => {
                        ui.horizontal(|ui| {
                            ui.add_space(indent);
                            if ui.button(t!("find.mark_all")).clicked() {
                                act.push(FindAct::MarkAll);
                            }
                            ui.checkbox(&mut f.bookmark_lines, t!("find.bookmark_lines"));
                            ui.checkbox(&mut f.purge, t!("find.purge"));
                            if ui.button(t!("find.clear_marks")).clicked() {
                                act.push(FindAct::ClearMarks);
                            }
                        });
                    }
                    _ => {}
                }
            });
        act
    }

    /// Seitenleiste für die Suche über einen Ordner; die Treffer erscheinen in der Ergebnisliste unten.
    fn files_panel(&mut self, ctx: &Context) -> Vec<FindAct> {
        let c = chrome(self.settings.theme);
        let job_running = self.find_job.is_some();
        let mut act = Vec::new();
        egui::SidePanel::right("findfiles")
            .resizable(true)
            .default_width(320.0)
            .min_width(250.0)
            .max_width(560.0)
            .frame(egui::Frame::NONE.fill(c.window).inner_margin(10))
            .show(ctx, |ui| {
                let f = &mut self.find;
                ui.spacing_mut().item_spacing = vec2(4.0, 5.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t!("files.title")).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, icons::GLYPH_CLOSE, "✕", &t!("find.close")) {
                            act.push(FindAct::Close);
                        }
                    });
                });
                ui.add_space(4.0);
                // Feste Größen: Felder, die sich nach der verfügbaren Breite richten, würden die Leiste aufschieben.
                let w = ui.available_width();
                let h = 22.0;
                let r = ui.add_sized([w, h], TextEdit::singleline(&mut f.what).hint_text(t!("find.hint")).id(egui::Id::new("files-what")));
                if f.focus_what {
                    f.focus_what = false;
                    r.request_focus();
                }
                let enter = |ui: &Ui, r: &egui::Response| r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if enter(ui, &r) && !job_running {
                    act.push(FindAct::FilesFind);
                }
                ui.horizontal(|ui| option_toggles(ui, f));
                ui.add_sized([w, h], TextEdit::singleline(&mut f.with).hint_text(t!("find.replace_hint")));
                ui.add_space(6.0);
                ui.label(RichText::new(t!("files.folder")).small().color(c.text_dim));
                ui.horizontal(|ui| {
                    let r = ui.add_sized([w - 32.0, h], TextEdit::singleline(&mut f.dir));
                    if enter(ui, &r) && !job_running {
                        act.push(FindAct::FilesFind);
                    }
                    if ui.button("…").on_hover_text(t!("files.pick_dir")).clicked() {
                        act.push(FindAct::PickDir);
                    }
                });
                ui.label(RichText::new(t!("files.files")).small().color(c.text_dim));
                let r = ui
                    .add_sized([w, h], TextEdit::singleline(&mut f.filters).hint_text("*.rs *.toml"))
                    .on_hover_text(t!("files.filters_tip"));
                if enter(ui, &r) && !job_running {
                    act.push(FindAct::FilesFind);
                }
                ui.checkbox(&mut f.subfolders, t!("files.subfolders"));
                ui.checkbox(&mut f.hidden, t!("files.hidden"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!job_running, egui::Button::new(t!("files.find"))).clicked() {
                        act.push(FindAct::FilesFind);
                    }
                    if ui.add_enabled(!job_running, egui::Button::new(t!("files.replace"))).on_hover_text(t!("files.replace_tip")).clicked() {
                        act.push(FindAct::FilesReplace);
                    }
                    if job_running && ui.button(t!("files.cancel")).clicked() {
                        act.push(FindAct::Cancel);
                    }
                });
                if job_running {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(t!("files.running"));
                    });
                }
                if !f.message.is_empty() {
                    ui.add_space(6.0);
                    let color = if f.message_err { ERROR_RED } else { c.text };
                    ui.add(egui::Label::new(RichText::new(&f.message).color(color)).wrap());
                }
            });
        act
    }
}

/// Was in Suchleiste oder Seitenleiste angeklickt wurde; ausgeführt nach dem Zeichnen.
enum FindAct {
    Next,
    Prev,
    Count,
    Replace,
    ReplaceAll,
    ReplaceOpen,
    ListCurrent,
    ListOpen,
    MarkAll,
    ClearMarks,
    ShowFiles,
    FilesFind,
    FilesReplace,
    PickDir,
    Cancel,
    Close,
}

const ERROR_RED: egui::Color32 = egui::Color32::from_rgb(0xD0, 0x40, 0x40);
const GLYPH_ARROW_UP: char = '\u{E74A}';
const GLYPH_ARROW_DOWN: char = '\u{E74B}';
const GLYPH_MORE: char = '\u{E712}';
/// Ab dieser Länge zählt die Suchleiste nicht mehr bei jeder Änderung mit.
const LIVE_COUNT_MAX_CHARS: usize = 3_000_000;

/// Kleiner Knopf mit Segoe-Symbol (oder Unicode-Ersatz), ohne Rahmen.
fn icon_button(ui: &mut Ui, glyph: char, fallback: &str, tip: &str) -> bool {
    ui.add(egui::Button::new(icons::rich(glyph, fallback, 12.0)).frame(false).min_size(vec2(24.0, 22.0)))
        .on_hover_text(tip)
        .clicked()
}

/// Schalter für Groß/Klein, ganze Wörter, reguläre Ausdrücke und Steuerzeichen.
fn option_toggles(ui: &mut Ui, f: &mut FindUi) {
    let mono = |s: &str| RichText::new(s).monospace().size(12.0);
    ui.toggle_value(&mut f.case, mono("Aa")).on_hover_text(t!("find.case_tip"));
    ui.toggle_value(&mut f.whole, mono("ab").underline()).on_hover_text(t!("find.whole_tip"));
    let mut regex = f.mode == Mode::Regex;
    if ui.toggle_value(&mut regex, mono(".*")).on_hover_text(t!("find.regex_tip")).changed() {
        f.mode = if regex { Mode::Regex } else { Mode::Normal };
    }
    let mut escapes = f.mode == Mode::Extended;
    if ui
        .toggle_value(&mut escapes, mono("\\n"))
        .on_hover_text(t!("find.escapes_tip"))
        .changed()
    {
        f.mode = if escapes { Mode::Extended } else { Mode::Normal };
    }
}

fn next_char(text: &str, at: usize) -> usize {
    let mut i = at + 1;
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}
