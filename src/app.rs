//! Programmzustand und alle Aktionen, die Menüs, Tasten und Werkzeugleiste auslösen.
use crate::commands::{Cmd, Ed};
use crate::doc::{DocId, Document, Kind, SelSet};
use crate::editor::{self, Metrics, Palette, View};
use crate::fileio::{self, Enc, Eol};
use crate::highlight;
use crate::settings::{Session, SessionTab, Settings, Storage};
use crate::textops;
use crate::ui_find::{FindJob, FindUi};
use crate::ui_panels::Results;
use egui::Context;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const APP_NAME: &str = "Quillpad";
/// Einträge unter „Zuletzt geöffnet“.
pub const RECENT_MAX: usize = 10;
/// Abstand der automatischen Sicherung.
pub const BACKUP_INTERVAL_MS: u64 = 5000;

/// Warnung vor riesigen Dateien: ab 2 GiB wird das Öffnen langsam und braucht viel Speicher.
pub fn huge_warning_needed(size: u64) -> bool {
    size >= 2 << 30
}

pub struct Pane {
    pub tabs: Vec<View>,
    pub active: usize,
    pub scroll: f32,
}

impl Pane {
    fn new() -> Pane {
        Pane { tabs: Vec::new(), active: 0, scroll: 0.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashKind {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

#[derive(Clone, Debug)]
pub enum Action {
    Cmd(Cmd),
    Paste,
    New,
    Open,
    OpenPath(PathBuf),
    Reload,
    Save,
    SaveAs,
    SaveCopy,
    SaveAll,
    Rename,
    Close,
    CloseAll,
    CloseOthers,
    CloseTab(usize, usize),
    CloseOthersOf(usize, usize),
    ActivateTab(usize, usize),
    NextTab,
    PrevTab,
    Print,
    Exit,
    OpenExplorer,
    OpenTerminal,
    CopyFullPath,
    CopyFileName,
    CopyDirPath,
    ToggleReadOnly,
    ShowFind(crate::ui_find::FindTab),
    FindNext,
    FindPrev,
    FindSelectedNext,
    GoToDialog,
    MatchBrace,
    BookmarkToggle,
    BookmarkNext,
    BookmarkPrev,
    BookmarksClear,
    BookmarksInvert,
    BookmarksCopyLines,
    BookmarksDeleteLines,
    ClearMarks,
    ToggleWrap,
    ToggleWs,
    ToggleEol,
    ToggleAllSymbols,
    ToggleGuides,
    ToggleLineNumbers,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    FoldAll,
    UnfoldAll,
    FoldCurrent,
    ToggleOnTop,
    ToggleFullscreen,
    MoveToOther,
    CloneToOther,
    ToggleSyncV,
    ToggleSyncH,
    ToggleMap,
    ToggleFuncs,
    ToggleFolder,
    PickWorkspace,
    ToggleMonitor,
    Summary,
    Encode(Enc),
    Convert(Enc),
    SetEol(Eol),
    SetLanguage(Option<String>),
    Preferences,
    SetTheme(crate::theme::ThemeId),
    CheckUpdates,
    Shortcuts,
    About,
    ColumnEditor,
    InsertDateTime,
    Autocomplete,
    Hash(HashKind),
}

pub enum Dialog {
    Message { title: String, text: String },
    ConfirmClose { doc: DocId },
    GoTo { input: String, by_offset: bool, first: bool },
    About,
    Summary { text: String },
    Prefs,
    Shortcuts,
    Column { text: String, numbers: bool, start: String, step: String, repeat: String, leading: u8, format: u8 },
    ConfirmReplaceOpen,
    Hash { title: String, value: String },
    ReloadAsk { doc: DocId },
    DeletedAsk { doc: DocId },
    Rename { name: String, first: bool },
    LossySave { doc: DocId, path: PathBuf },
    ConfirmReplaceFiles { count_hint: String },
    HugeOpen { path: PathBuf },
    Update { release: crate::update::Release, auto: bool },
}

pub struct Shot {
    pub path: PathBuf,
    pub frame: u32,
    pub sent: bool,
}

impl Shot {
    pub fn new(path: PathBuf) -> Shot {
        Shot { path, frame: 0, sent: false }
    }
}

pub struct App {
    pub ctx: Context,
    pub storage: Option<Storage>,
    pub settings: Settings,
    pub docs: HashMap<DocId, Document>,
    next_doc: DocId,
    pub panes: [Pane; 2],
    pub active_pane: usize,
    pub split_frac: f32,
    pub metrics: Option<Metrics>,
    pub rebuild_metrics: bool,
    pub style_dirty: bool,
    /// Ob die Oberflächenschrift gerade die des klassischen Looks ist.
    fonts_retro: bool,
    pub pending: Vec<Action>,
    pub find: FindUi,
    pub find_job: Option<FindJob>,
    pub results: Results,
    pub dialog: Option<Dialog>,
    pub closing: Vec<DocId>,
    pub exit_after_close: bool,
    pub shot: Option<Shot>,
    pub editor_focused: bool,
    pub show_map: bool,
    pub show_funcs: bool,
    pub show_folder: bool,
    pub folder_cache: HashMap<PathBuf, Vec<(String, bool, PathBuf)>>,
    pub func_cache: Option<(DocId, u64, Vec<(String, usize)>)>,
    pub sync_v: bool,
    pub sync_h: bool,
    pub fullscreen: bool,
    pub on_top: bool,
    pub tab_drag: Option<(usize, usize)>,
    pub close_confirmed: bool,
    pub menu_width: f32,
    pub inbox: Option<std::sync::mpsc::Receiver<Vec<PathBuf>>>,
    /// Laufende Suche nach Updates; true = vom Menü ausgelöst, dann gibt es immer eine Antwort.
    update_rx: Option<(std::sync::mpsc::Receiver<crate::update::Outcome>, bool)>,
    /// Gefundene neuere Version, die gezeigt wird, sobald kein anderer Dialog offen ist.
    update_found: Option<crate::update::Release>,
    pub test_clicks: Vec<egui::Pos2>,
    last_title: String,
    last_backup: Instant,
    last_watch: Instant,
    session_dirty_probe: u64,
}

impl App {
    pub fn new(ctx: Context, storage: Option<Storage>, settings: Settings, files: Vec<PathBuf>) -> App {
        let retro = settings.theme.retro();
        crate::ui_chrome::setup_fonts(&ctx, &settings.font, retro);
        let mut settings = settings;
        // Eine ungültige Tabbreite fällt auf den Standard zurück.
        if settings.ed.tab_size == 0 {
            settings.ed.tab_size = 4;
        }
        let mut app = App {
            ctx,
            storage,
            find: FindUi::new(&settings),
            settings,
            docs: HashMap::new(),
            next_doc: 1,
            panes: [Pane::new(), Pane::new()],
            active_pane: 0,
            split_frac: 0.5,
            metrics: None,
            rebuild_metrics: false,
            style_dirty: true,
            fonts_retro: retro,
            pending: Vec::new(),
            find_job: None,
            results: Results::default(),
            dialog: None,
            closing: Vec::new(),
            exit_after_close: false,
            shot: None,
            editor_focused: true,
            show_map: false,
            show_funcs: false,
            show_folder: false,
            folder_cache: HashMap::new(),
            func_cache: None,
            sync_v: false,
            sync_h: false,
            fullscreen: false,
            on_top: false,
            tab_drag: None,
            close_confirmed: false,
            menu_width: 0.0,
            inbox: None,
            test_clicks: Vec::new(),
            last_title: String::new(),
            last_backup: Instant::now(),
            last_watch: Instant::now(),
            session_dirty_probe: 0,
            update_rx: None,
            update_found: None,
        };
        app.restore_session();
        for f in files {
            app.open_path(&f);
        }
        if app.panes[0].tabs.is_empty() {
            app.new_doc();
        }
        app.show_folder = app.settings.workspace.is_some();
        // Nur im echten Betrieb (mit Einstellungsordner), nicht in Tests und Bildschirmfotos.
        let due = crate::update::now_secs() - app.settings.last_update_check >= crate::update::AUTO_INTERVAL_SECS;
        if app.storage.is_some() && app.settings.update_check && due {
            app.start_update_check(false);
        }
        app
    }

    /// Zustände für automatische Bilder; im normalen Betrieb ungenutzt.
    pub fn apply_test_state(&mut self, state: &str) {
        for part in state.split(',') {
            match part {
                "dark" => self.settings.theme = crate::theme::ThemeId::Dark,
                t if t.starts_with("lang=") => {
                    if let Some(l) = crate::i18n::Lang::from_locale(&t[5..]) {
                        self.settings.language = Some(l);
                        crate::i18n::set_lang(l);
                    }
                }
                t if t.starts_with("theme=") => {
                    if let Some(id) = crate::theme::ThemeId::from_key(&t[6..]) {
                        self.settings.theme = id;
                    }
                }
                "wrap" => self.settings.ed.wrap = true,
                "symbols" => {
                    self.settings.ed.show_ws = true;
                    self.settings.ed.show_eol = true;
                }
                "find" => self.pending.push(Action::ShowFind(crate::ui_find::FindTab::Find)),
                "replace" => self.pending.push(Action::ShowFind(crate::ui_find::FindTab::Replace)),
                "files" => self.pending.push(Action::ShowFind(crate::ui_find::FindTab::Files)),
                "map" => self.show_map = true,
                "funcs" => self.show_funcs = true,
                "folder" => {
                    self.show_folder = true;
                    if self.settings.workspace.is_none() {
                        self.settings.workspace = std::env::current_dir().ok().map(|p| p.display().to_string());
                    }
                }
                "split" => self.pending.push(Action::CloneToOther),
                "results" => {
                    self.find.what = "map".into();
                    self.find_all_docs(false);
                }
                other if other.starts_with("click=") => {
                    let mut it = other[6..].split('x').filter_map(|n| n.parse::<f32>().ok());
                    if let (Some(x), Some(y)) = (it.next(), it.next()) {
                        self.test_clicks.push(egui::pos2(x, y));
                    }
                }
                "prefs" => self.dialog = Some(Dialog::Prefs),
                "about" => self.dialog = Some(Dialog::About),
                "shortcuts" => self.dialog = Some(Dialog::Shortcuts),
                "goto" => self.pending.push(Action::GoToDialog),
                "column" => self.pending.push(Action::ColumnEditor),
                _ => {}
            }
        }
        self.style_dirty = true;
    }

    pub fn alloc_doc(&mut self, title: String, text: &str, enc: Enc, eol: Eol) -> DocId {
        let id = self.next_doc;
        self.next_doc += 1;
        self.docs.insert(id, Document::new(id, title, text, enc, eol));
        id
    }

    pub fn new_doc(&mut self) -> DocId {
        let used: HashSet<String> = self.docs.values().filter(|d| d.path.is_none()).map(|d| d.title.clone()).collect();
        let mut n = 1;
        while used.contains(&crate::t!("common.untitled", n = n)) {
            n += 1;
        }
        let enc = Enc::from_name(&self.settings.default_enc);
        let eol = Eol::from_name(&self.settings.default_eol);
        let id = self.alloc_doc(crate::t!("common.untitled", n = n), "", enc, eol);
        let p = self.active_pane;
        self.add_view(p, id);
        id
    }

    fn add_view(&mut self, pane: usize, doc: DocId) {
        let rev = self.docs[&doc].rev;
        let p = &mut self.panes[pane];
        let at = if p.tabs.is_empty() { 0 } else { (p.active + 1).min(p.tabs.len()) };
        p.tabs.insert(at, View::new(doc, rev));
        p.active = at;
        self.active_pane = pane;
    }

    pub fn is_split(&self) -> bool {
        !self.panes[1].tabs.is_empty()
    }

    /// Auswahlen aller Ansichten an den aktuellen Text anpassen.
    pub fn sync_views(&mut self) {
        for p in self.panes.iter_mut() {
            for v in p.tabs.iter_mut() {
                if let Some(d) = self.docs.get(&v.doc) {
                    v.sync(d);
                }
            }
        }
    }

    pub fn cur_view(&self) -> &View {
        let p = &self.panes[self.active_pane];
        &p.tabs[p.active.min(p.tabs.len() - 1)]
    }

    pub fn cur_view_mut(&mut self) -> &mut View {
        let p = &mut self.panes[self.active_pane];
        let i = p.active.min(p.tabs.len() - 1);
        &mut p.tabs[i]
    }

    pub fn cur_id(&self) -> DocId {
        self.cur_view().doc
    }

    pub fn cur_doc(&self) -> &Document {
        &self.docs[&self.cur_id()]
    }

    pub fn cur_doc_mut(&mut self) -> &mut Document {
        let id = self.cur_id();
        self.docs.get_mut(&id).expect("Dokument offen")
    }

    /// Arbeitszugriff auf aktives Dokument und aktive Ansicht.
    pub fn ed(&mut self) -> Option<Ed<'_>> {
        let p = &mut self.panes[self.active_pane];
        let i = p.active.min(p.tabs.len().checked_sub(1)?);
        let v = &mut p.tabs[i];
        let doc = self.docs.get_mut(&v.doc)?;
        v.sync(doc);
        Some(Ed { doc, v, o: &self.settings.ed, m: self.metrics.as_mut()? })
    }

    pub fn palette(&self) -> Palette {
        crate::theme::palette(self.settings.theme)
    }

    pub fn apply_cmd(&mut self, cmd: Cmd) {
        let ctx = self.ctx.clone();
        if let Some(mut ed) = self.ed() {
            if let Some(clip) = ed.apply(cmd) {
                ctx.copy_text(clip);
            }
        }
    }

    pub fn message(&mut self, title: &str, text: impl Into<String>) {
        self.dialog = Some(Dialog::Message { title: title.into(), text: text.into() });
    }

    /// Startet die Suche nach einer neueren Version im Hintergrund.
    pub fn start_update_check(&mut self, manual: bool) {
        if self.update_rx.is_some() {
            return;
        }
        self.settings.last_update_check = crate::update::now_secs();
        self.update_rx = Some((crate::update::check_in_background(self.ctx.clone()), manual));
    }

    fn poll_update(&mut self) {
        use crate::update::Outcome;
        use std::sync::mpsc::TryRecvError;
        if let Some((rx, manual)) = &self.update_rx {
            let manual = *manual;
            match rx.try_recv() {
                Ok(outcome) => {
                    self.update_rx = None;
                    match outcome {
                        Outcome::Newer(r) => self.update_found = Some(r),
                        Outcome::UpToDate if manual => {
                            self.message("Nach Updates suchen", format!("Quillpad {} ist die neueste Version.", crate::update::VERSION));
                        }
                        Outcome::Failed(e) if manual => {
                            self.message("Nach Updates suchen", format!("Die Suche nach Updates hat nicht geklappt.\n{e}"));
                        }
                        // Automatische Suche: ohne Neuigkeit kein Dialog.
                        _ => {}
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.update_rx = None,
            }
        }
        if self.dialog.is_none() {
            if let Some(release) = self.update_found.take() {
                self.dialog = Some(Dialog::Update { release, auto: self.settings.update_check });
            }
        }
    }

    pub fn focus_editor(&mut self) {
        self.cur_view_mut().want_focus = true;
    }

    // ---------- Dateien ----------

    pub fn activate_doc(&mut self, id: DocId) -> bool {
        for pane in 0..2 {
            if let Some(i) = self.panes[pane].tabs.iter().position(|v| v.doc == id) {
                self.panes[pane].active = i;
                self.active_pane = pane;
                self.focus_editor();
                return true;
            }
        }
        false
    }

    /// Ab dieser Größe gelten die Einschränkungen für große Dateien.
    pub fn is_large(&self, size: u64) -> bool {
        self.settings.large_file_enabled && size >= self.settings.large_file_mb.clamp(1, 4096) << 20
    }

    /// Öffnet eine Datei. Ein leeres, unverändertes „neu N“ wird dabei ersetzt.
    pub fn open_path(&mut self, path: &Path) -> Option<DocId> {
        self.open_path_checked(path, false)
    }

    pub fn open_path_checked(&mut self, path: &Path, confirmed_huge: bool) -> Option<DocId> {
        let full = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let full = strip_verbatim(full);
        if let Some(id) = self.docs.values().find(|d| d.path.as_deref() == Some(full.as_path())).map(|d| d.id) {
            self.activate_doc(id);
            return Some(id);
        }
        let size = std::fs::metadata(&full).map(|m| m.len()).unwrap_or(0);
        if !confirmed_huge && !self.settings.suppress_2gb_warning && huge_warning_needed(size) {
            self.dialog = Some(Dialog::HugeOpen { path: full });
            return None;
        }
        let bytes = match std::fs::read(&full) {
            Ok(b) => b,
            Err(e) => {
                self.message("Datei öffnen", format!("„{}“ konnte nicht geöffnet werden:\n{e}", full.display()));
                return None;
            }
        };
        let large = self.is_large(size);
        if large {
            // Große Dateien schalten den Zeilenumbruch der Ansicht ab.
            self.settings.ed.wrap = false;
        }
        let l = fileio::decode(&bytes, Eol::from_name(&self.settings.default_eol));
        let name = full.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let id = self.alloc_doc(name.clone(), &l.text, l.enc, l.eol);
        // Große Dateien bleiben ohne Farben.
        let lang = if large { None } else { highlight::detect(&name, &l.text) };
        {
            let d = self.docs.get_mut(&id).expect("eben angelegt");
            d.path = Some(full.clone());
            d.disk_mtime = mtime(&full);
            d.large = large;
            d.hl.set_language(lang);
        }
        let pane = self.active_pane;
        let replace_blank = {
            let tabs = &self.panes[pane].tabs;
            tabs.len() == 1 && {
                let d = &self.docs[&tabs[0].doc];
                d.path.is_none() && d.len() == 0 && !d.is_modified()
            }
        };
        if replace_blank {
            let old = self.panes[pane].tabs.remove(0).doc;
            self.panes[pane].active = 0;
            if !self.panes.iter().any(|p| p.tabs.iter().any(|v| v.doc == old)) {
                self.docs.remove(&old);
            }
        }
        self.add_view(pane, id);
        self.add_recent(&full);
        Some(id)
    }

    fn add_recent(&mut self, p: &Path) {
        let s = p.display().to_string();
        self.settings.recent.retain(|x| *x != s);
        self.settings.recent.insert(0, s);
        self.settings.recent.truncate(RECENT_MAX);
    }

    /// Schreibt ein Dokument. false, wenn nicht gespeichert wurde.
    pub fn write_doc(&mut self, id: DocId, path: &Path, as_copy: bool) -> bool {
        let Some(doc) = self.docs.get(&id) else { return false };
        let (bytes, lossy) = fileio::encode(&doc.full_text(), doc.enc, doc.eol);
        if lossy {
            self.dialog = Some(Dialog::LossySave { doc: id, path: path.to_path_buf() });
            return false;
        }
        if let Err(e) = fileio::write_atomic(path, &bytes) {
            self.message("Speichern", format!("„{}“ konnte nicht gespeichert werden:\n{e}", path.display()));
            return false;
        }
        if as_copy {
            return true;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let doc = self.docs.get_mut(&id).expect("oben geprüft");
        let path_changed = doc.path.as_deref() != Some(path);
        doc.path = Some(path.to_path_buf());
        doc.disk_mtime = mtime(path);
        doc.mark_saved();
        if path_changed && doc.hl.language.is_none() {
            let head = doc.slice(0, doc.len().min(200));
            doc.hl.set_language(highlight::detect(&name, &head));
        }
        self.add_recent(path);
        true
    }

    pub fn save_doc(&mut self, id: DocId) -> bool {
        match self.docs.get(&id).and_then(|d| d.path.clone()) {
            Some(p) => self.write_doc(id, &p, false),
            None => self.save_as(id),
        }
    }

    fn save_as(&mut self, id: DocId) -> bool {
        let Some(doc) = self.docs.get(&id) else { return false };
        let mut dlg = rfd::FileDialog::new().set_file_name(doc.name());
        if let Some(dir) = doc.path.as_ref().and_then(|p| p.parent()) {
            dlg = dlg.set_directory(dir);
        }
        match dlg.save_file() {
            Some(p) => self.write_doc(id, &p, false),
            None => false,
        }
    }

    /// Lädt von der Festplatte neu.
    pub fn reload_doc(&mut self, id: DocId) {
        let Some(path) = self.docs.get(&id).and_then(|d| d.path.clone()) else { return };
        match std::fs::read(&path) {
            Ok(bytes) => {
                let l = fileio::decode(&bytes, Eol::from_name(&self.settings.default_eol));
                let d = self.docs.get_mut(&id).expect("oben geprüft");
                d.reset_text(&l.text);
                d.enc = l.enc;
                d.eol = l.eol;
                d.disk_mtime = mtime(&path);
                for p in self.panes.iter_mut() {
                    for v in p.tabs.iter_mut().filter(|v| v.doc == id) {
                        v.box_sel = None;
                    }
                }
            }
            Err(e) => self.message("Neu laden", format!("„{}“ konnte nicht gelesen werden:\n{e}", path.display())),
        }
    }

    pub fn close_view(&mut self, pane: usize, idx: usize) {
        if idx >= self.panes[pane].tabs.len() {
            return;
        }
        let id = self.panes[pane].tabs[idx].doc;
        self.panes[pane].tabs.remove(idx);
        let p = &mut self.panes[pane];
        if p.active >= p.tabs.len() {
            p.active = p.tabs.len().saturating_sub(1);
        } else if idx < p.active {
            p.active -= 1;
        }
        if !self.panes.iter().any(|p| p.tabs.iter().any(|v| v.doc == id)) {
            self.remove_backup(id);
            self.docs.remove(&id);
        }
        if self.panes[1].tabs.is_empty() {
            self.active_pane = 0;
        }
        if self.panes[0].tabs.is_empty() {
            if self.panes[1].tabs.is_empty() {
                self.active_pane = 0;
                if !self.exit_after_close {
                    self.new_doc();
                }
            } else {
                self.panes.swap(0, 1);
                self.active_pane = 0;
            }
        }
        if !self.panes[self.active_pane].tabs.is_empty() {
            self.focus_editor();
        }
    }

    /// Schließt eine Ansicht; ist es die letzte eines geänderten Dokuments, wird nachgefragt.
    fn request_close_view(&mut self, pane: usize, idx: usize) {
        let Some(id) = self.panes[pane].tabs.get(idx).map(|v| v.doc) else { return };
        let others = self.panes.iter().flat_map(|p| p.tabs.iter()).filter(|v| v.doc == id).count();
        if others <= 1 && self.docs[&id].is_modified() {
            self.closing.push(id);
        } else {
            self.close_view(pane, idx);
        }
    }

    pub fn close_doc_everywhere(&mut self, id: DocId) {
        for pane in 0..2 {
            while let Some(i) = self.panes[pane].tabs.iter().position(|v| v.doc == id) {
                self.close_view(pane, i);
            }
        }
    }

    /// Arbeitet die Warteschlange beim Schließen mehrerer Dokumente ab.
    fn process_closing(&mut self, ctx: &Context) {
        while self.dialog.is_none() {
            let Some(id) = self.closing.first().copied() else { break };
            if !self.docs.contains_key(&id) {
                self.closing.remove(0);
                continue;
            }
            if self.docs[&id].is_modified() {
                self.activate_doc(id);
                self.dialog = Some(Dialog::ConfirmClose { doc: id });
                break;
            }
            self.closing.remove(0);
            self.close_doc_everywhere(id);
        }
        if self.closing.is_empty() && self.dialog.is_none() && self.exit_after_close {
            self.exit_after_close = false;
            self.close_confirmed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    pub fn confirm_close_answer(&mut self, id: DocId, answer: Option<bool>) {
        self.dialog = None;
        match answer {
            None => {
                self.closing.clear();
                self.exit_after_close = false;
            }
            Some(true) => {
                if self.save_doc(id) {
                    self.closing.retain(|d| *d != id);
                    self.close_doc_everywhere(id);
                }
                // Abgebrochen oder fehlgeschlagen: nichts schließen.
                else if self.dialog.is_none() {
                    self.closing.clear();
                    self.exit_after_close = false;
                }
            }
            Some(false) => {
                self.closing.retain(|d| *d != id);
                self.close_doc_everywhere(id);
            }
        }
    }

    // ---------- Sitzung ----------

    fn restore_session(&mut self) {
        let Some(st) = self.storage.clone() else { return };
        if !self.settings.remember_session {
            return;
        }
        let session = st.load_session();
        let mut idmap: HashMap<u64, DocId> = HashMap::new();
        for pane in 0..2 {
            for t in &session.panes[pane] {
                let id = match idmap.get(&t.doc_id) {
                    Some(id) => *id,
                    None => {
                        let Some(id) = self.restore_doc(t, &st) else { continue };
                        idmap.insert(t.doc_id, id);
                        id
                    }
                };
                let rev = self.docs[&id].rev;
                let mut v = View::new(id, rev);
                let n = self.docs[&id].len();
                v.sels = SelSet::range(t.anchor.min(n), t.head.min(n));
                v.top_row = t.top_row;
                self.panes[pane].tabs.push(v);
            }
            self.panes[pane].active = session.active[pane].min(self.panes[pane].tabs.len().saturating_sub(1));
        }
        if self.panes[0].tabs.is_empty() && !self.panes[1].tabs.is_empty() {
            self.panes.swap(0, 1);
        }
        self.active_pane = if self.panes[1].tabs.is_empty() { 0 } else { session.active_pane.min(1) };
    }

    fn restore_doc(&mut self, t: &SessionTab, st: &Storage) -> Option<DocId> {
        let backup = t.backup.as_ref().and_then(|b| std::fs::read(st.backup_dir().join(b)).ok());
        let path = t.path.as_ref().map(PathBuf::from);
        let from_backup = backup.is_some();
        let (text, enc, eol, mt) = match (&backup, &path) {
            (Some(b), _) => (
                fileio::to_lf(String::from_utf8_lossy(b).into_owned()),
                Enc::from_name(&t.enc),
                Eol::from_name(&t.eol),
                path.as_deref().and_then(mtime),
            ),
            (None, Some(p)) => {
                let bytes = std::fs::read(p).ok()?;
                let l = fileio::decode(&bytes, Eol::from_name(&self.settings.default_eol));
                (l.text, l.enc, l.eol, mtime(p))
            }
            (None, None) => (String::new(), Enc::from_name(&t.enc), Eol::from_name(&t.eol), None),
        };
        let id = self.alloc_doc(t.title.clone(), &text, enc, eol);
        let d = self.docs.get_mut(&id).expect("eben angelegt");
        d.path = path;
        d.disk_mtime = mt;
        d.read_only = t.read_only;
        d.extra_dirty = from_backup;
        d.large = self.settings.large_file_enabled && text.len() as u64 >= self.settings.large_file_mb.clamp(1, 4096) << 20;
        d.hl.set_language(if d.large { None } else { t.language.clone() });
        d.bookmarks = t.bookmarks.iter().copied().filter(|l| *l < d.lines()).collect();
        Some(id)
    }

    fn backup_name(id: DocId) -> String {
        format!("{id}.txt")
    }

    fn remove_backup(&mut self, id: DocId) {
        if let Some(st) = &self.storage {
            let _ = std::fs::remove_file(st.backup_dir().join(Self::backup_name(id)));
        }
    }

    /// Sichert ungespeicherte Dokumente und merkt sich die offenen Tabs.
    pub fn save_session(&mut self) {
        let Some(st) = self.storage.clone() else { return };
        self.settings.find_history = self.find.history.clone();
        st.save_settings(&self.settings);
        if !self.settings.remember_session {
            let _ = st.save_session(&Session::default());
            return;
        }
        let dir = st.backup_dir();
        let _ = std::fs::create_dir_all(&dir);
        let mut session = Session { active_pane: self.active_pane, ..Default::default() };
        let mut keep: HashSet<String> = HashSet::new();
        for pane in 0..2 {
            session.active[pane] = self.panes[pane].active;
            for v in &self.panes[pane].tabs {
                let Some(d) = self.docs.get_mut(&v.doc) else { continue };
                let main = v.sels.main();
                let mut backup = None;
                if d.is_modified() || (d.path.is_none() && d.len() > 0) {
                    let name = Self::backup_name(d.id);
                    if d.backup_rev != d.rev || d.rev == 0 || d.backup_file.is_none() || !dir.join(&name).exists() {
                        if std::fs::write(dir.join(&name), d.full_text()).is_ok() {
                            d.backup_rev = d.rev;
                            d.backup_file = Some(name.clone());
                        }
                    }
                    keep.insert(name.clone());
                    backup = Some(name);
                }
                session.panes[pane].push(SessionTab {
                    doc_id: d.id,
                    path: d.path.as_ref().map(|p| p.display().to_string()),
                    title: d.title.clone(),
                    backup,
                    enc: d.enc.name(),
                    eol: d.eol.name().into(),
                    language: d.hl.language.clone(),
                    anchor: main.anchor,
                    head: main.head,
                    top_row: v.top_row,
                    bookmarks: d.bookmarks.iter().copied().collect(),
                    read_only: d.read_only,
                });
            }
        }
        st.save_session(&session);
        // Sicherungen entfernen, die zu keinem Tab mehr gehören (nur eigene Dateien in diesem Ordner).
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if !keep.contains(&n) && n.ends_with(".txt") {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }

    fn timers(&mut self) {
        if self.last_backup.elapsed() > Duration::from_millis(BACKUP_INTERVAL_MS) {
            self.last_backup = Instant::now();
            let probe: u64 = self.docs.values().map(|d| d.rev + d.id).sum::<u64>() + self.docs.len() as u64;
            if probe != self.session_dirty_probe {
                self.session_dirty_probe = probe;
                self.save_session();
            }
        }
        if self.last_watch.elapsed() > Duration::from_millis(1200) {
            self.last_watch = Instant::now();
            self.check_disk_changes();
        }
    }

    pub fn check_disk_changes(&mut self) {
        let ids: Vec<DocId> = self.docs.keys().copied().collect();
        for id in ids {
            let d = &self.docs[&id];
            let Some(path) = d.path.clone() else { continue };
            let monitor = d.monitor;
            match (std::fs::metadata(&path).and_then(|m| m.modified()), d.disk_mtime) {
                (Ok(now), Some(known)) if now != known => {
                    if monitor {
                        self.reload_doc(id);
                        for p in self.panes.iter_mut() {
                            for v in p.tabs.iter_mut().filter(|v| v.doc == id) {
                                let n = self.docs[&id].len();
                                v.sels = SelSet::single(n);
                                v.want_reveal = true;
                                v.seen_rev = self.docs[&id].rev;
                            }
                        }
                    } else if self.settings.auto_reload && !self.docs[&id].is_modified() {
                        // Ohne Nachfrage neu laden geht nur, solange nichts verloren gehen kann.
                        self.reload_doc(id);
                    } else if self.dialog.is_none() {
                        self.docs.get_mut(&id).expect("vorhanden").disk_mtime = Some(now);
                        self.dialog = Some(Dialog::ReloadAsk { doc: id });
                        return;
                    }
                }
                (Err(_), Some(_)) if self.dialog.is_none() => {
                    let d = self.docs.get_mut(&id).expect("vorhanden");
                    d.disk_mtime = None;
                    d.extra_dirty = true;
                    self.dialog = Some(Dialog::DeletedAsk { doc: id });
                    return;
                }
                _ => {}
            }
        }
    }

    // ---------- Aktionen ----------

    pub fn run_pending(&mut self, ctx: &Context) {
        for a in std::mem::take(&mut self.pending) {
            self.run(ctx, a);
        }
        self.process_closing(ctx);
    }

    fn toggle_symbol_all(&mut self) {
        let on = !(self.settings.ed.show_ws && self.settings.ed.show_eol);
        self.settings.ed.show_ws = on;
        self.settings.ed.show_eol = on;
    }

    fn cur_lines_for_bookmarks(&self) -> Vec<usize> {
        self.cur_doc().bookmarks.iter().copied().collect()
    }

    fn jump_bookmark(&mut self, forward: bool) {
        let line = {
            let d = self.cur_doc();
            let cur = d.text.char_to_line(self.cur_view().sels.main().head);
            let next = if forward {
                d.bookmarks.range(cur + 1..).next().or_else(|| d.bookmarks.iter().next())
            } else {
                d.bookmarks.range(..cur).next_back().or_else(|| d.bookmarks.iter().next_back())
            };
            next.copied()
        };
        if let Some(l) = line {
            self.goto_line(l);
        }
    }

    pub fn goto_line(&mut self, line: usize) {
        let d = self.cur_doc();
        let pos = d.line_start(line.min(d.lines() - 1));
        let v = self.cur_view_mut();
        v.sels = SelSet::single(pos);
        v.want_reveal = true;
        v.want_focus = true;
    }

    pub fn select_range(&mut self, a: usize, b: usize) {
        let v = self.cur_view_mut();
        v.sels = SelSet::range(a, b);
        v.want_reveal = true;
    }

    /// Fügt pro Zeile ab dem Cursor einen Text in derselben Spalte ein.
    pub fn column_insert(&mut self, values: impl Fn(usize, usize) -> String) {
        let (first, last, col) = {
            let d = self.cur_doc();
            let sels = &self.cur_view().sels;
            let main = sels.main();
            let col_pos = sels.sels.iter().map(|s| s.start()).min().unwrap_or(main.head);
            let l0 = d.text.char_to_line(col_pos);
            let col = col_pos - d.line_start(l0);
            let l1 = if sels.sels.len() > 1 {
                d.text.char_to_line(sels.sels.last().expect("nicht leer").start())
            } else if !main.is_empty() {
                d.text.char_to_line(main.end())
            } else {
                d.lines() - 1
            };
            (l0, l1, col)
        };
        let before = self.cur_view().sels.clone();
        let id = self.cur_id();
        let d = self.docs.get_mut(&id).expect("offen");
        if d.read_only {
            return;
        }
        d.begin(Kind::Other, &before);
        for line in (first..=last).rev() {
            let len = d.line_len(line);
            let value = values(line - first, last - first + 1);
            let at = d.line_start(line) + col.min(len);
            let pad = " ".repeat(col.saturating_sub(len));
            d.splice(at, at, &format!("{pad}{value}"));
        }
        let start = d.line_start(first) + col.min(d.line_len(first));
        let after = SelSet::single(start);
        d.commit(&after);
        let rev = d.rev;
        let v = self.cur_view_mut();
        v.sels = after;
        v.seen_rev = rev;
    }

    fn hash_text(&self) -> String {
        let main = self.cur_view().sels.main();
        let d = self.cur_doc();
        if main.is_empty() {
            d.full_text()
        } else {
            d.slice(main.start(), main.end())
        }
    }

    pub fn change_font_size(&mut self, size: f32) {
        self.settings.font_size = size.clamp(6.0, 72.0);
        self.metrics = None;
    }

    pub fn run(&mut self, ctx: &Context, a: Action) {
        self.sync_views();
        match a {
            Action::Cmd(c) => self.apply_cmd(c),
            Action::Paste => match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                Ok(t) => self.apply_cmd(Cmd::Paste(t)),
                Err(_) => self.message("Einfügen", "Die Zwischenablage enthält keinen Text."),
            },
            Action::New => {
                self.new_doc();
            }
            Action::Open => {
                let mut dlg = rfd::FileDialog::new();
                if let Some(dir) = self.cur_doc().path.as_ref().and_then(|p| p.parent()) {
                    dlg = dlg.set_directory(dir);
                }
                if let Some(files) = dlg.pick_files() {
                    for f in files {
                        self.open_path(&f);
                    }
                }
            }
            Action::OpenPath(p) => {
                self.open_path(&p);
            }
            Action::Reload => {
                let id = self.cur_id();
                if self.cur_doc().is_modified() {
                    self.dialog = Some(Dialog::ReloadAsk { doc: id });
                } else {
                    self.reload_doc(id);
                }
            }
            Action::Save => {
                let id = self.cur_id();
                self.save_doc(id);
            }
            Action::SaveAs => {
                let id = self.cur_id();
                self.save_as(id);
            }
            Action::SaveCopy => {
                let id = self.cur_id();
                if let Some(p) = rfd::FileDialog::new().set_file_name(self.cur_doc().name()).save_file() {
                    self.write_doc(id, &p, true);
                }
            }
            Action::SaveAll => {
                let ids: Vec<DocId> = self.docs.values().filter(|d| d.is_modified()).map(|d| d.id).collect();
                for id in ids {
                    self.activate_doc(id);
                    if !self.save_doc(id) {
                        break;
                    }
                }
            }
            Action::Rename => {
                if self.cur_doc().path.is_none() {
                    let name = self.cur_doc().title.clone();
                    self.dialog = Some(Dialog::Rename { name, first: true });
                } else {
                    let name = self.cur_doc().name();
                    self.dialog = Some(Dialog::Rename { name, first: true });
                }
            }
            Action::Close => {
                let p = self.active_pane;
                let i = self.panes[p].active;
                self.request_close_view(p, i);
            }
            Action::CloseAll => {
                self.closing = self.docs.keys().copied().collect();
                self.closing.sort();
            }
            Action::CloseOthers => {
                let keep = self.cur_id();
                self.closing = self.docs.keys().copied().filter(|d| *d != keep).collect();
                self.closing.sort();
            }
            Action::CloseTab(p, i) => self.request_close_view(p, i),
            Action::CloseOthersOf(p, i) => {
                if let Some(keep) = self.panes[p].tabs.get(i).map(|v| v.doc) {
                    self.closing = self.docs.keys().copied().filter(|d| *d != keep).collect();
                    self.closing.sort();
                }
            }
            Action::ActivateTab(p, i) => {
                if i < self.panes[p].tabs.len() {
                    self.panes[p].active = i;
                    self.active_pane = p;
                    self.focus_editor();
                }
            }
            Action::NextTab | Action::PrevTab => {
                let p = &mut self.panes[self.active_pane];
                let n = p.tabs.len();
                if n > 0 {
                    p.active = if matches!(a, Action::NextTab) { (p.active + 1) % n } else { (p.active + n - 1) % n };
                }
                self.focus_editor();
            }
            Action::Print => self.print_preview(),
            Action::Exit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Action::OpenExplorer => {
                if let Some(p) = self.cur_doc().path.clone() {
                    #[cfg(windows)]
                    {
                        let _ = std::process::Command::new("explorer").arg("/select,").arg(&p).spawn();
                    }
                    #[cfg(not(windows))]
                    {
                        open_external(p.parent().unwrap_or(&p));
                    }
                }
            }
            Action::OpenTerminal => {
                if let Some(dir) = self.cur_doc().path.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf) {
                    #[cfg(windows)]
                    {
                        let _ = std::process::Command::new("cmd").args(["/C", "start", "cmd"]).current_dir(dir).spawn();
                    }
                    #[cfg(not(windows))]
                    {
                        // Das erste vorhandene Terminal; welche es gibt, hängt von der Linux-Variante ab.
                        let started = ["x-terminal-emulator", "gnome-terminal", "konsole", "xfce4-terminal", "xterm"]
                            .iter()
                            .any(|t| std::process::Command::new(t).current_dir(&dir).spawn().is_ok());
                        if !started {
                            self.message("Terminal", "Auf diesem System wurde kein Terminalprogramm gefunden.");
                        }
                    }
                }
            }
            Action::CopyFullPath => {
                let t = self.cur_doc().path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| self.cur_doc().title.clone());
                ctx.copy_text(t);
            }
            Action::CopyFileName => ctx.copy_text(self.cur_doc().name()),
            Action::CopyDirPath => {
                if let Some(d) = self.cur_doc().path.as_ref().and_then(|p| p.parent()) {
                    ctx.copy_text(d.display().to_string());
                }
            }
            Action::ToggleReadOnly => {
                let d = self.cur_doc_mut();
                d.read_only = !d.read_only;
            }
            Action::ShowFind(tab) => self.show_find(tab),
            Action::FindNext => {
                self.find_step(false);
            }
            Action::FindPrev => {
                self.find_step(true);
            }
            Action::FindSelectedNext => {
                let main = self.cur_view().sels.main();
                if !main.is_empty() {
                    self.find.what = self.cur_doc().slice(main.start(), main.end());
                }
                self.find_step(false);
            }
            Action::GoToDialog => self.dialog = Some(Dialog::GoTo { input: String::new(), by_offset: false, first: true }),
            Action::MatchBrace => {
                let head = self.cur_view().sels.main().head;
                if let Some((a, b)) = editor::matching_brace(self.cur_doc(), head) {
                    let target = if head == a || head == a + 1 { b } else { a };
                    let v = self.cur_view_mut();
                    v.sels = SelSet::single(target);
                    v.want_reveal = true;
                }
            }
            Action::BookmarkToggle => {
                let head = self.cur_view().sels.main().head;
                let d = self.cur_doc_mut();
                let line = d.text.char_to_line(head);
                if !d.bookmarks.remove(&line) {
                    d.bookmarks.insert(line);
                }
            }
            Action::BookmarkNext => self.jump_bookmark(true),
            Action::BookmarkPrev => self.jump_bookmark(false),
            Action::BookmarksClear => self.cur_doc_mut().bookmarks.clear(),
            Action::BookmarksInvert => {
                let d = self.cur_doc_mut();
                let all: std::collections::BTreeSet<usize> = (0..d.lines()).filter(|l| !d.bookmarks.contains(l)).collect();
                d.bookmarks = all;
            }
            Action::BookmarksCopyLines => {
                let lines = self.cur_lines_for_bookmarks();
                let d = self.cur_doc();
                let t = lines.iter().map(|l| d.line_text(*l)).collect::<Vec<_>>().join("\n");
                ctx.copy_text(t);
            }
            Action::BookmarksDeleteLines => {
                let lines = self.cur_lines_for_bookmarks();
                if lines.is_empty() || self.cur_doc().read_only {
                    return;
                }
                let before = self.cur_view().sels.clone();
                let d = self.cur_doc_mut();
                d.begin(Kind::Other, &before);
                for l in lines.iter().rev() {
                    let n = d.lines();
                    let (a, b) = if *l + 1 < n {
                        (d.line_start(*l), d.line_start(*l + 1))
                    } else if *l > 0 {
                        (d.line_end(*l - 1), d.line_end(*l))
                    } else {
                        (0, d.line_end(*l))
                    };
                    d.splice(a, b, "");
                }
                d.commit(&SelSet::single(0));
                let rev = d.rev;
                let v = self.cur_view_mut();
                v.sels = SelSet::single(0);
                v.seen_rev = rev;
            }
            Action::ClearMarks => self.cur_doc_mut().marks.clear(),
            Action::ToggleWrap => self.settings.ed.wrap = !self.settings.ed.wrap,
            Action::ToggleWs => self.settings.ed.show_ws = !self.settings.ed.show_ws,
            Action::ToggleEol => self.settings.ed.show_eol = !self.settings.ed.show_eol,
            Action::ToggleAllSymbols => self.toggle_symbol_all(),
            Action::ToggleGuides => self.settings.ed.indent_guides = !self.settings.ed.indent_guides,
            Action::ToggleLineNumbers => self.settings.ed.line_numbers = !self.settings.ed.line_numbers,
            Action::ZoomIn => self.change_font_size(self.settings.font_size + 1.0),
            Action::ZoomOut => self.change_font_size(self.settings.font_size - 1.0),
            Action::ZoomReset => self.change_font_size(16.0),
            Action::FoldAll => self.cur_doc_mut().fold_all(),
            Action::UnfoldAll => self.cur_doc_mut().unfold_all(),
            Action::FoldCurrent => {
                let head = self.cur_view().sels.main().head;
                let d = self.cur_doc_mut();
                let line = d.text.char_to_line(head);
                let mut l = line;
                loop {
                    if d.folded.contains_key(&l) || d.fold_end(l, true).is_some_and(|e| e >= line) {
                        d.toggle_fold(l);
                        break;
                    }
                    if l == 0 {
                        break;
                    }
                    l -= 1;
                }
                let start = d.line_start(l);
                let v = self.cur_view_mut();
                v.sels = SelSet::single(start);
            }
            Action::ToggleOnTop => {
                self.on_top = !self.on_top;
                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if self.on_top {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                }));
            }
            Action::ToggleFullscreen => {
                self.fullscreen = !self.fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
            }
            Action::MoveToOther => {
                let from = self.active_pane;
                let to = 1 - from;
                if self.panes[from].tabs.len() + self.panes[to].tabs.len() > 1 {
                    let i = self.panes[from].active;
                    let v = self.panes[from].tabs.remove(i);
                    let f = &mut self.panes[from];
                    f.active = f.active.min(f.tabs.len().saturating_sub(1));
                    let t = &mut self.panes[to];
                    t.tabs.push(v);
                    t.active = t.tabs.len() - 1;
                    self.active_pane = to;
                    if self.panes[0].tabs.is_empty() {
                        self.panes.swap(0, 1);
                        self.active_pane = 0;
                    }
                    self.focus_editor();
                }
            }
            Action::CloneToOther => {
                let id = self.cur_id();
                let to = 1 - self.active_pane;
                if let Some(i) = self.panes[to].tabs.iter().position(|v| v.doc == id) {
                    self.panes[to].active = i;
                    self.active_pane = to;
                } else {
                    let (top, sels) = (self.cur_view().top_row, self.cur_view().sels.clone());
                    self.add_view(to, id);
                    let v = self.cur_view_mut();
                    v.top_row = top;
                    v.sels = sels;
                }
            }
            Action::ToggleSyncV => self.sync_v = !self.sync_v,
            Action::ToggleSyncH => self.sync_h = !self.sync_h,
            Action::ToggleMap => self.show_map = !self.show_map,
            Action::ToggleFuncs => self.show_funcs = !self.show_funcs,
            Action::ToggleFolder => {
                self.show_folder = !self.show_folder;
                if self.show_folder && self.settings.workspace.is_none() {
                    self.pick_workspace();
                }
            }
            Action::PickWorkspace => self.pick_workspace(),
            Action::ToggleMonitor => {
                let d = self.cur_doc_mut();
                if d.path.is_some() {
                    d.monitor = !d.monitor;
                    d.read_only = d.monitor;
                }
            }
            Action::Summary => {
                let d = self.cur_doc();
                let s = textops::stats(&d.full_text());
                let sel: usize = self.cur_view().sels.sels.iter().map(|x| x.end() - x.start()).sum();
                let text = format!(
                    "Pfad: {}\nZeichen: {}\nWörter: {}\nZeilen: {}\nGröße im Speicher: {} Byte\nMarkierte Zeichen: {}\nCodierung: {}\nZeilenende: {}",
                    d.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| d.title.clone()),
                    s.chars, s.words, s.lines, s.bytes, sel, d.enc.label(), d.eol.label(),
                );
                self.dialog = Some(Dialog::Summary { text });
            }
            Action::Encode(enc) => {
                // Dieselben Bytes neu lesen: bevorzugt von der Festplatte, sonst aus dem aktuellen Text.
                let id = self.cur_id();
                let d = &self.docs[&id];
                let bytes = if d.is_modified() {
                    None
                } else {
                    d.path.as_ref().and_then(|p| std::fs::read(p).ok())
                };
                let bytes = bytes.unwrap_or_else(|| fileio::encode(&d.full_text(), d.enc, d.eol).0);
                let text = fileio::to_lf(fileio::decode_with(&bytes, enc));
                let d = self.docs.get_mut(&id).expect("offen");
                let modified = d.is_modified();
                d.reset_text(&text);
                d.enc = enc;
                d.extra_dirty = modified;
                self.cur_view_mut().sels = SelSet::single(0);
            }
            Action::Convert(enc) => {
                let d = self.cur_doc_mut();
                if d.enc != enc {
                    d.enc = enc;
                    d.extra_dirty = true;
                }
            }
            Action::SetEol(e) => {
                let d = self.cur_doc_mut();
                if d.eol != e {
                    d.eol = e;
                    d.extra_dirty = true;
                }
            }
            Action::SetLanguage(l) => self.cur_doc_mut().hl.set_language(l),
            Action::Preferences => self.dialog = Some(Dialog::Prefs),
            Action::SetTheme(t) => {
                self.settings.theme = t;
                self.style_dirty = true;
            }
            Action::Shortcuts => self.dialog = Some(Dialog::Shortcuts),
            Action::About => self.dialog = Some(Dialog::About),
            Action::CheckUpdates => self.start_update_check(true),
            Action::ColumnEditor => {
                self.dialog = Some(Dialog::Column {
                    text: String::new(),
                    numbers: false,
                    start: "1".into(),
                    step: "1".into(),
                    repeat: "1".into(),
                    leading: 0,
                    format: 0,
                })
            }
            Action::InsertDateTime => {
                let t = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                self.apply_cmd(Cmd::Insert(t));
            }
            Action::Autocomplete => {
                let id = self.cur_id();
                let p = &mut self.panes[self.active_pane];
                let i = p.active;
                let d = &self.docs[&id];
                editor::start_completion(d, &mut p.tabs[i]);
            }
            Action::Hash(kind) => {
                use md5::Digest as _;
                let text = self.hash_text();
                let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
                let (title, value) = match kind {
                    HashKind::Md5 => ("MD5", hex(&md5::Md5::digest(text.as_bytes()))),
                    HashKind::Sha1 => ("SHA-1", hex(&sha1::Sha1::digest(text.as_bytes()))),
                    HashKind::Sha256 => ("SHA-256", hex(&sha2::Sha256::digest(text.as_bytes()))),
                    HashKind::Sha512 => ("SHA-512", hex(&sha2::Sha512::digest(text.as_bytes()))),
                };
                self.dialog = Some(Dialog::Hash { title: title.into(), value });
            }
        }
    }

    fn pick_workspace(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            self.settings.workspace = Some(dir.display().to_string());
            self.folder_cache.clear();
            self.show_folder = true;
        }
    }

    /// Druckansicht: der farbige Text als Seite im Browser, die den Druckdialog öffnet.
    fn print_preview(&mut self) {
        let id = self.cur_id();
        let html = print_html(self.docs.get_mut(&id).expect("offen"));
        let path = std::env::temp_dir().join(format!("quillpad-druck-{}.html", std::process::id()));
        match std::fs::write(&path, html) {
            Ok(()) => open_external(&path),
            Err(e) => self.message("Drucken", format!("Die Druckansicht konnte nicht erstellt werden:\n{e}")),
        }
    }

    // ---------- Hauptschleife ----------

    pub fn frame(&mut self, ctx: &Context) {
        self.sync_views();
        if self.metrics.is_none() || self.rebuild_metrics {
            self.rebuild_metrics = false;
            self.metrics = Some(Metrics::new(ctx, self.settings.font_size));
        }
        if self.style_dirty {
            self.style_dirty = false;
            let retro = self.settings.theme.retro();
            if retro != self.fonts_retro {
                self.fonts_retro = retro;
                crate::ui_chrome::setup_fonts(ctx, &self.settings.font, retro);
                self.rebuild_metrics = true;
            }
            self.apply_style(ctx);
        }
        self.handle_window_events(ctx);
        self.handle_shortcuts(ctx);
        self.draw_main(ctx);
        self.draw_dialogs(ctx);
        self.poll_find_job(ctx);
        self.poll_update();
        self.run_pending(ctx);
        if self.storage.is_some() {
            self.timers();
        }
        let title = {
            let d = self.cur_doc();
            let star = if d.is_modified() { "*" } else { "" };
            format!("{star}{} - {APP_NAME}", d.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| d.title.clone()))
        };
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
        ctx.request_repaint_after(Duration::from_millis(1500));
    }

    fn handle_window_events(&mut self, ctx: &Context) {
        let forwarded: Vec<Vec<PathBuf>> = self.inbox.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        for files in forwarded {
            for f in files {
                self.open_path(&f);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        for p in dropped {
            if p.is_dir() {
                self.settings.workspace = Some(p.display().to_string());
                self.show_folder = true;
                self.folder_cache.clear();
            } else {
                self.open_path(&p);
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.close_confirmed {
            if self.settings.remember_session && self.storage.is_some() {
                // Alles Ungespeicherte wandert in die Sitzung und ist beim nächsten Start wieder da.
                if let Some(r) = ctx.input(|i| i.viewport().inner_rect) {
                    self.settings.window = Some([r.width(), r.height()]);
                }
                self.save_session();
            } else if self.docs.values().any(|d| d.is_modified()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.closing = self.docs.values().filter(|d| d.is_modified()).map(|d| d.id).collect();
                self.closing.sort();
                self.exit_after_close = true;
            } else {
                self.save_session();
            }
        }
    }
}

/// Der farbige Text als Webseite, die beim Öffnen den Druckdialog zeigt.
pub fn print_html(d: &mut Document) -> String {
    let n = d.lines();
    let colored = n < 200_000;
    if colored {
        while !d.hl.ensure(&d.text, 0, n, Duration::from_secs(10)) {}
    }
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let name = esc(&d.name());
    let mut html = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>{name}</title><style>body{{font:10pt 'Courier New',monospace;color:#000}}pre{{white-space:pre-wrap;margin:0}}</style><body onload=\"window.print()\"><pre>"
    );
    for l in 0..n {
        let text = d.line_text(l);
        match d.hl.spans(l).filter(|s| colored && !s.is_empty()) {
            Some(spans) => {
                let chars: Vec<char> = text.chars().collect();
                let mut i = 0usize;
                for s in spans.iter() {
                    let end = (i + s.chars as usize).min(chars.len());
                    let piece: String = chars[i..end].iter().collect();
                    let weight = if s.bold { "font-weight:bold;" } else { "" };
                    html.push_str(&format!(
                        "<span style=\"{weight}color:#{:02x}{:02x}{:02x}\">{}</span>",
                        s.color.r(), s.color.g(), s.color.b(), esc(&piece)
                    ));
                    i = end;
                }
            }
            None => html.push_str(&esc(&text)),
        }
        html.push('\n');
    }
    html.push_str("</pre>");
    html
}

pub fn mtime(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Windows liefert kanonische Pfade mit der Vorsilbe \\?\ – die zeigen wir nicht an.
pub fn strip_verbatim(p: PathBuf) -> PathBuf {
    let s = p.display().to_string();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => p,
    }
}

pub fn open_external(p: &Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("rundll32").arg("url.dll,FileProtocolHandler").arg(p).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(p).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(p).spawn();
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.frame(ctx);
        let Some(shot) = self.shot.as_mut() else { return };
        ctx.request_repaint();
        shot.frame += 1;
        if shot.frame >= 120 && !shot.sent {
            shot.sent = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let image = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            let [w, h] = image.size;
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
            let ok = image::save_buffer(&shot.path, &bytes, w as u32, h as u32, image::ExtendedColorType::Rgba8);
            if let Err(e) = &ok {
                eprintln!("Bild konnte nicht gespeichert werden: {e}");
            }
            std::process::exit(if ok.is_ok() { 0 } else { 1 });
        }
        if shot.frame > 900 {
            eprintln!("Kein Bild vom Fenster erhalten.");
            std::process::exit(2);
        }
    }

    /// Für automatische Bilder: Klicks, damit auch aufgeklappte Menüs geprüft werden können.
    fn raw_input_hook(&mut self, _ctx: &Context, raw: &mut egui::RawInput) {
        let Some(shot) = &self.shot else { return };
        for (i, p) in self.test_clicks.iter().enumerate() {
            let at = 30 + 20 * i as u32;
            let button = |pressed| egui::Event::PointerButton {
                pos: *p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            if shot.frame == at {
                raw.events.push(egui::Event::PointerMoved(*p));
            } else if shot.frame == at + 2 {
                raw.events.push(button(true));
            } else if shot.frame == at + 4 {
                raw.events.push(button(false));
            }
        }
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {}
}
