//! Ein geöffnetes Dokument: Text, Rückgängig-Verlauf, Lesezeichen, Faltungen.
use crate::fileio::{Enc, Eol};
use crate::highlight::DocHl;
use ropey::Rope;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::time::{Instant, SystemTime};

pub type DocId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sel {
    pub anchor: usize,
    pub head: usize,
}

impl Sel {
    pub fn caret(p: usize) -> Sel {
        Sel { anchor: p, head: p }
    }
    pub fn new(anchor: usize, head: usize) -> Sel {
        Sel { anchor, head }
    }
    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }
    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelSet {
    pub sels: Vec<Sel>,
    pub primary: usize,
}

impl SelSet {
    pub fn single(p: usize) -> SelSet {
        SelSet {
            sels: vec![Sel::caret(p)],
            primary: 0,
        }
    }
    pub fn range(a: usize, b: usize) -> SelSet {
        SelSet {
            sels: vec![Sel::new(a, b)],
            primary: 0,
        }
    }
    pub fn main(&self) -> Sel {
        self.sels[self.primary.min(self.sels.len() - 1)]
    }
    /// Sortiert die Auswahlen und verschmilzt überlappende.
    pub fn normalize(&mut self) {
        if self.sels.is_empty() {
            self.sels.push(Sel::caret(0));
            self.primary = 0;
            return;
        }
        let head = self.main().head;
        self.sels.sort_by_key(|s| (s.start(), s.end()));
        let mut out: Vec<Sel> = Vec::with_capacity(self.sels.len());
        for s in self.sels.drain(..) {
            if let Some(last) = out.last_mut() {
                let touching = s.start() < last.end()
                    || (s.is_empty() && s.start() == last.end())
                    || (last.is_empty() && s.start() == last.end());
                if touching {
                    let (a, b) = (last.start().min(s.start()), last.end().max(s.end()));
                    *last = if s.head < s.anchor && a != b {
                        Sel::new(b, a)
                    } else {
                        Sel::new(a, b)
                    };
                    continue;
                }
            }
            out.push(s);
        }
        self.primary = out
            .iter()
            .rposition(|s| s.start() <= head && head <= s.end())
            .unwrap_or(out.len() - 1);
        self.sels = out;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Typing,
    Erase,
    Other,
}

#[derive(Clone, Debug)]
pub struct Edit {
    pub start: usize,
    pub removed: String,
    pub inserted: String,
}

#[derive(Clone, Debug)]
pub struct Group {
    pub id: u64,
    pub kind: Kind,
    pub edits: Vec<Edit>,
    pub before: SelSet,
    pub after: SelSet,
    pub at: Instant,
}

/// Protokoll der Änderungen, damit andere Ansichten desselben Dokuments folgen können.
#[derive(Clone, Copy, Debug)]
pub struct LogRec {
    pub rev: u64,
    pub start: usize,
    pub removed: usize,
    pub inserted: usize,
    pub first_line: usize,
    pub removed_lines: usize,
    pub inserted_lines: usize,
}

pub const LOG_CAP: usize = 512;

pub struct Document {
    pub id: DocId,
    pub text: Rope,
    pub path: Option<PathBuf>,
    pub title: String,
    pub enc: Enc,
    pub eol: Eol,
    pub read_only: bool,
    pub monitor: bool,
    /// Große Datei: keine Farben, keine Klammern, keine Wort-Hervorhebung, keine Vervollständigung.
    pub large: bool,
    pub undo: Vec<Group>,
    pub redo: Vec<Group>,
    open: Option<Group>,
    next_group: u64,
    pub saved_group: Option<u64>,
    pub extra_dirty: bool,
    pub rev: u64,
    pub log: VecDeque<LogRec>,
    pub bookmarks: BTreeSet<usize>,
    pub folded: BTreeMap<usize, usize>,
    pub fold_rev: u64,
    fold_cache: Option<(u64, Vec<u32>)>,
    pub marks: Vec<(usize, usize)>,
    pub hl: DocHl,
    pub disk_mtime: Option<SystemTime>,
    pub last_edit: Instant,
    pub backup_rev: u64,
    pub backup_file: Option<String>,
}

impl Document {
    pub fn new(id: DocId, title: String, text: &str, enc: Enc, eol: Eol) -> Document {
        Document {
            id,
            text: Rope::from_str(text),
            path: None,
            title,
            enc,
            eol,
            read_only: false,
            monitor: false,
            large: false,
            undo: Vec::new(),
            redo: Vec::new(),
            open: None,
            next_group: 1,
            saved_group: None,
            extra_dirty: false,
            rev: 0,
            log: VecDeque::new(),
            bookmarks: BTreeSet::new(),
            folded: BTreeMap::new(),
            fold_rev: 0,
            fold_cache: None,
            marks: Vec::new(),
            hl: DocHl::new(None),
            disk_mtime: None,
            last_edit: Instant::now(),
            backup_rev: 0,
            backup_file: None,
        }
    }

    pub fn name(&self) -> String {
        match &self.path {
            Some(p) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string()),
            None => self.title.clone(),
        }
    }

    pub fn is_modified(&self) -> bool {
        self.extra_dirty || self.undo.last().map(|g| g.id) != self.saved_group
    }

    pub fn mark_saved(&mut self) {
        self.saved_group = self.undo.last().map(|g| g.id);
        self.extra_dirty = false;
    }

    pub fn len(&self) -> usize {
        self.text.len_chars()
    }
    pub fn lines(&self) -> usize {
        self.text.len_lines()
    }
    pub fn line_start(&self, line: usize) -> usize {
        self.text.line_to_char(line.min(self.lines().saturating_sub(1)))
    }
    /// Länge einer Zeile in Zeichen, ohne den Zeilenumbruch.
    pub fn line_len(&self, line: usize) -> usize {
        let l = self.text.line(line);
        let n = l.len_chars();
        if n > 0 && l.char(n - 1) == '\n' {
            n - 1
        } else {
            n
        }
    }
    pub fn line_end(&self, line: usize) -> usize {
        self.line_start(line) + self.line_len(line)
    }
    pub fn line_text(&self, line: usize) -> String {
        let mut s = self.text.line(line).to_string();
        if s.ends_with('\n') {
            s.pop();
        }
        s
    }
    pub fn slice(&self, a: usize, b: usize) -> String {
        self.text.slice(a..b).to_string()
    }
    pub fn full_text(&self) -> String {
        self.text.to_string()
    }

    /// Öffnet eine Bearbeitungsgruppe. Tippen und Löschen fassen wir zusammen,
    /// solange sie unmittelbar aufeinander folgen.
    pub fn begin(&mut self, kind: Kind, before: &SelSet) {
        if self.open.is_some() {
            return;
        }
        if kind != Kind::Other {
            let mergeable = self.undo.last().is_some_and(|g| {
                g.kind == kind
                    && g.after == *before
                    && g.at.elapsed().as_millis() < 1200
                    && Some(g.id) != self.saved_group
            });
            if mergeable {
                self.open = self.undo.pop();
                return;
            }
        }
        let id = self.next_group;
        self.next_group += 1;
        self.open = Some(Group {
            id,
            kind,
            edits: Vec::new(),
            before: before.clone(),
            after: before.clone(),
            at: Instant::now(),
        });
    }

    pub fn commit(&mut self, after: &SelSet) {
        if let Some(mut g) = self.open.take() {
            if g.edits.is_empty() {
                return;
            }
            g.after = after.clone();
            g.at = Instant::now();
            self.undo.push(g);
            self.redo.clear();
        }
    }

    /// Ersetzt den Bereich a..b. Muss zwischen begin und commit aufgerufen werden.
    pub fn splice(&mut self, a: usize, b: usize, text: &str) {
        if a == b && text.is_empty() {
            return;
        }
        let removed = self.slice(a, b);
        self.raw_splice(a, &removed, text);
        if let Some(g) = &mut self.open {
            g.edits.push(Edit {
                start: a,
                removed,
                inserted: text.to_string(),
            });
        }
    }

    fn raw_splice(&mut self, a: usize, removed: &str, inserted: &str) {
        let removed_chars = removed.chars().count();
        let inserted_chars = inserted.chars().count();
        let first_line = self.text.char_to_line(a);
        let at_line_start = self.text.line_to_char(first_line) == a;
        let removed_lines = removed.bytes().filter(|b| *b == b'\n').count();
        let inserted_lines = inserted.bytes().filter(|b| *b == b'\n').count();

        if removed_chars > 0 {
            self.text.remove(a..a + removed_chars);
        }
        if inserted_chars > 0 {
            self.text.insert(a, inserted);
        }
        self.rev += 1;
        self.last_edit = Instant::now();
        self.hl.invalidate_from(first_line);
        self.log.push_back(LogRec {
            rev: self.rev,
            start: a,
            removed: removed_chars,
            inserted: inserted_chars,
            first_line,
            removed_lines,
            inserted_lines,
        });
        if self.log.len() > LOG_CAP {
            self.log.pop_front();
        }
        let whole_lines = at_line_start && removed.ends_with('\n');
        self.shift_line_data(first_line, at_line_start, whole_lines, removed_lines, inserted_lines);
        self.shift_marks(a, removed_chars, inserted_chars);
    }

    /// Lesezeichen und Faltungen wandern mit dem Text.
    fn shift_line_data(&mut self, first: usize, at_start: bool, whole_lines: bool, old: usize, new: usize) {
        if old == 0 && new == 0 {
            return;
        }
        let shift = |l: usize| -> Option<usize> {
            if whole_lines {
                // Ganze Zeilen entfernt: die dahinter rücken nach, ihre Zeilen bleiben erhalten.
                return if l < first {
                    Some(l)
                } else if l < first + old {
                    None
                } else {
                    Some(l - old + new)
                };
            }
            // Zeilenumbruch am Zeilenanfang schiebt die Zeile selbst nach unten.
            if l == first && at_start && old == 0 {
                return Some(l + new);
            }
            if l <= first {
                Some(l)
            } else if l <= first + old {
                None
            } else {
                Some(l - old + new)
            }
        };
        self.bookmarks = self.bookmarks.iter().filter_map(|l| shift(*l)).collect();
        if !self.folded.is_empty() {
            let old_map = std::mem::take(&mut self.folded);
            for (s, e) in old_map {
                if let (Some(ns), Some(ne)) = (shift(s), shift(e)) {
                    if ne > ns {
                        self.folded.insert(ns, ne);
                    }
                }
            }
            self.fold_rev += 1;
        }
    }

    fn shift_marks(&mut self, a: usize, removed: usize, inserted: usize) {
        if self.marks.is_empty() {
            return;
        }
        let b = a + removed;
        self.marks.retain_mut(|m| {
            if m.1 <= a {
                true
            } else if m.0 >= b {
                m.0 = m.0 - removed + inserted;
                m.1 = m.1 - removed + inserted;
                true
            } else {
                false
            }
        });
    }

    pub fn undo(&mut self) -> Option<SelSet> {
        self.open = None;
        let g = self.undo.pop()?;
        for e in g.edits.iter().rev() {
            self.raw_splice(e.start, &e.inserted, &e.removed);
        }
        let s = g.before.clone();
        self.redo.push(g);
        Some(s)
    }

    pub fn redo(&mut self) -> Option<SelSet> {
        self.open = None;
        let g = self.redo.pop()?;
        for e in g.edits.iter() {
            self.raw_splice(e.start, &e.removed, &e.inserted);
        }
        let s = g.after.clone();
        self.undo.push(g);
        Some(s)
    }

    /// Ersetzt den gesamten Inhalt (z. B. beim Neuladen), ohne Verlauf.
    pub fn reset_text(&mut self, text: &str) {
        self.text = Rope::from_str(text);
        self.undo.clear();
        self.redo.clear();
        self.open = None;
        self.saved_group = None;
        self.extra_dirty = false;
        self.rev += 1;
        self.log.clear();
        self.marks.clear();
        self.folded.clear();
        self.fold_rev += 1;
        let lines = self.lines();
        self.bookmarks.retain(|b| *b < lines);
        self.hl.invalidate_from(0);
    }

    /// Faltbereiche; große Dateien erst nach kurzer Tipp-Pause neu berechnen.
    pub fn fold_end(&mut self, line: usize, force: bool) -> Option<usize> {
        let n = self.lines();
        if n > 300_000 {
            return None;
        }
        let stale = self.fold_cache.as_ref().map(|c| c.0) != Some(self.rev);
        if stale && (force || n < 20_000 || self.last_edit.elapsed().as_millis() > 300) {
            let ends = crate::textops::compute_folds(self.text.lines().map(|l| l.into()), n);
            self.fold_cache = Some((self.rev, ends));
        }
        let c = self.fold_cache.as_ref()?;
        match c.1.get(line) {
            Some(&e) if e as usize > line && (e as usize) < n => Some(e as usize),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn is_hidden(&self, line: usize) -> bool {
        self.folded.range(..line).any(|(_, e)| line <= *e)
    }

    pub fn toggle_fold(&mut self, line: usize) {
        if self.folded.remove(&line).is_some() {
            self.fold_rev += 1;
        } else if let Some(e) = self.fold_end(line, true) {
            self.folded.insert(line, e);
            self.fold_rev += 1;
        }
    }

    /// Klappt alle Bereiche auf, die diese Zeile verstecken.
    pub fn unfold_containing(&mut self, line: usize) {
        let hit: Vec<usize> = self
            .folded
            .range(..line)
            .filter(|(_, e)| line <= **e)
            .map(|(s, _)| *s)
            .collect();
        if !hit.is_empty() {
            for s in hit {
                self.folded.remove(&s);
            }
            self.fold_rev += 1;
        }
    }

    pub fn fold_all(&mut self) {
        let n = self.lines();
        self.folded.clear();
        for l in 0..n {
            if let Some(e) = self.fold_end(l, true) {
                self.folded.insert(l, e);
            }
        }
        self.fold_rev += 1;
    }

    pub fn unfold_all(&mut self) {
        self.folded.clear();
        self.fold_rev += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(t: &str) -> Document {
        Document::new(1, "neu 1".into(), t, Enc::Utf8, Eol::Crlf)
    }

    fn edit(doc: &mut Document, kind: Kind, a: usize, b: usize, t: &str) {
        let before = SelSet::single(a);
        doc.begin(kind, &before);
        doc.splice(a, b, t);
        doc.commit(&SelSet::single(a + t.chars().count()));
    }

    #[test]
    fn undo_redo_restore_text_and_selection() {
        let mut doc = d("hallo");
        edit(&mut doc, Kind::Other, 0, 5, "welt");
        assert_eq!(doc.full_text(), "welt");
        assert_eq!(doc.undo().unwrap(), SelSet::single(0));
        assert_eq!(doc.full_text(), "hallo");
        assert_eq!(doc.redo().unwrap(), SelSet::single(4));
        assert_eq!(doc.full_text(), "welt");
    }

    #[test]
    fn typing_merges_into_one_undo_step() {
        let mut doc = d("");
        for (i, c) in "abc".chars().enumerate() {
            edit(&mut doc, Kind::Typing, i, i, &c.to_string());
        }
        assert_eq!(doc.undo.len(), 1);
        doc.undo();
        assert_eq!(doc.full_text(), "");
    }

    #[test]
    fn modified_flag_follows_save_point() {
        let mut doc = d("x");
        assert!(!doc.is_modified());
        edit(&mut doc, Kind::Other, 0, 0, "a");
        assert!(doc.is_modified());
        doc.mark_saved();
        assert!(!doc.is_modified());
        doc.undo();
        assert!(doc.is_modified());
        doc.redo();
        assert!(!doc.is_modified());
    }

    #[test]
    fn typing_after_save_starts_a_new_step() {
        let mut doc = d("");
        edit(&mut doc, Kind::Typing, 0, 0, "a");
        doc.mark_saved();
        edit(&mut doc, Kind::Typing, 1, 1, "b");
        assert_eq!(doc.undo.len(), 2);
        doc.undo();
        assert!(!doc.is_modified());
    }

    #[test]
    fn bookmarks_follow_lines() {
        let mut doc = d("a\nb\nc\n");
        doc.bookmarks.insert(1);
        doc.bookmarks.insert(2);
        edit(&mut doc, Kind::Other, 0, 0, "x\n");
        assert_eq!(doc.bookmarks.iter().copied().collect::<Vec<_>>(), vec![2, 3]);
        // Zeile ohne Lesezeichen löschen: die anderen rücken nach und bleiben.
        let (s, e) = (doc.line_start(1), doc.line_start(2));
        edit(&mut doc, Kind::Other, s, e, "");
        assert_eq!(doc.bookmarks.iter().copied().collect::<Vec<_>>(), vec![1, 2]);
        // Zeile mit Lesezeichen löschen: dieses verschwindet.
        let (s, e) = (doc.line_start(1), doc.line_start(2));
        edit(&mut doc, Kind::Other, s, e, "");
        assert_eq!(doc.bookmarks.iter().copied().collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn marks_shift_and_drop() {
        let mut doc = d("abcdef");
        doc.marks = vec![(1, 2), (4, 5)];
        edit(&mut doc, Kind::Other, 0, 0, "xx");
        assert_eq!(doc.marks, vec![(3, 4), (6, 7)]);
        edit(&mut doc, Kind::Other, 3, 4, "");
        assert_eq!(doc.marks, vec![(5, 6)]);
    }

    #[test]
    fn normalize_merges_overlaps_and_keeps_primary() {
        let mut s = SelSet {
            sels: vec![Sel::new(5, 8), Sel::new(0, 2), Sel::new(7, 9)],
            primary: 1,
        };
        s.normalize();
        assert_eq!(s.sels, vec![Sel::new(0, 2), Sel::new(5, 9)]);
        assert_eq!(s.primary, 0);
    }

    #[test]
    fn fold_hides_lines_and_unfolds() {
        let mut doc = d("a {\n  b\n  c\n}\nd");
        doc.toggle_fold(0);
        assert!(doc.is_hidden(2));
        assert!(!doc.is_hidden(4));
        doc.unfold_containing(2);
        assert!(!doc.is_hidden(2));
    }
}
