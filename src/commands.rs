//! Alles, was den Text oder die Auswahl verändert. Tastatur und Menüs
//! laufen über dieselben Befehle.
use crate::doc::{Document, Kind, Sel, SelSet};
use crate::editor::{self, BoxSel, EdOpts, Metrics, View};
use crate::highlight;
use crate::textops::{self, CaseKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    DocStart,
    DocEnd,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Cmd {
    Insert(String),
    Paste(String),
    Newline,
    Backspace,
    Delete,
    WordBackspace,
    WordDelete,
    Indent,
    Outdent,
    Move(Motion, bool),
    BoxMove(Motion),
    SelectAll,
    Undo,
    Redo,
    Copy,
    Cut,
    DeleteSelection,
    DuplicateLines,
    DeleteLines,
    MoveLinesUp,
    MoveLinesDown,
    JoinLines,
    Case(CaseKind),
    SortLines { descending: bool, ignore_case: bool },
    DedupLines,
    RemoveEmptyLines { blank_too: bool },
    TrimTrailing,
    TrimLeading,
    TabsToSpaces,
    LeadingSpacesToTabs,
    Comment(Option<bool>),
    BlockComment,
    ToggleOverwrite,
    CollapseSelections,
}

impl Cmd {
    pub fn mutates(&self) -> bool {
        !matches!(
            self,
            Cmd::Move(..) | Cmd::BoxMove(_) | Cmd::SelectAll | Cmd::Copy | Cmd::ToggleOverwrite | Cmd::CollapseSelections
        )
    }
}

pub struct Ed<'a> {
    pub doc: &'a mut Document,
    pub v: &'a mut View,
    pub o: &'a EdOpts,
    pub m: &'a mut Metrics,
}

/// Eine Ersetzung samt Regel, wo Cursor oder Auswahl danach stehen.
struct Change {
    start: usize,
    end: usize,
    text: String,
    select: bool,
    caret_back: usize,
}

impl Change {
    fn caret(start: usize, end: usize, text: String) -> Change {
        Change { start, end, text, select: false, caret_back: 0 }
    }
    fn erase(start: usize, end: usize) -> Change {
        Change::caret(start, end, String::new())
    }
}

fn char_class(c: char) -> u8 {
    if c == '\n' {
        3
    } else if c.is_whitespace() {
        0
    } else if c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

impl<'a> Ed<'a> {
    pub fn set(&mut self, s: SelSet) {
        self.v.set_sels(s);
        self.v.seen_rev = self.doc.rev;
    }

    fn word_left(&self, pos: usize) -> usize {
        let t = &self.doc.text;
        let mut p = pos;
        if p == 0 {
            return 0;
        }
        if t.char(p - 1) == '\n' {
            return p - 1;
        }
        while p > 0 && char_class(t.char(p - 1)) == 0 {
            p -= 1;
        }
        if p > 0 {
            let k = char_class(t.char(p - 1));
            if k != 3 {
                while p > 0 && char_class(t.char(p - 1)) == k {
                    p -= 1;
                }
            }
        }
        p
    }

    fn word_right(&self, pos: usize) -> usize {
        let t = &self.doc.text;
        let n = t.len_chars();
        let mut p = pos;
        if p >= n {
            return n;
        }
        if t.char(p) == '\n' {
            return p + 1;
        }
        let k = char_class(t.char(p));
        if k != 0 {
            while p < n && char_class(t.char(p)) == k {
                p += 1;
            }
        }
        while p < n && char_class(t.char(p)) == 0 {
            p += 1;
        }
        p
    }

    /// Das Wort unter dieser Position (für den Doppelklick).
    pub fn word_at(&self, pos: usize) -> (usize, usize) {
        let t = &self.doc.text;
        let n = t.len_chars();
        let probe = if pos < n && char_class(t.char(pos)) != 3 {
            pos
        } else if pos > 0 {
            pos - 1
        } else {
            return (pos, pos);
        };
        let k = char_class(t.char(probe));
        if k == 3 {
            return (pos, pos);
        }
        let (mut a, mut b) = (probe, probe + 1);
        while a > 0 && char_class(t.char(a - 1)) == k {
            a -= 1;
        }
        while b < n && char_class(t.char(b)) == k {
            b += 1;
        }
        (a, b)
    }

    fn target(&mut self, pos: usize, motion: Motion, goal: Option<f32>) -> (usize, Option<f32>) {
        let n = self.doc.len();
        match motion {
            Motion::Left => (pos.saturating_sub(1), None),
            Motion::Right => ((pos + 1).min(n), None),
            Motion::WordLeft => (self.word_left(pos), None),
            Motion::WordRight => (self.word_right(pos), None),
            Motion::DocStart => (0, None),
            Motion::DocEnd => (n, None),
            Motion::Home => {
                let line = self.doc.text.char_to_line(pos);
                let start = self.doc.line_start(line);
                let s = self.doc.line_text(line);
                let (row_start, _) = editor::row_bounds(self.doc, self.v, self.o, self.m, pos);
                if row_start > start {
                    return (row_start, None);
                }
                let first = s.chars().take_while(|c| *c == ' ' || *c == '\t').count();
                let p = if pos == start + first { start } else { start + first };
                (p, None)
            }
            Motion::End => {
                let (_, row_end) = editor::row_bounds(self.doc, self.v, self.o, self.m, pos);
                (row_end, None)
            }
            Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown => {
                let (row, x) = editor::pos_row_x(self.doc, self.v, self.o, self.m, pos);
                let g = goal.unwrap_or(x);
                let page = self.v.page_rows.max(1) as u64;
                let total = self.v.map.total_rows(self.doc).max(1);
                let new_row = match motion {
                    Motion::Up => row.saturating_sub(1),
                    Motion::Down => (row + 1).min(total - 1),
                    Motion::PageUp => row.saturating_sub(page),
                    _ => (row + page).min(total - 1),
                };
                if matches!(motion, Motion::PageUp) {
                    self.v.top_row = (self.v.top_row - page as f64).max(0.0);
                } else if matches!(motion, Motion::PageDown) {
                    self.v.top_row += page as f64;
                }
                (editor::row_x_pos(self.doc, self.v, self.o, self.m, new_row, g), Some(g))
            }
        }
    }

    fn do_move(&mut self, motion: Motion, extend: bool) {
        let single = self.v.sels.sels.len() == 1;
        let mut sels = std::mem::take(&mut self.v.sels.sels);
        let mut new_goal = None;
        for s in sels.iter_mut() {
            let collapsing = !extend && !s.is_empty();
            let p = match (collapsing, motion) {
                (true, Motion::Left) => s.start(),
                (true, Motion::Right) => s.end(),
                _ => {
                    let goal = if single { self.v.goal_x } else { None };
                    let (p, g) = self.target(s.head, motion, goal);
                    new_goal = g;
                    p
                }
            };
            *s = if extend { Sel::new(s.anchor, p) } else { Sel::caret(p) };
        }
        self.v.sels.sels = sels;
        self.v.goal_x = if single { new_goal } else { None };
        self.v.sels.normalize();
        self.v.want_reveal = true;
    }

    /// Spaltenauswahl: pro Zeile eine eigene Auswahl zwischen zwei x-Positionen.
    pub fn apply_box(&mut self, b: BoxSel) {
        let (l0, l1) = (b.anchor_line.min(b.head_line), b.anchor_line.max(b.head_line));
        let mut sels = Vec::new();
        let up = b.head_line < b.anchor_line;
        for line in l0..=l1.min(self.doc.lines() - 1) {
            let a = editor::line_x_pos(self.doc, self.o, self.m, line, b.anchor_x);
            let h = editor::line_x_pos(self.doc, self.o, self.m, line, b.head_x);
            sels.push(Sel::new(a, h));
        }
        let primary = if up { 0 } else { sels.len().saturating_sub(1) };
        self.v.sels = SelSet { sels, primary };
        self.v.box_sel = Some(b);
        self.v.want_reveal = true;
    }

    fn box_move(&mut self, motion: Motion) {
        let mut b = match self.v.box_sel {
            Some(b) => b,
            None => {
                let head = self.v.sels.main().head;
                let line = self.doc.text.char_to_line(head);
                let x = editor::line_pos_x(self.doc, self.o, self.m, head);
                BoxSel { anchor_line: line, anchor_x: x, head_line: line, head_x: x }
            }
        };
        let ch = self.m.ch;
        match motion {
            Motion::Up => b.head_line = b.head_line.saturating_sub(1),
            Motion::Down => b.head_line = (b.head_line + 1).min(self.doc.lines() - 1),
            Motion::Left => b.head_x = (b.head_x - ch).max(0.0),
            Motion::Right => b.head_x += ch,
            _ => {}
        }
        self.apply_box(b);
    }

    fn change(&mut self, kind: Kind, mut changes: Vec<Change>) {
        changes.sort_by_key(|c| c.start);
        let mut prev_end = 0;
        for c in changes.iter_mut() {
            c.start = c.start.max(prev_end);
            c.end = c.end.max(c.start);
            prev_end = c.end;
        }
        let before = self.v.sels.clone();
        self.doc.begin(kind, &before);
        let mut new_sels = Vec::with_capacity(changes.len());
        let mut delta: isize = 0;
        for c in &changes {
            let n = c.text.chars().count() as isize;
            let start = (c.start as isize + delta) as usize;
            let end = start + n as usize;
            new_sels.push(if c.select {
                Sel::new(start, end)
            } else {
                Sel::caret(end - c.caret_back)
            });
            delta += n - (c.end - c.start) as isize;
        }
        for c in changes.iter().rev() {
            self.doc.splice(c.start, c.end, &c.text);
        }
        let primary = self.v.sels.primary.min(new_sels.len().saturating_sub(1));
        let mut set = SelSet { sels: new_sels, primary };
        set.normalize();
        self.doc.commit(&set);
        self.set(set);
        self.v.goal_x = None;
        self.v.want_reveal = true;
    }

    fn insert(&mut self, text: &str, kind: Kind) {
        let auto = self.o.auto_close && text.chars().count() == 1;
        let single = text.chars().next().unwrap_or(' ');
        let pair = match single {
            '(' => Some(')'),
            '[' => Some(']'),
            '{' => Some('}'),
            '"' => Some('"'),
            '\'' => Some('\''),
            _ => None,
        };
        let sels = self.v.sels.sels.clone();
        // Schließendes Zeichen überspringen, wenn es schon dasteht.
        if auto
            && matches!(single, ')' | ']' | '}' | '"' | '\'')
            && sels
                .iter()
                .all(|s| s.is_empty() && s.head < self.doc.len() && self.doc.text.char(s.head) == single)
        {
            self.do_move(Motion::Right, false);
            return;
        }
        let changes = sels
            .iter()
            .map(|s| {
                let mut end = s.end();
                if self.v.overwrite && s.is_empty() && end < self.doc.len() && self.doc.text.char(end) != '\n' {
                    end += 1;
                }
                if auto && s.is_empty() {
                    if let Some(p) = pair {
                        let next = self.doc.text.get_char(s.head).unwrap_or('\n');
                        if next.is_whitespace() || matches!(next, ')' | ']' | '}') {
                            return Change {
                                start: s.start(),
                                end,
                                text: format!("{single}{p}"),
                                select: false,
                                caret_back: 1,
                            };
                        }
                    }
                }
                Change::caret(s.start(), end, text.to_string())
            })
            .collect();
        self.change(kind, changes);
    }

    fn newline(&mut self) {
        let sels = self.v.sels.sels.clone();
        let changes = sels
            .iter()
            .map(|s| {
                let mut text = String::from("\n");
                if self.o.auto_indent {
                    let line = self.doc.text.char_to_line(s.start());
                    let start = self.doc.line_start(line);
                    let before = self.doc.slice(start, s.start());
                    let ws: String = before.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                    text.push_str(&ws);
                    if before.trim_end().ends_with('{') {
                        text.push_str(&self.indent_unit());
                    }
                }
                Change::caret(s.start(), s.end(), text)
            })
            .collect();
        self.change(Kind::Other, changes);
    }

    fn indent_unit(&self) -> String {
        if self.o.use_spaces {
            " ".repeat(self.o.tab_size)
        } else {
            "\t".to_string()
        }
    }

    /// Die von den Auswahlen berührten ganzen Zeilen.
    fn touched_lines(&self) -> (usize, usize) {
        let mut l0 = usize::MAX;
        let mut l1 = 0;
        for s in &self.v.sels.sels {
            let a = self.doc.text.char_to_line(s.start());
            let mut b = self.doc.text.char_to_line(s.end());
            if !s.is_empty() && b > a && self.doc.line_start(b) == s.end() {
                b -= 1;
            }
            l0 = l0.min(a);
            l1 = l1.max(b);
        }
        (l0, l1)
    }

    /// Formt die berührten Zeilen um und markiert das Ergebnis, wenn vorher etwas markiert war.
    fn map_lines(&mut self, f: impl FnOnce(&str) -> String) {
        let (l0, l1) = self.touched_lines();
        let a = self.doc.line_start(l0);
        let b = self.doc.line_end(l1);
        self.map_range(a, b, f);
    }

    fn map_range(&mut self, a: usize, b: usize, f: impl FnOnce(&str) -> String) {
        let had_selection = self.v.sels.sels.iter().any(|s| !s.is_empty());
        let old = self.doc.slice(a, b);
        let new = f(&old);
        if new == old {
            return;
        }
        let head = self.v.sels.main().head;
        let (old_n, new_n) = (old.chars().count() as isize, new.chars().count() as isize);
        // Ohne Auswahl bleibt der Cursor an seiner Stelle im Text.
        let desired = (head as isize + new_n - old_n).clamp(a as isize, a as isize + new_n);
        let caret_back = (a as isize + new_n - desired) as usize;
        let c = Change {
            start: a,
            end: b,
            text: new,
            select: had_selection,
            caret_back,
        };
        self.v.box_sel = None;
        self.v.sels = SelSet::single(head.min(b));
        self.change(Kind::Other, vec![c]);
    }

    /// Ohne Auswahl wirken Sortieren und Ähnliches auf das ganze Dokument.
    fn map_selection_or_all(&mut self, f: impl FnOnce(&str) -> String) {
        if self.v.sels.sels.iter().any(|s| !s.is_empty()) {
            self.map_lines(f);
        } else {
            let n = self.doc.len();
            self.map_range(0, n, f);
        }
    }

    fn move_lines(&mut self, up: bool) {
        let (l0, l1) = self.touched_lines();
        if (up && l0 == 0) || (!up && l1 + 1 >= self.doc.lines()) {
            return;
        }
        let block = self.doc.slice(self.doc.line_start(l0), self.doc.line_end(l1));
        let (a, b, swap_text) = if up {
            (self.doc.line_start(l0 - 1), self.doc.line_end(l1), self.doc.line_text(l0 - 1))
        } else {
            (self.doc.line_start(l0), self.doc.line_end(l1 + 1), self.doc.line_text(l1 + 1))
        };
        let new = if up {
            format!("{block}\n{swap_text}")
        } else {
            format!("{swap_text}\n{block}")
        };
        let shift = swap_text.chars().count() as isize + 1;
        let shift = if up { -shift } else { shift };
        let mut after = self.v.sels.clone();
        for s in after.sels.iter_mut() {
            s.anchor = (s.anchor as isize + shift) as usize;
            s.head = (s.head as isize + shift) as usize;
        }
        let before = self.v.sels.clone();
        self.doc.begin(Kind::Other, &before);
        self.doc.splice(a, b, &new);
        self.doc.commit(&after);
        self.set(after);
        self.v.want_reveal = true;
    }

    fn copy_text(&self) -> Option<String> {
        let parts: Vec<String> = self
            .v
            .sels
            .sels
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| self.doc.slice(s.start(), s.end()))
            .collect();
        (!parts.is_empty()).then(|| parts.join("\n"))
    }

    fn paste(&mut self, clip: &str) {
        let clip = crate::fileio::to_lf(clip.to_string());
        let sels = self.v.sels.sels.clone();
        let parts: Vec<&str> = clip.split('\n').collect();
        let changes: Vec<Change> = if sels.len() > 1 && parts.len() == sels.len() {
            sels.iter()
                .zip(parts)
                .map(|(s, p)| Change::caret(s.start(), s.end(), p.to_string()))
                .collect()
        } else {
            sels.iter()
                .map(|s| Change::caret(s.start(), s.end(), clip.clone()))
                .collect()
        };
        self.change(Kind::Other, changes);
    }

    fn comment_block(&mut self) {
        let (_, block) = highlight::comment_tokens(self.doc.hl.language.as_deref());
        let (open, close) = block.unwrap_or(("/*", "*/"));
        let s = self.v.sels.main();
        if s.is_empty() {
            return;
        }
        let text = self.doc.slice(s.start(), s.end());
        let new = match text.strip_prefix(open).and_then(|t| t.strip_suffix(close)) {
            Some(inner) => inner.to_string(),
            None => format!("{open}{text}{close}"),
        };
        let mut c = Change::caret(s.start(), s.end(), new);
        c.select = true;
        self.v.sels = SelSet { sels: vec![s], primary: 0 };
        self.change(Kind::Other, vec![c]);
    }

    fn delete_selection(&mut self) {
        let sels = self.v.sels.sels.clone();
        let changes: Vec<Change> = sels
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| Change::erase(s.start(), s.end()))
            .collect();
        if !changes.is_empty() {
            self.change(Kind::Other, changes);
        }
    }

    /// Führt einen Befehl aus. Rückgabe: Text für die Zwischenablage (Kopieren, Ausschneiden).
    pub fn apply(&mut self, cmd: Cmd) -> Option<String> {
        let result = self.run(cmd);
        self.v.map.update(self.doc, self.o, self.v.text_w, self.m);
        result
    }

    fn run(&mut self, cmd: Cmd) -> Option<String> {
        if cmd.mutates() && self.doc.read_only {
            return None;
        }
        if !matches!(cmd, Cmd::BoxMove(_)) {
            self.v.box_sel = None;
        }
        if !matches!(cmd, Cmd::Move(Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown, _)) {
            self.v.goal_x = None;
        }
        match cmd {
            Cmd::Insert(t) => self.insert(&t, Kind::Typing),
            Cmd::Paste(t) => self.paste(&t),
            Cmd::Newline => self.newline(),
            Cmd::Backspace => {
                let tab = self.o.tab_size.max(1);
                let use_spaces = self.o.use_spaces;
                let sels = self.v.sels.sels.clone();
                let changes: Vec<Change> = sels
                    .iter()
                    .filter_map(|s| {
                        if !s.is_empty() {
                            return Some(Change::erase(s.start(), s.end()));
                        }
                        if s.head == 0 {
                            return None;
                        }
                        // Ganze Einrückung auf einmal löschen, wenn mit Leerzeichen eingerückt wird.
                        if use_spaces {
                            let line = self.doc.text.char_to_line(s.head);
                            let before = self.doc.slice(self.doc.line_start(line), s.head);
                            let n = before.chars().count();
                            if n > 0 && before.chars().all(|c| c == ' ') {
                                let del = if n % tab == 0 { tab } else { n % tab };
                                return Some(Change::erase(s.head - del, s.head));
                            }
                        }
                        let start = s.head - 1;
                        if self.o.auto_close && s.head < self.doc.len() {
                            let (prev, next) = (self.doc.text.char(start), self.doc.text.char(s.head));
                            if matches!((prev, next), ('(', ')') | ('[', ']') | ('{', '}') | ('"', '"') | ('\'', '\'')) {
                                return Some(Change::erase(start, s.head + 1));
                            }
                        }
                        Some(Change::erase(start, s.head))
                    })
                    .collect();
                if !changes.is_empty() {
                    self.change(Kind::Erase, changes);
                }
            }
            Cmd::Delete => {
                let n = self.doc.len();
                let changes: Vec<Change> = self
                    .v
                    .sels
                    .sels
                    .iter()
                    .filter_map(|s| {
                        if !s.is_empty() {
                            Some(Change::erase(s.start(), s.end()))
                        } else if s.head < n {
                            Some(Change::erase(s.head, s.head + 1))
                        } else {
                            None
                        }
                    })
                    .collect();
                if !changes.is_empty() {
                    self.change(Kind::Erase, changes);
                }
            }
            Cmd::WordBackspace | Cmd::WordDelete => {
                let left = matches!(cmd, Cmd::WordBackspace);
                let sels = self.v.sels.sels.clone();
                let changes: Vec<Change> = sels
                    .iter()
                    .filter_map(|s| {
                        if !s.is_empty() {
                            return Some(Change::erase(s.start(), s.end()));
                        }
                        let (a, b) = if left {
                            (self.word_left(s.head), s.head)
                        } else {
                            (s.head, self.word_right(s.head))
                        };
                        (a != b).then(|| Change::erase(a, b))
                    })
                    .collect();
                if !changes.is_empty() {
                    self.change(Kind::Other, changes);
                }
            }
            Cmd::Indent => {
                let multi_line = self.v.sels.sels.iter().any(|s| {
                    !s.is_empty() && self.doc.text.char_to_line(s.start()) != self.doc.text.char_to_line(s.end())
                });
                if multi_line {
                    let unit = self.indent_unit();
                    self.map_lines(|t| textops::indent(t, &unit));
                } else if self.o.use_spaces {
                    let tab = self.o.tab_size.max(1);
                    let sels = self.v.sels.sels.clone();
                    let changes = sels
                        .iter()
                        .map(|s| {
                            let line = self.doc.text.char_to_line(s.start());
                            let col = s.start() - self.doc.line_start(line);
                            Change::caret(s.start(), s.end(), " ".repeat(tab - col % tab))
                        })
                        .collect();
                    self.change(Kind::Other, changes);
                } else {
                    self.insert("\t", Kind::Other);
                }
            }
            Cmd::Outdent => {
                let tab = self.o.tab_size.max(1);
                self.map_lines(|t| textops::outdent(t, tab));
            }
            Cmd::Move(m, extend) => self.do_move(m, extend),
            Cmd::BoxMove(m) => self.box_move(m),
            Cmd::SelectAll => {
                let n = self.doc.len();
                self.v.sels = SelSet::range(0, n);
            }
            Cmd::Undo => {
                if let Some(s) = self.doc.undo() {
                    self.set(s);
                    self.v.want_reveal = true;
                }
            }
            Cmd::Redo => {
                if let Some(s) = self.doc.redo() {
                    self.set(s);
                    self.v.want_reveal = true;
                }
            }
            Cmd::Copy => return self.copy_text(),
            Cmd::Cut => {
                let t = self.copy_text();
                if t.is_some() {
                    self.delete_selection();
                }
                return t;
            }
            Cmd::DeleteSelection => self.delete_selection(),
            Cmd::DuplicateLines => {
                let (l0, l1) = self.touched_lines();
                let (a, b) = (self.doc.line_start(l0), self.doc.line_end(l1));
                let block = self.doc.slice(a, b);
                let after = self.v.sels.clone();
                self.doc.begin(Kind::Other, &after);
                self.doc.splice(b, b, &format!("\n{block}"));
                self.doc.commit(&after);
                self.set(after);
            }
            Cmd::DeleteLines => {
                let (l0, l1) = self.touched_lines();
                let n = self.doc.lines();
                let (a, b) = if l1 + 1 < n {
                    (self.doc.line_start(l0), self.doc.line_start(l1 + 1))
                } else if l0 > 0 {
                    (self.doc.line_end(l0 - 1), self.doc.line_end(l1))
                } else {
                    (0, self.doc.line_end(l1))
                };
                self.v.sels = SelSet::single(a);
                self.change(Kind::Other, vec![Change::erase(a, b)]);
            }
            Cmd::MoveLinesUp => self.move_lines(true),
            Cmd::MoveLinesDown => self.move_lines(false),
            Cmd::JoinLines => {
                let (l0, l1) = self.touched_lines();
                let l1 = if l1 == l0 { (l0 + 1).min(self.doc.lines() - 1) } else { l1 };
                let (a, b) = (self.doc.line_start(l0), self.doc.line_end(l1));
                self.map_range(a, b, textops::join_lines);
            }
            Cmd::Case(k) => {
                let sels = self.v.sels.sels.clone();
                let changes: Vec<Change> = sels
                    .iter()
                    .filter(|s| !s.is_empty())
                    .map(|s| {
                        let new = textops::change_case(&self.doc.slice(s.start(), s.end()), k);
                        Change { start: s.start(), end: s.end(), text: new, select: true, caret_back: 0 }
                    })
                    .collect();
                if !changes.is_empty() {
                    self.change(Kind::Other, changes);
                }
            }
            Cmd::SortLines { descending, ignore_case } => {
                self.map_selection_or_all(|t| textops::sort_lines(t, descending, ignore_case))
            }
            Cmd::DedupLines => self.map_selection_or_all(textops::dedup_lines),
            Cmd::RemoveEmptyLines { blank_too } => {
                self.map_selection_or_all(|t| textops::remove_empty_lines(t, blank_too))
            }
            Cmd::TrimTrailing => self.map_selection_or_all(textops::trim_trailing),
            Cmd::TrimLeading => self.map_selection_or_all(textops::trim_leading),
            Cmd::TabsToSpaces => {
                let tab = self.o.tab_size.max(1);
                self.map_selection_or_all(|t| textops::tabs_to_spaces(t, tab));
            }
            Cmd::LeadingSpacesToTabs => {
                let tab = self.o.tab_size.max(1);
                self.map_selection_or_all(|t| textops::leading_spaces_to_tabs(t, tab));
            }
            Cmd::Comment(mode) => {
                let (line, block) = highlight::comment_tokens(self.doc.hl.language.as_deref());
                if let Some(tok) = line {
                    self.map_lines(|t| textops::comment_lines(t, tok, mode));
                } else if block.is_some() {
                    self.comment_block();
                }
            }
            Cmd::BlockComment => self.comment_block(),
            Cmd::ToggleOverwrite => self.v.overwrite = !self.v.overwrite,
            Cmd::CollapseSelections => {
                let m = self.v.sels.main();
                self.v.sels = SelSet { sels: vec![m], primary: 0 };
            }
        }
        None
    }
}
