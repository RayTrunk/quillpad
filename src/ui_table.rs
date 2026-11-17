//! Tabellenansicht für CSV-Dokumente: eigener Painter, nur sichtbare Zeilen.
//! Der Text bleibt die Wahrheit. Die aktive Zelle ist die Zelle, in der die
//! Schreibmarke des Tabs steht; so folgen Suche, Rückgängig und die geteilte
//! Ansicht ohne eigene Buchführung. Jede Änderung geht als eine Gruppe in den Text.

use crate::csv::{self, cell_at, cell_pos, field_value, Dialect, Parsed, RowView};
use crate::doc::{Document, SelSet};
use crate::editor::{Metrics, Palette, View};
use egui::{pos2, Key, Modifiers, Rect, Sense, Ui, Vec2};

mod input;
mod paint;

/// Ab dieser Textgröße startet eine Tabellendatei nicht in der Tabelle.
pub const AUTO_TABLE_MAX_BYTES: usize = 50 << 20;
/// Datensätze, aus denen die Spaltenbreite geschätzt wird.
const SAMPLE_RECORDS: usize = 200;
/// Obergrenze der geschätzten Spaltenbreite in Zeichen.
const MAX_AUTO_CH: usize = 40;
/// „Passend“ per Doppelklick darf breiter werden, aber nicht endlos.
const MAX_FIT_CH: usize = 160;
const MIN_CH: f32 = 3.0;
const PAD: f32 = 6.0;
const SCROLL_W: f32 = 14.0;
/// Greifbreite am Spaltenrand.
const GRIP: f32 = 4.0;

pub struct TableState {
    pub rows: RowView,
    /// Spaltenbreiten in Punkten.
    pub widths: Vec<f32>,
    /// Bildlauf in Punkten (x waagrecht, y senkrecht).
    pub scroll: Vec2,
    /// Rechteckauswahl als (Datensatz, Spalte); das andere Ende ist die aktive Zelle.
    pub anchor: Option<(usize, usize)>,
    /// Inhalt der Zelle in Bearbeitung.
    pub editing: Option<String>,
    pub focused: bool,
    /// Dialekt, für den `rows` gebaut wurde; `None` = noch nie.
    built: Option<Dialect>,
    /// Zuletzt gesehene Revision; fremde Änderungen setzen die aktive Zelle.
    seen_rev: u64,
    /// Zeichenbreite, für die `widths` gelten (Zoom skaliert mit).
    width_ch: f32,
    /// Aktive Zelle, die (noch) kein Feld im Text hat: (Revision, Position, Datensatz, Spalte).
    virt: Option<(u64, usize, usize, usize)>,
    last_active: Option<(usize, usize)>,
    /// Spaltenrand wird gezogen: (Spalte, Start-x, Startbreite).
    resize: Option<(usize, f32, f32)>,
    sel_drag: bool,
    /// Bearbeitung eben begonnen: Fokus und Schreibmarke noch setzen.
    edit_fresh: bool,
    /// Zelle, deren Inhalt gerade bearbeitet wird (Datensatz, Spalte).
    edit_at: Option<(usize, usize)>,
    vbar_drag: bool,
    hbar_drag: bool,
}

impl TableState {
    pub fn new(rev: u64) -> TableState {
        TableState {
            rows: RowView::default(),
            widths: Vec::new(),
            scroll: Vec2::ZERO,
            anchor: None,
            editing: None,
            focused: false,
            built: None,
            seen_rev: rev,
            width_ch: 0.0,
            virt: None,
            last_active: None,
            resize: None,
            sel_drag: false,
            edit_fresh: false,
            edit_at: None,
            vbar_drag: false,
            hbar_drag: false,
        }
    }
}

#[derive(Default)]
pub struct TableOut {
    pub focused: bool,
    pub clipboard: Option<String>,
    pub status: Option<String>,
}

/// Startet dieses Dokument beim Öffnen in der Tabelle?
pub fn auto_table(doc: &Document) -> bool {
    doc.csv.is_some() && !doc.large && doc.text.len_bytes() < AUTO_TABLE_MAX_BYTES
}

/// Tasten, die eine Tabelle mit Fokus selbst behandelt; globale Kürzel treten dafür zurück.
pub fn claims_key(key: Key, m: Modifiers) -> bool {
    if m.alt || ((m.ctrl || m.command) && !matches!(key, Key::Home | Key::End)) {
        return false;
    }
    matches!(
        key,
        Key::F2 | Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight | Key::Enter | Key::Tab
            | Key::Delete | Key::Escape | Key::Home | Key::End | Key::PageUp | Key::PageDown | Key::Backspace
    )
}

/// Aktive Zelle (Datensatz, Spalte) dieses Tabs; für Tests und die Statusleiste.
#[cfg_attr(not(test), allow(dead_code))]
pub fn active_cell(doc: &mut Document, v: &View) -> (usize, usize) {
    let rev = doc.rev;
    let Some(c) = doc.csv.as_mut() else { return (0, 0) };
    let (p, _) = c.parsed(&doc.text, rev);
    locate(&p, v, v.table.as_ref().and_then(|t| t.virt), rev)
}

fn locate(p: &Parsed, v: &View, virt: Option<(u64, usize, usize, usize)>, rev: u64) -> (usize, usize) {
    let s = v.sels.main();
    if let Some((r, pos, rec, col)) = virt {
        if r == rev && s.is_empty() && s.head == pos {
            return (rec, col);
        }
    }
    cell_at(p, s.start())
}

/// Spaltenname ohne Kopfzeile: A … Z, AA, AB …
pub fn col_name(mut i: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + (i % 26) as u8);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

/// Erste Textzeile eines Werts und ob weitere folgen.
fn first_line(v: &str) -> (&str, bool) {
    match v.find('\n') {
        Some(i) => (&v[..i], true),
        None => (v, false),
    }
}

fn text_w(s: &str, m: &mut Metrics) -> f32 {
    s.chars().map(|c| m.w(c)).sum()
}

/// Breite für `n` Zeichen samt Innenabstand; aufgerundet, damit genau passender Inhalt nicht gekürzt wird.
fn col_width(n: usize, m: &Metrics) -> f32 {
    (n.max(MIN_CH as usize) as f32 * m.ch + 2.0 * PAD + 2.0).ceil()
}

/// Kürzt auf `max` Punkte, mit „…“ am Ende.
fn fit(s: &str, max: f32, m: &mut Metrics) -> String {
    if text_w(s, m) <= max {
        return s.to_string();
    }
    let ell = m.w('…');
    let mut w = 0.0;
    let mut out = String::new();
    for c in s.chars() {
        let cw = m.w(c);
        if w + cw + ell > max {
            break;
        }
        w += cw;
        out.push(c);
    }
    out.push('…');
    out
}

/// Breite einer Spalte aus den ersten Datensätzen, in Zeichen.
fn content_chars(p: &Parsed, chars: &[char], col: usize, records: usize, cap: usize) -> usize {
    p.records
        .iter()
        .take(records)
        .filter_map(|r| r.fields.get(col))
        .map(|f| {
            let v = field_value(chars, f);
            let (l, more) = first_line(&v);
            l.chars().count() + usize::from(more) * 2
        })
        .max()
        .unwrap_or(0)
        .min(cap)
}

/// Erste geänderte Textposition seit Revision `since`, umgerechnet auf den jetzigen Text.
fn first_change(doc: &Document, since: u64) -> Option<usize> {
    let mut recs = doc.log.iter().filter(|r| r.rev > since).peekable();
    // Lücke im Protokoll: lieber nichts verschieben als falsch.
    if recs.peek().is_none_or(|r| r.rev != since + 1) {
        return None;
    }
    let mut pos: Option<usize> = None;
    for r in recs {
        let mapped = pos.map(|p| {
            if p <= r.start {
                p
            } else if p >= r.start + r.removed {
                p - r.removed + r.inserted
            } else {
                r.start + r.inserted
            }
        });
        pos = Some(mapped.map_or(r.start, |p| p.min(r.start)));
    }
    pos
}

/// Position aus Revision `since` auf den jetzigen Text umrechnen.
fn map_pos(doc: &Document, since: u64, mut p: usize) -> usize {
    for r in doc.log.iter().filter(|r| r.rev > since) {
        if p >= r.start + r.removed {
            p = p - r.removed + r.inserted;
        } else if p > r.start {
            p = r.start + r.inserted;
        }
    }
    p.min(doc.len())
}

/// Zieht den Dialekt fürs Anzeigen heran: Ein leeres Dokument hat keine Kopfzeile,
/// damit es eine bearbeitbare Zelle gibt.
fn shown_dialect(doc: &Document) -> Option<Dialect> {
    let d = doc.csv.as_ref()?.dialect;
    Some(Dialect { header: d.header && doc.len() > 0, ..d })
}

fn parsed(doc: &mut Document) -> (std::sync::Arc<Parsed>, std::sync::Arc<Vec<char>>) {
    let rev = doc.rev;
    let c = doc.csv.as_mut().expect("Tabelle ohne CSV-Dialekt");
    c.parsed(&doc.text, rev)
}

fn set_active(v: &mut View, st: &mut TableState, p: &Parsed, rev: u64, rec: usize, col: usize) {
    let pos = cell_pos(p, rec, col);
    v.set_sels(SelSet::single(pos));
    st.virt = (cell_at(p, pos) != (rec, col)).then_some((rev, pos, rec, col));
}

/// Maße eines Durchlaufs: ganze Fläche, Zellbereich ohne Kopf, Nummern und Leisten.
#[derive(Clone, Copy)]
struct Layout {
    rect: Rect,
    body: Rect,
    row_h: f32,
    head_h: f32,
    num_w: f32,
    ncols: usize,
    nrows: usize,
    need_v: bool,
    need_h: bool,
    vbar: Rect,
    hbar: Rect,
}

impl Layout {
    fn col_x(&self, widths: &[f32], scroll_x: f32, col: usize) -> f32 {
        self.body.left() - scroll_x + widths[..col].iter().sum::<f32>()
    }

    fn hit_col(&self, widths: &[f32], scroll_x: f32, x: f32) -> Option<usize> {
        let mut x0 = self.body.left() - scroll_x;
        for (c, w) in widths[..self.ncols].iter().enumerate() {
            if x < x0 + w {
                return (x >= x0).then_some(c);
            }
            x0 += w;
        }
        None
    }

    /// Spaltenrand unter dem Zeiger (in der Kopfzeile ziehbar).
    fn hit_border(&self, widths: &[f32], scroll_x: f32, x: f32) -> Option<usize> {
        let mut x0 = self.body.left() - scroll_x;
        for (c, w) in widths[..self.ncols].iter().enumerate() {
            x0 += w;
            if (x - x0).abs() <= GRIP && x0 > self.body.left() {
                return Some(c);
            }
        }
        None
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Step {
    Stay,
    Down,
    Up,
    Right,
    Left,
}

/// Alles, was eine Bildschirmzeile braucht, für einen Durchlauf.
struct Ctx<'a> {
    doc: &'a mut Document,
    v: &'a mut View,
    st: &'a mut TableState,
    d: Dialect,
    ncols: usize,
    out: &'a mut TableOut,
}

impl Ctx<'_> {
    fn active(&self, p: &Parsed) -> (usize, usize) {
        locate(p, self.v, self.st.virt, self.doc.rev)
    }

    fn view_row(&self, rec: usize) -> Option<usize> {
        self.st.rows.rows.iter().position(|&r| r == rec)
    }

    /// Eigene Änderung übernehmen: eine Gruppe, Ansicht und Schreibmarke mitführen.
    fn apply(&mut self, changes: Vec<csv::Change>, before: usize, after: usize, structural: bool) -> bool {
        if self.doc.read_only {
            self.out.status = Some(crate::t!("find.read_only"));
            return false;
        }
        if !csv::apply_changes(self.doc, changes, &SelSet::single(before), after) {
            return false;
        }
        let rev = self.doc.rev;
        if !structural {
            self.st.rows.mark_own(rev);
        }
        self.st.seen_rev = rev;
        self.v.seen_rev = rev;
        self.v.set_sels(SelSet::single(after));
        self.st.virt = None;
        true
    }

    /// Bearbeitung in die bearbeitete Zelle übernehmen und weitergehen. Ist die aktive Zelle
    /// inzwischen woanders (Suche, Springen), bleibt sie dort und rutscht nur mit dem Text mit.
    fn commit(&mut self, step: Step) {
        let Some(value) = self.st.editing.take() else { return };
        let (p, _) = parsed(self.doc);
        let active = self.active(&p);
        let (rec, col) = self.st.edit_at.take().unwrap_or(active);
        if (rec, col) != active {
            let keep = self.v.sels.clone();
            let since = self.doc.rev;
            let text = self.doc.full_text();
            let changes = csv::set_cell(&text, &p, self.d, rec, col, &value);
            let at = cell_pos(&p, rec, col);
            if self.apply(changes, at, at, false) {
                let mut keep = keep;
                for s in keep.sels.iter_mut() {
                    s.anchor = map_pos(self.doc, since, s.anchor);
                    s.head = map_pos(self.doc, since, s.head);
                }
                self.v.set_sels(keep);
            }
            return;
        }
        let text = self.doc.full_text();
        let changes = csv::set_cell(&text, &p, self.d, rec, col, &value);
        let before = cell_pos(&p, rec, col);
        let after = changes.first().map_or(before, |c| {
            // Ein fehlendes Feld kommt hinter den ergänzten Trennzeichen zu stehen.
            let lead = c.text.chars().take_while(|&ch| ch == self.d.delim).count();
            c.start + lead + usize::from(c.text.chars().nth(lead) == Some('"'))
        });
        self.apply(changes, before, after, false);
        let (p, _) = parsed(self.doc);
        let rev = self.doc.rev;
        set_active(self.v, self.st, &p, rev, rec, col);
        self.step(&p, step, false);
    }

    fn step(&mut self, p: &Parsed, step: Step, extend: bool) {
        match step {
            Step::Stay => {}
            Step::Down => self.move_by(p, 1, 0, extend),
            Step::Up => self.move_by(p, -1, 0, extend),
            Step::Right => self.move_by(p, 0, 1, extend),
            Step::Left => self.move_by(p, 0, -1, extend),
        }
    }

    fn move_to(&mut self, p: &Parsed, vrow: usize, col: usize, extend: bool) {
        let n = self.st.rows.rows.len();
        if n == 0 {
            return;
        }
        let cur = self.active(p);
        if extend {
            self.st.anchor.get_or_insert(cur);
        } else {
            self.st.anchor = None;
        }
        let rec = self.st.rows.rows[vrow.min(n - 1)];
        let rev = self.doc.rev;
        set_active(self.v, self.st, p, rev, rec, col.min(self.ncols - 1));
    }

    fn move_by(&mut self, p: &Parsed, dr: isize, dc: isize, extend: bool) {
        let (rec, col) = self.active(p);
        let vr = self.view_row(rec).unwrap_or(0);
        let vr = vr.saturating_add_signed(dr);
        let col = col.saturating_add_signed(dc);
        self.move_to(p, vr, col, extend);
    }

    /// Ausgewählte Zellen: Bereich der Ansichtszeilen und Spalten.
    fn selection(&self, p: &Parsed) -> (usize, usize, usize, usize) {
        let (rec, col) = self.active(p);
        let vr = self.view_row(rec).unwrap_or(0);
        match self.st.anchor.and_then(|(ar, ac)| self.view_row(ar).map(|avr| (avr, ac))) {
            Some((avr, ac)) => (vr.min(avr), vr.max(avr), col.min(ac), col.max(ac)),
            None => (vr, vr, col, col),
        }
    }

    fn selected_values(&self, p: &Parsed, chars: &[char]) -> Vec<Vec<String>> {
        let (r0, r1, c0, c1) = self.selection(p);
        (r0..=r1)
            .filter_map(|vr| self.st.rows.rows.get(vr))
            .map(|&rec| {
                (c0..=c1)
                    .map(|c| p.records.get(rec).and_then(|r| r.fields.get(c)).map(|f| field_value(chars, f)).unwrap_or_default())
                    .collect()
            })
            .collect()
    }

    fn clear_selection(&mut self, p: &Parsed, chars: &[char]) {
        if self.st.rows.rows.is_empty() {
            return;
        }
        let (r0, r1, c0, c1) = self.selection(p);
        let cells: Vec<(usize, usize)> = self.st.rows.rows[r0..=r1.min(self.st.rows.rows.len() - 1)]
            .iter()
            .flat_map(|&rec| (c0..=c1).map(move |c| (rec, c)))
            .collect();
        let changes = csv::clear_cells(chars, p, self.d.delim, &cells);
        let (rec, col) = self.active(p);
        let at = cell_pos(p, rec, col);
        let anchor = self.st.anchor;
        if self.apply(changes, at, at, false) {
            let (p, _) = parsed(self.doc);
            let rev = self.doc.rev;
            set_active(self.v, self.st, &p, rev, rec, col);
            self.st.anchor = anchor;
        }
    }

    fn paste(&mut self, p: &Parsed, t: &str) {
        let block = csv::from_tsv(t);
        if block.is_empty() {
            return;
        }
        let (r0, _, c0, _) = self.selection(p);
        let Some(&rec) = self.st.rows.rows.get(r0) else { return };
        let text = self.doc.full_text();
        let (changes, cut) = csv::paste_block(&text, p, self.d, rec, c0, &block);
        let at = cell_pos(p, rec, c0);
        if self.apply(changes, at, at, true) {
            let (p, _) = parsed(self.doc);
            let rev = self.doc.rev;
            set_active(self.v, self.st, &p, rev, rec, c0);
            self.st.anchor = None;
        }
        if cut {
            self.out.status = Some(crate::t!("table.paste_cut"));
        }
    }

    fn undo(&mut self, redo: bool) {
        if self.doc.read_only {
            return;
        }
        let s = if redo { self.doc.redo() } else { self.doc.undo() };
        if let Some(s) = s {
            self.v.set_sels(s);
            self.v.seen_rev = self.doc.rev;
            self.st.seen_rev = self.doc.rev;
            self.st.virt = None;
            self.st.anchor = None;
        }
    }

    fn start_edit(&mut self, p: &Parsed, chars: &[char], replace: Option<String>) {
        if self.doc.read_only {
            self.out.status = Some(crate::t!("find.read_only"));
            return;
        }
        if self.st.rows.rows.is_empty() {
            return;
        }
        let (rec, col) = self.active(p);
        let value = replace.unwrap_or_else(|| {
            p.records.get(rec).and_then(|r| r.fields.get(col)).map(|f| field_value(chars, f)).unwrap_or_default()
        });
        self.st.anchor = None;
        self.st.editing = Some(value);
        self.st.edit_fresh = true;
        self.st.edit_at = Some((rec, col));
    }
}

/// Offene Zellbearbeitung übernehmen, z. B. vor Speichern, Schließen oder Tabwechsel.
/// `refocus`: die Tabelle bekommt den Fokus zurück (nicht, wenn die Aktion ihn woandershin legt).
/// Hat sich der Text inzwischen fremd geändert, verfällt die Bearbeitung wie beim Zeichnen.
pub fn commit_edit(doc: &mut Document, v: &mut View, refocus: bool) {
    let Some(mut st) = v.table.take() else { return };
    if st.editing.is_some() && st.seen_rev != doc.rev {
        st.editing = None;
        st.edit_at = None;
    }
    if st.editing.is_some() {
        match shown_dialect(doc) {
            Some(d) => {
                let (p, _) = parsed(doc);
                let ncols = p.records.iter().map(|r| r.fields.len()).max().unwrap_or(1).max(1);
                let mut out = TableOut::default();
                Ctx { doc, v, st: &mut st, d, ncols, out: &mut out }.commit(Step::Stay);
                // Wer gerade tippte, tippt in der Tabelle weiter.
                v.want_focus |= refocus && st.focused;
            }
            None => st.editing = None,
        }
    }
    v.table = Some(st);
}

pub fn show(ui: &mut Ui, doc: &mut Document, v: &mut View, pal: &Palette, m: &mut Metrics, retro: bool) -> TableOut {
    let mut out = TableOut::default();
    let Some(mut st) = v.table.take() else { return out };
    if let Some(d) = shown_dialect(doc) {
        draw(ui, doc, v, &mut st, d, pal, m, retro, &mut out);
    }
    st.focused = out.focused;
    v.table = Some(st);
    out
}

#[allow(clippy::too_many_arguments)]
fn draw(
    ui: &mut Ui,
    doc: &mut Document,
    v: &mut View,
    st: &mut TableState,
    d: Dialect,
    pal: &Palette,
    m: &mut Metrics,
    retro: bool,
    out: &mut TableOut,
) {
    let rect = ui.max_rect();
    let ctx = ui.ctx().clone();
    let base_id = egui::Id::new(("table", v.uid));
    let edit_id = base_id.with("edit");

    // Fremde Änderung (Textansicht, Rückgängig dort, Ersetzen): die Zelle der ersten Änderung wird aktiv.
    if st.seen_rev != doc.rev {
        if let Some(pos) = first_change(doc, st.seen_rev) {
            v.set_sels(SelSet::single(pos.min(doc.len())));
        }
        st.seen_rev = doc.rev;
        st.virt = None;
        st.anchor = None;
        st.editing = None;
    }
    let (mut p, mut chars) = parsed(doc);
    if st.built != Some(d) || st.rows.needs_rebuild(doc.rev) {
        st.rows.rebuild(&chars, &p, d, doc.rev);
        st.built = Some(d);
    }
    let ncols = p.records.iter().map(|r| r.fields.len()).max().unwrap_or(1).max(1);

    // Spaltenbreiten
    if st.width_ch > 0.0 && (st.width_ch - m.ch).abs() > 0.01 {
        let k = m.ch / st.width_ch;
        st.widths.iter_mut().for_each(|w| *w = (*w * k).round());
    }
    st.width_ch = m.ch;
    while st.widths.len() < ncols {
        let col = st.widths.len();
        let mut n = content_chars(&p, &chars, col, SAMPLE_RECORDS, MAX_AUTO_CH);
        if !d.header {
            n = n.max(col_name(col).len());
        }
        st.widths.push(col_width(n, m));
    }

    let mut cx = Ctx { doc, v, st, d, ncols, out };

    // Kopfzeile ist keine Datenzeile: die Schreibmarke dorthin rutscht auf die erste Zeile.
    {
        let (rec, col) = cx.active(&p);
        if cx.view_row(rec).is_none() && !cx.st.rows.rows.is_empty() {
            let first = cx.st.rows.rows[0];
            let rev = cx.doc.rev;
            set_active(cx.v, cx.st, &p, rev, first, col.min(ncols - 1));
        }
    }

    // Aktive Zelle unter offener Bearbeitung verschoben (Suche, Springen): erst übernehmen.
    if cx.st.editing.is_some() && cx.st.edit_at.is_some_and(|e| e != cx.active(&p)) {
        cx.v.want_focus |= ctx.memory(|mem| mem.has_focus(edit_id));
        cx.commit(Step::Stay);
        (p, chars) = parsed(cx.doc);
    }

    // Aufteilung
    let row_h = (m.line_h + 6.0).round();
    let head_h = row_h;
    let digits = (p.records.len().max(1) as f64).log10().floor() as usize + 1;
    let num_w = (digits.max(2) as f32 * m.ch + 2.0 * PAD + 4.0).round();
    let content_w: f32 = cx.st.widths[..ncols].iter().sum();
    let nrows = cx.st.rows.rows.len();
    let content_h = nrows as f32 * row_h;
    let avail_w = rect.width() - num_w;
    let avail_h = rect.height() - head_h;
    let mut need_v = content_h > avail_h;
    let need_h = content_w > avail_w - if need_v { SCROLL_W } else { 0.0 };
    need_v = content_h > avail_h - if need_h { SCROLL_W } else { 0.0 };
    let body = Rect::from_min_max(
        pos2(rect.left() + num_w, rect.top() + head_h),
        pos2(rect.right() - if need_v { SCROLL_W } else { 0.0 }, rect.bottom() - if need_h { SCROLL_W } else { 0.0 }),
    );
    let page_rows = ((body.height() / row_h).floor() as usize).max(1);

    let resp = ui.interact(rect, base_id, Sense::click_and_drag());
    if cx.v.want_focus {
        cx.v.want_focus = false;
        if cx.st.editing.is_some() {
            cx.st.edit_fresh = true;
        } else {
            resp.request_focus();
        }
    }

    // Mausrad
    if ui.rect_contains_pointer(rect) {
        let delta = ui.input(|i| i.smooth_scroll_delta);
        cx.st.scroll -= delta;
    }

    let vbar = Rect::from_min_max(pos2(body.right(), body.top()), pos2(rect.right(), body.bottom()));
    let hbar = Rect::from_min_max(pos2(body.left(), body.bottom()), pos2(body.right(), rect.bottom()));
    let lay = Layout { rect, body, row_h, head_h, num_w, ncols, nrows, need_v, need_h, vbar, hbar };
    input::pointer(ui, &mut cx, &lay, &resp, &mut p, &mut chars, m);
    input::keys(ui, &mut cx, &resp, base_id, edit_id, &mut p, &mut chars, page_rows);

    // Aktive Zelle sichtbar halten
    let active = cx.active(&p);
    let active_vr = cx.view_row(active.0);
    if cx.st.last_active != Some(active) {
        cx.st.last_active = Some(active);
        if let Some(vr) = active_vr {
            let y = vr as f32 * row_h;
            if y < cx.st.scroll.y {
                cx.st.scroll.y = y;
            } else if y + row_h > cx.st.scroll.y + body.height() {
                cx.st.scroll.y = y + row_h - body.height();
            }
        }
        let c = active.1.min(ncols - 1);
        let x0: f32 = cx.st.widths[..c].iter().sum();
        let x1 = x0 + cx.st.widths[c];
        if x0 < cx.st.scroll.x {
            cx.st.scroll.x = x0;
        } else if x1 > cx.st.scroll.x + body.width() {
            cx.st.scroll.x = (x1 - body.width()).min(x0);
        }
    }

    let content_w: f32 = cx.st.widths[..ncols].iter().sum();
    let max_x = (content_w - body.width()).max(0.0);
    let max_y = (nrows as f32 * row_h - body.height()).max(0.0);
    cx.st.scroll.x = cx.st.scroll.x.clamp(0.0, max_x);
    cx.st.scroll.y = cx.st.scroll.y.clamp(0.0, max_y);

    let has_focus = resp.has_focus() || ctx.memory(|mem| mem.has_focus(edit_id)) || cx.st.edit_fresh;
    cx.out.focused = has_focus;
    paint::paint(ui, &mut cx, &lay, &p, &chars, has_focus, pal, m, retro, edit_id);

    // Bildlaufleisten zuletzt, damit sie über den Zellen liegen; ein Ziehen wirkt ab dem nächsten Bild.
    let before = cx.st.scroll;
    if need_v {
        paint::scrollbar(ui, base_id.with("v"), vbar, true, nrows as f32 * row_h, body.height(), &mut cx.st.scroll.y, &mut cx.st.vbar_drag, pal);
    }
    if need_h {
        paint::scrollbar(ui, base_id.with("h"), hbar, false, content_w, body.width(), &mut cx.st.scroll.x, &mut cx.st.hbar_drag, pal);
    }
    cx.st.scroll.x = cx.st.scroll.x.clamp(0.0, max_x);
    cx.st.scroll.y = cx.st.scroll.y.clamp(0.0, max_y);
    if cx.st.scroll != before {
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_names_count_like_spreadsheets() {
        assert_eq!(col_name(0), "A");
        assert_eq!(col_name(25), "Z");
        assert_eq!(col_name(26), "AA");
        assert_eq!(col_name(27), "AB");
        assert_eq!(col_name(701), "ZZ");
        assert_eq!(col_name(702), "AAA");
    }

    #[test]
    fn table_claims_navigation_but_not_ctrl_combos() {
        assert!(claims_key(Key::F2, Modifiers::NONE));
        assert!(claims_key(Key::F2, Modifiers::SHIFT));
        assert!(claims_key(Key::Enter, Modifiers::NONE));
        assert!(claims_key(Key::Home, Modifiers::CTRL));
        assert!(!claims_key(Key::F2, Modifiers::CTRL));
        assert!(!claims_key(Key::F3, Modifiers::NONE));
        assert!(!claims_key(Key::Tab, Modifiers::CTRL));
    }
}
