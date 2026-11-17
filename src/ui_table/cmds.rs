//! Befehle der Tabelle: Zeilen, Spalten, Filter und „Reihenfolge übernehmen“.
//! Menü „Tabelle“, Kontextmenü und Kürzel landen alle hier; jede Änderung ist eine Gruppe.
//! Die Zeilenbefehle arbeiten auf rohen Datensatzindizes, die Kopfzeile schützt diese Stelle.

use super::{cell_pos, ncols_of, parsed, prepare, set_active, shown_dialect, Ctx, TableOut};
use crate::csv::{self, Parsed};
use crate::doc::Document;
use crate::editor::View;
use egui::{Button, Ui};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableCmd {
    RowAbove,
    RowBelow,
    DeleteRows,
    RowUp,
    RowDown,
    ColLeft,
    ColRight,
    DeleteCol,
    ColMoveLeft,
    ColMoveRight,
    ShowFilter,
    ApplyOrder,
    ClearSort,
}

impl TableCmd {
    pub fn label(self) -> String {
        match self {
            TableCmd::RowAbove => crate::t!("table.row_above"),
            TableCmd::RowBelow => crate::t!("table.row_below"),
            TableCmd::DeleteRows => crate::t!("table.delete_rows"),
            TableCmd::RowUp => crate::t!("table.row_up"),
            TableCmd::RowDown => crate::t!("table.row_down"),
            TableCmd::ColLeft => crate::t!("table.col_left"),
            TableCmd::ColRight => crate::t!("table.col_right"),
            TableCmd::DeleteCol => crate::t!("table.delete_col"),
            TableCmd::ColMoveLeft => crate::t!("table.col_move_left"),
            TableCmd::ColMoveRight => crate::t!("table.col_move_right"),
            TableCmd::ShowFilter => crate::t!("table.show_filter"),
            TableCmd::ApplyOrder => crate::t!("table.apply_order"),
            TableCmd::ClearSort => crate::t!("table.clear_sort"),
        }
    }

    /// Kürzel wie in der Kürzeltabelle (englische Schreibweise).
    pub fn keys(self) -> &'static str {
        match self {
            TableCmd::RowBelow => "Ctrl+Enter",
            TableCmd::DeleteRows => "Ctrl+Shift+Del",
            TableCmd::ShowFilter => "Ctrl+Shift+L",
            _ => "",
        }
    }
}

/// Was gerade möglich ist; gesperrte Einträge bleiben sichtbar, nur grau.
#[derive(Clone, Copy, Debug, Default)]
pub struct CmdState {
    edit: bool,
    identity: bool,
    sorted: bool,
    filtered: bool,
    has_rows: bool,
    can_up: bool,
    can_down: bool,
    col: usize,
    ncols: usize,
}

impl CmdState {
    pub fn enabled(&self, c: TableCmd) -> bool {
        use TableCmd as T;
        match c {
            T::RowAbove | T::RowBelow | T::ColLeft | T::ColRight => self.edit,
            T::DeleteRows => self.edit && self.has_rows,
            // Bei Sortierung oder Filter hätte „oben“ keine feste Bedeutung im Text.
            T::RowUp => self.edit && self.identity && self.can_up,
            T::RowDown => self.edit && self.identity && self.can_down,
            T::DeleteCol => self.edit && self.ncols > 1,
            T::ColMoveLeft => self.edit && self.col > 0,
            T::ColMoveRight => self.edit && self.col + 1 < self.ncols,
            T::ShowFilter => true,
            // Ausgeblendete Zeilen hätten keine definierte Position.
            T::ApplyOrder => self.edit && self.sorted && !self.filtered,
            T::ClearSort => self.sorted,
        }
    }
}

/// Einträge für Menü „Tabelle“ und Kontextmenü; gibt den gewählten Befehl zurück.
pub fn menu_items(ui: &mut Ui, s: &CmdState) -> Option<TableCmd> {
    use TableCmd as T;
    let groups: [&[TableCmd]; 3] = [
        &[T::RowAbove, T::RowBelow, T::DeleteRows, T::RowUp, T::RowDown],
        &[T::ColLeft, T::ColRight, T::DeleteCol, T::ColMoveLeft, T::ColMoveRight],
        &[T::ShowFilter, T::ApplyOrder, T::ClearSort],
    ];
    let mut picked = None;
    for (i, g) in groups.iter().enumerate() {
        if i > 0 {
            ui.separator();
        }
        for &c in g.iter() {
            let mut b = Button::new(c.label());
            if !c.keys().is_empty() {
                b = b.shortcut_text(crate::i18n::keys(c.keys()));
            }
            if ui.add_enabled(s.enabled(c), b).clicked() {
                picked = Some(c);
                ui.close_menu();
            }
        }
    }
    picked
}

/// Zustand der Befehle für den Tab; `None` ohne Tabellenansicht.
pub fn cmd_state(doc: &mut Document, v: &mut View) -> Option<CmdState> {
    let mut st = v.table.take()?;
    let state = shown_dialect(doc).map(|d| {
        let (p, _) = prepare(doc, v, &mut st, d);
        let ncols = ncols_of(&p);
        let mut out = TableOut::default();
        Ctx { doc, v, st: &mut st, d, ncols, out: &mut out }.cmd_state(&p)
    });
    v.table = Some(st);
    Some(state.unwrap_or_default())
}

/// Führt einen Befehl im Tab aus. Die Tabelle behält danach den Fokus, außer beim Filter.
pub fn run_cmd(doc: &mut Document, v: &mut View, c: TableCmd) -> TableOut {
    let mut out = TableOut::default();
    let Some(mut st) = v.table.take() else { return out };
    if let Some(d) = shown_dialect(doc) {
        let (p, _) = prepare(doc, v, &mut st, d);
        let ncols = ncols_of(&p);
        let mut cx = Ctx { doc, v, st: &mut st, d, ncols, out: &mut out };
        cx.leave_header(&p);
        cx.run(&p, c);
        if c != TableCmd::ShowFilter {
            cx.v.want_focus = true;
        }
    }
    v.table = Some(st);
    out
}

impl Ctx<'_> {
    pub(super) fn cmd_state(&self, p: &Parsed) -> CmdState {
        let rows = &self.st.rows;
        let first = usize::from(self.d.header);
        let has_rows = !rows.rows.is_empty();
        let (r0, r1, _, _) = self.selection(p);
        CmdState {
            edit: !self.doc.read_only,
            identity: rows.is_identity(),
            sorted: rows.sort.is_some(),
            filtered: !rows.filter.is_empty(),
            has_rows,
            can_up: rows.rows.get(r0).is_some_and(|&r| r > first),
            can_down: rows.rows.get(r1).is_some_and(|&r| r + 1 < p.records.len()),
            col: self.active(p).1,
            ncols: self.ncols,
        }
    }

    /// Sortier- und Filterspalte folgen eingefügten, gelöschten und verschobenen Spalten.
    fn remap_cols(&mut self, f: impl Fn(usize) -> Option<usize>) {
        let rows = &mut self.st.rows;
        rows.sort = rows.sort.and_then(|(c, dir)| f(c).map(|c| (c, dir)));
        rows.filter_col = rows.filter_col.and_then(&f);
    }

    /// Nach einer Strukturänderung: Ansicht neu, Zelle (Datensatz, Spalte) aktiv.
    fn after_struct(&mut self, rec: usize, col: usize) {
        self.rebuild_view();
        let (p, _) = parsed(self.doc);
        let rev = self.doc.rev;
        self.st.anchor = None;
        set_active(self.v, self.st, &p, rev, rec, col);
    }

    pub(super) fn run(&mut self, p: &Parsed, c: TableCmd) {
        use TableCmd as T;
        if !self.cmd_state(p).enabled(c) {
            return;
        }
        let (rec, col) = self.active(p);
        let (r0, r1, _, _) = self.selection(p);
        let first = usize::from(self.d.header);
        let before = cell_pos(p, rec, col);
        let n = self.st.rows.rows.len();
        // Zeichen aus dem Zwischenspeicher, passend zu `p`: kein Kopieren des ganzen Texts.
        let (_, chars) = parsed(self.doc);
        match c {
            T::RowAbove | T::RowBelow => {
                // Im Text direkt vor bzw. nach dem Datensatz der aktiven Zelle, in der Ansicht
                // an derselben Stelle; nie oberhalb der Kopfzeile.
                let (at, at_view) = match self.view_row(rec) {
                    Some(vr) if c == T::RowAbove => (rec, vr),
                    Some(vr) => (rec + 1, vr + 1),
                    None => (p.records.len(), n),
                };
                let at = at.max(first);
                let changes = csv::insert_row(&chars, p, self.d, at);
                let after = changes.first().map_or(before, |ch| ch.start + usize::from(ch.text.starts_with('\n')));
                if self.apply(changes, before, after, true) {
                    let rev = self.doc.rev;
                    self.st.rows.after_own_insert(at_view, at, rev);
                    self.st.anchor = None;
                    let (p, _) = parsed(self.doc);
                    set_active(self.v, self.st, &p, rev, at, col);
                }
            }
            T::DeleteRows => {
                let recs: Vec<usize> = self.st.rows.rows[r0..=r1.min(n - 1)].iter().copied().filter(|&r| r >= first).collect();
                let changes = csv::delete_rows(&chars, p, &recs);
                let after = changes.iter().map(|ch| ch.start).min().unwrap_or(before);
                if self.apply(changes, before, after, true) {
                    let rev = self.doc.rev;
                    self.st.rows.after_own_delete(&recs, rev);
                    self.st.anchor = None;
                    let left = self.st.rows.rows.len();
                    if left > 0 {
                        let next = self.st.rows.rows[r0.min(left - 1)];
                        let (p, _) = parsed(self.doc);
                        set_active(self.v, self.st, &p, rev, next, col);
                    }
                }
            }
            T::RowUp | T::RowDown => {
                // Nur ohne Sortierung und Filter: Ansichtszeilen sind dann aufeinanderfolgende Datensätze.
                let up = c == T::RowUp;
                let (a, b) = (self.st.rows.rows[r0], self.st.rows.rows[r1.min(n - 1)]);
                let changes = if a == b {
                    csv::move_row(&chars, p, a, up)
                } else {
                    // Block um eins verschieben: der Nachbar wandert auf die andere Seite.
                    let mut order: Vec<usize> = (first..p.records.len()).collect();
                    if up {
                        order[a - 1 - first..=b - first].rotate_left(1);
                    } else {
                        order[a - first..=b + 1 - first].rotate_right(1);
                    }
                    csv::apply_order(&chars, p, self.d, &order)
                };
                let anchor = self.st.anchor;
                if self.apply(changes, before, before, true) {
                    let shift = |r: usize| if up { r - 1 } else { r + 1 };
                    self.after_struct(shift(rec), col);
                    self.st.anchor = anchor.map(|(r, c)| (shift(r), c));
                }
            }
            T::ColLeft | T::ColRight => {
                let at = if c == T::ColLeft { col } else { col + 1 };
                let changes = csv::insert_col(p, self.d, at);
                if self.apply(changes, before, before, true) {
                    if let Some(&w) = self.st.widths.get(col) {
                        self.st.widths.insert(at.min(self.st.widths.len()), w);
                    }
                    self.remap_cols(|k| Some(if k >= at { k + 1 } else { k }));
                    self.after_struct(rec, at);
                }
            }
            T::DeleteCol => {
                let changes = csv::delete_col(p, col);
                if self.apply(changes, before, before, true) {
                    if col < self.st.widths.len() {
                        self.st.widths.remove(col);
                    }
                    self.remap_cols(|k| match k.cmp(&col) {
                        std::cmp::Ordering::Less => Some(k),
                        std::cmp::Ordering::Equal => None,
                        std::cmp::Ordering::Greater => Some(k - 1),
                    });
                    self.after_struct(rec, col.min(self.ncols.saturating_sub(2)));
                }
            }
            T::ColMoveLeft | T::ColMoveRight => {
                let left = c == T::ColMoveLeft;
                let to = if left { col - 1 } else { col + 1 };
                let changes = csv::move_col(&chars, p, self.d, col, left);
                if self.apply(changes, before, before, true) {
                    if to < self.st.widths.len() && col < self.st.widths.len() {
                        self.st.widths.swap(col, to);
                    }
                    self.remap_cols(|k| Some(if k == col { to } else if k == to { col } else { k }));
                    self.after_struct(rec, to);
                }
            }
            T::ShowFilter => {
                self.st.filter_open = true;
                self.st.filter_focus = true;
            }
            T::ApplyOrder => {
                // Datensätze bytegleich in Ansichtsreihenfolge; danach ist die Sortierung aus.
                let order = self.st.rows.rows.clone();
                let target = self.view_row(rec).map_or(rec, |vr| first + vr);
                let changes = csv::apply_order(&chars, p, self.d, &order);
                self.st.rows.sort = None;
                self.apply(changes, before, before, true);
                self.after_struct(target, col);
            }
            T::ClearSort => {
                self.st.rows.sort = None;
                self.rebuild_view();
            }
        }
    }
}
