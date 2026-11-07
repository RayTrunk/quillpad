//! Das Schreibfeld: selbst gezeichnet, zeichnet nur die sichtbaren Zeilen und bleibt
//! dadurch auch bei großen Dateien flüssig.
use crate::commands::{Cmd, Ed, Motion};
use crate::doc::{Document, Sel, SelSet};
use egui::{
    pos2, vec2, Align2, Color32, Context, Event, FontFamily, FontId, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct EdOpts {
    pub wrap: bool,
    pub show_ws: bool,
    pub show_eol: bool,
    pub indent_guides: bool,
    pub line_numbers: bool,
    pub tab_size: usize,
    pub use_spaces: bool,
    pub auto_indent: bool,
    pub auto_close: bool,
    pub smart_highlight: bool,
    pub highlight_line: bool,
    pub brace_match: bool,
    pub auto_complete: bool,
    pub auto_complete_from: usize,
    pub auto_complete_ignore_numbers: bool,
}

impl Default for EdOpts {
    fn default() -> Self {
        EdOpts {
            wrap: false,
            show_ws: false,
            show_eol: false,
            indent_guides: true,
            line_numbers: true,
            tab_size: 4,
            use_spaces: false,
            auto_indent: true,
            auto_close: false,
            smart_highlight: true,
            highlight_line: true,
            brace_match: true,
            auto_complete: true,
            auto_complete_from: 1,
            auto_complete_ignore_numbers: true,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub fg: Color32,
    pub line_bg: Color32,
    pub sel_bg: Color32,
    pub gutter_bg: Color32,
    pub gutter_fg: Color32,
    pub margin_bg: Color32,
    pub caret: Color32,
    pub symbol: Color32,
    pub guide: Color32,
    pub brace_bg: Color32,
    pub smart_bg: Color32,
    pub mark_bg: Color32,
    pub bookmark: Color32,
    pub fold: Color32,
    pub scroll_bg: Color32,
    pub scroll_fg: Color32,
    /// Bildlaufleisten mit klassischer 3D-Kante.
    pub scroll_bevel: bool,
}

impl Palette {
    pub fn light() -> Palette {
        Palette {
            bg: Color32::WHITE,
            fg: Color32::BLACK,
            line_bg: Color32::from_rgb(0xE8, 0xE8, 0xFF),
            sel_bg: Color32::from_rgb(0xC0, 0xC0, 0xC0),
            gutter_bg: Color32::from_rgb(0xE4, 0xE4, 0xE4),
            gutter_fg: Color32::from_rgb(0x80, 0x80, 0x80),
            margin_bg: Color32::from_rgb(0xEE, 0xEE, 0xEE),
            caret: Color32::BLACK,
            symbol: Color32::from_rgb(0xB0, 0xB0, 0xB0),
            guide: Color32::from_rgb(0xD0, 0xD0, 0xD0),
            brace_bg: Color32::from_rgb(0xFF, 0xC8, 0xC8),
            smart_bg: Color32::from_rgb(0xA8, 0xF0, 0xA8),
            mark_bg: Color32::from_rgb(0x80, 0xE0, 0xE0),
            bookmark: Color32::from_rgb(0x30, 0x60, 0xD0),
            fold: Color32::from_rgb(0x70, 0x70, 0x70),
            scroll_bg: Color32::from_rgb(0xF0, 0xF0, 0xF0),
            scroll_fg: Color32::from_rgb(0xC0, 0xC0, 0xC0),
            scroll_bevel: false,
        }
    }
    pub fn dark() -> Palette {
        Palette {
            bg: Color32::from_rgb(0x1F, 0x1F, 0x1F),
            fg: Color32::from_rgb(0xDC, 0xDC, 0xDC),
            line_bg: Color32::from_rgb(0x2C, 0x2C, 0x3C),
            sel_bg: Color32::from_rgb(0x4A, 0x50, 0x68),
            gutter_bg: Color32::from_rgb(0x2A, 0x2A, 0x2A),
            gutter_fg: Color32::from_rgb(0x88, 0x88, 0x88),
            margin_bg: Color32::from_rgb(0x26, 0x26, 0x26),
            caret: Color32::from_rgb(0xF0, 0xF0, 0xF0),
            symbol: Color32::from_rgb(0x60, 0x60, 0x60),
            guide: Color32::from_rgb(0x40, 0x40, 0x40),
            brace_bg: Color32::from_rgb(0x70, 0x38, 0x38),
            smart_bg: Color32::from_rgb(0x2E, 0x60, 0x38),
            mark_bg: Color32::from_rgb(0x20, 0x60, 0x68),
            bookmark: Color32::from_rgb(0x60, 0x90, 0xF0),
            fold: Color32::from_rgb(0xA0, 0xA0, 0xA0),
            scroll_bg: Color32::from_rgb(0x28, 0x28, 0x28),
            scroll_fg: Color32::from_rgb(0x55, 0x55, 0x55),
            scroll_bevel: false,
        }
    }
}

/// Zeichenbreiten und Zeilenhöhe der Schrift im Schreibfeld.
pub struct Metrics {
    pub font: FontId,
    pub bold: FontId,
    pub line_h: f32,
    pub ch: f32,
    widths: HashMap<char, f32>,
    ctx: Context,
}

pub const EDITOR_FAMILY: &str = "editor";
pub const EDITOR_BOLD_FAMILY: &str = "editor-bold";

impl Metrics {
    pub fn new(ctx: &Context, size: f32) -> Metrics {
        let font = FontId::new(size, FontFamily::Name(EDITOR_FAMILY.into()));
        let bold = FontId::new(size, FontFamily::Name(EDITOR_BOLD_FAMILY.into()));
        let (row, ch) = ctx.fonts(|f| (f.row_height(&font), f.glyph_width(&font, ' ')));
        Metrics {
            font,
            bold,
            line_h: (row * 1.06).ceil().max(row),
            ch,
            widths: HashMap::new(),
            ctx: ctx.clone(),
        }
    }

    pub fn w(&mut self, c: char) -> f32 {
        if let Some(w) = self.widths.get(&c) {
            return *w;
        }
        let font = self.font.clone();
        let w = self.ctx.fonts(|f| f.glyph_width(&font, c));
        self.widths.insert(c, w);
        w
    }

    pub fn tab_w(&self, o: &EdOpts) -> f32 {
        self.ch * o.tab_size.max(1) as f32
    }

    fn advance(&mut self, x: f32, c: char, tab_w: f32) -> f32 {
        if c == '\t' {
            ((x / tab_w).floor() + 1.0) * tab_w
        } else {
            x + self.w(c)
        }
    }
}

/// Stellen, an denen eine umbrochene Zeile in die nächste Reihe wechselt.
pub fn wrap_points(s: &str, width: f32, tab_w: f32, m: &mut Metrics) -> Vec<usize> {
    let mut out = Vec::new();
    if s.is_ascii() && !s.contains('\t') && s.len() as f32 * m.ch <= width {
        return out;
    }
    let chars: Vec<char> = s.chars().collect();
    let mut x = 0.0;
    let mut row_start = 0usize;
    let mut last_break = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        let nx = m.advance(x, c, tab_w);
        if nx > width && i > row_start {
            let at = if last_break > row_start { last_break } else { i };
            out.push(at);
            row_start = at;
            last_break = at;
            x = 0.0;
            for &k in &chars[at..i] {
                x = m.advance(x, k, tab_w);
            }
            continue;
        }
        x = nx;
        i += 1;
        if c == ' ' || c == '\t' {
            last_break = i;
        }
    }
    out
}

/// Ordnet Dokumentzeilen den sichtbaren Reihen zu (Umbruch und Faltung).
#[derive(Default)]
pub struct RowMap {
    wrap_rows: Vec<u32>,
    prefix: Vec<u64>,
    rev: u64,
    width: f32,
    fold_rev: u64,
    wrap: bool,
    identity: bool,
    built: bool,
    tab_w: f32,
    ch: f32,
}

impl RowMap {
    pub fn update(&mut self, doc: &Document, o: &EdOpts, width: f32, m: &mut Metrics) {
        let tab_w = m.tab_w(o);
        let wrap = o.wrap;
        let lines = doc.lines();
        let width_changed = (self.width - width).abs() > 0.5 || self.tab_w != tab_w || self.ch != m.ch;
        let same = self.built
            && self.rev == doc.rev
            && self.fold_rev == doc.fold_rev
            && self.wrap == wrap
            && (!wrap || !width_changed);
        if same {
            return;
        }
        if !wrap {
            self.wrap_rows.clear();
        } else if !self.built || !self.wrap || width_changed || self.wrap_rows.is_empty() || !self.replay(doc, width, tab_w, m) {
            self.wrap_rows = (0..lines)
                .map(|l| {
                    let s = doc.line_text(l);
                    wrap_points(&s, width, tab_w, m).len() as u32 + 1
                })
                .collect();
        }
        self.identity = !wrap && doc.folded.is_empty();
        self.prefix.clear();
        if !self.identity {
            self.prefix.reserve(lines + 1);
            let mut total = 0u64;
            let mut hide_until = 0usize;
            let has_folds = !doc.folded.is_empty();
            for l in 0..lines {
                self.prefix.push(total);
                let hidden = l < hide_until;
                if has_folds {
                    if let Some(e) = doc.folded.get(&l) {
                        hide_until = hide_until.max(*e + 1);
                    }
                }
                if !hidden {
                    total += if wrap { self.wrap_rows[l] as u64 } else { 1 };
                }
            }
            self.prefix.push(total);
        }
        self.rev = doc.rev;
        self.fold_rev = doc.fold_rev;
        self.wrap = wrap;
        self.width = width;
        self.tab_w = tab_w;
        self.ch = m.ch;
        self.built = true;
    }

    /// Änderungen aus dem Protokoll nachziehen statt alles neu zu messen.
    fn replay(&mut self, doc: &Document, width: f32, tab_w: f32, m: &mut Metrics) -> bool {
        if self.rev == doc.rev {
            return self.wrap_rows.len() == doc.lines();
        }
        let oldest = doc.log.front().map(|r| r.rev).unwrap_or(u64::MAX);
        if oldest > self.rev + 1 {
            return false;
        }
        for r in doc.log.iter().filter(|r| r.rev > self.rev) {
            let end = r.first_line + r.removed_lines + 1;
            if end > self.wrap_rows.len() {
                return false;
            }
            // Platzhalter; die wirkliche Messung folgt, wenn der Endstand feststeht.
            self.wrap_rows.splice(
                r.first_line..end,
                std::iter::repeat(u32::MAX).take(r.inserted_lines + 1),
            );
        }
        if self.wrap_rows.len() != doc.lines() {
            return false;
        }
        for l in 0..self.wrap_rows.len() {
            if self.wrap_rows[l] == u32::MAX {
                let s = doc.line_text(l);
                self.wrap_rows[l] = wrap_points(&s, width, tab_w, m).len() as u32 + 1;
            }
        }
        true
    }

    pub fn total_rows(&self, doc: &Document) -> u64 {
        if self.identity || !self.built {
            doc.lines() as u64
        } else {
            *self.prefix.last().unwrap_or(&1)
        }
    }

    pub fn line_to_row(&self, line: usize) -> u64 {
        if self.identity || !self.built {
            line as u64
        } else {
            self.prefix.get(line).copied().unwrap_or(0)
        }
    }

    /// Reihe -> (Zeile, Unterreihe).
    pub fn row_to_line(&self, row: u64, doc: &Document) -> (usize, usize) {
        if self.identity || !self.built {
            return ((row as usize).min(doc.lines().saturating_sub(1)), 0);
        }
        let n = self.prefix.len().saturating_sub(1);
        let idx = self
            .prefix
            .partition_point(|p| *p <= row)
            .saturating_sub(1)
            .min(n.saturating_sub(1));
        (idx, (row - self.prefix[idx]) as usize)
    }

    fn next_visible(&self, line: usize, doc: &Document) -> usize {
        if self.identity || !self.built {
            return line + 1;
        }
        let mut l = line + 1;
        let n = doc.lines();
        while l < n && self.prefix[l + 1] == self.prefix[l] {
            l += 1;
        }
        l
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxSel {
    pub anchor_line: usize,
    pub anchor_x: f32,
    pub head_line: usize,
    pub head_x: f32,
}

pub struct Complete {
    pub items: Vec<String>,
    pub selected: usize,
    pub start: usize,
}

pub struct View {
    pub doc: u64,
    pub sels: SelSet,
    pub top_row: f64,
    pub scroll_x: f32,
    pub goal_x: Option<f32>,
    pub map: RowMap,
    pub page_rows: usize,
    pub text_w: f32,
    pub box_sel: Option<BoxSel>,
    pub want_reveal: bool,
    pub seen_rev: u64,
    pub overwrite: bool,
    pub complete: Option<Complete>,
    pub want_focus: bool,
    pub max_x: f32,
    pub uid: u64,
    drag_anchor: Option<Sel>,
    gutter_drag: Option<usize>,
    scroll_drag: Option<f32>,
    blink_base: f64,
}

static VIEW_UID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl View {
    pub fn new(doc: u64, rev: u64) -> View {
        View {
            doc,
            sels: SelSet::single(0),
            top_row: 0.0,
            scroll_x: 0.0,
            goal_x: None,
            map: RowMap::default(),
            page_rows: 20,
            text_w: 600.0,
            box_sel: None,
            want_reveal: false,
            seen_rev: rev,
            overwrite: false,
            complete: None,
            want_focus: true,
            max_x: 0.0,
            uid: VIEW_UID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            drag_anchor: None,
            gutter_drag: None,
            scroll_drag: None,
            blink_base: 0.0,
        }
    }

    pub fn set_sels(&mut self, mut s: SelSet) {
        s.normalize();
        self.sels = s;
    }

    /// Auswahlen an Änderungen aus anderen Ansichten desselben Dokuments anpassen.
    pub fn sync(&mut self, doc: &Document) {
        if self.seen_rev != doc.rev {
            for r in doc.log.iter().filter(|r| r.rev > self.seen_rev) {
                let map = |p: usize| {
                    if p <= r.start {
                        p
                    } else if p >= r.start + r.removed {
                        p - r.removed + r.inserted
                    } else {
                        r.start + r.inserted
                    }
                };
                for s in self.sels.sels.iter_mut() {
                    s.anchor = map(s.anchor);
                    s.head = map(s.head);
                }
            }
            self.seen_rev = doc.rev;
        }
        let n = doc.len();
        for s in self.sels.sels.iter_mut() {
            s.anchor = s.anchor.min(n);
            s.head = s.head.min(n);
        }
    }
}

fn row_breaks(s: &str, o: &EdOpts, v: &View, m: &mut Metrics) -> Vec<usize> {
    if o.wrap {
        let tab_w = m.tab_w(o);
        wrap_points(s, v.text_w, tab_w, m)
    } else {
        Vec::new()
    }
}

fn measure(s: &str, from: usize, to: usize, tab_w: f32, m: &mut Metrics) -> f32 {
    let mut x = 0.0;
    for c in s.chars().skip(from).take(to.saturating_sub(from)) {
        x = m.advance(x, c, tab_w);
    }
    x
}

pub fn pos_row_x(doc: &Document, v: &View, o: &EdOpts, m: &mut Metrics, pos: usize) -> (u64, f32) {
    let line = doc.text.char_to_line(pos);
    let col = pos - doc.line_start(line);
    let s = doc.line_text(line);
    let breaks = row_breaks(&s, o, v, m);
    let sub = breaks.iter().take_while(|b| **b <= col).count();
    let start = if sub == 0 { 0 } else { breaks[sub - 1] };
    let tab_w = m.tab_w(o);
    (v.map.line_to_row(line) + sub as u64, measure(&s, start, col, tab_w, m))
}

/// Anfang und Ende der sichtbaren Reihe, in der diese Position liegt.
pub fn row_bounds(doc: &Document, v: &View, o: &EdOpts, m: &mut Metrics, pos: usize) -> (usize, usize) {
    let line = doc.text.char_to_line(pos);
    let ls = doc.line_start(line);
    let col = pos - ls;
    let s = doc.line_text(line);
    let breaks = row_breaks(&s, o, v, m);
    let sub = breaks.iter().take_while(|b| **b <= col).count();
    let start = if sub == 0 { 0 } else { breaks[sub - 1] };
    let end = if sub < breaks.len() {
        breaks[sub].saturating_sub(1).max(start)
    } else {
        s.chars().count()
    };
    (ls + start, ls + end)
}

fn x_to_col(s: &str, from: usize, to: usize, x: f32, tab_w: f32, m: &mut Metrics) -> usize {
    let mut cur = 0.0;
    for (i, c) in s.chars().skip(from).take(to - from).enumerate() {
        let nx = m.advance(cur, c, tab_w);
        if x < (cur + nx) / 2.0 {
            return from + i;
        }
        cur = nx;
    }
    to
}

pub fn row_x_pos(doc: &Document, v: &View, o: &EdOpts, m: &mut Metrics, row: u64, x: f32) -> usize {
    let (line, sub) = v.map.row_to_line(row, doc);
    let s = doc.line_text(line);
    let n = s.chars().count();
    let breaks = row_breaks(&s, o, v, m);
    let sub = sub.min(breaks.len());
    let start = if sub == 0 { 0 } else { breaks[sub - 1] };
    let end = if sub < breaks.len() {
        breaks[sub].saturating_sub(1).max(start)
    } else {
        n
    };
    let tab_w = m.tab_w(o);
    doc.line_start(line) + x_to_col(&s, start, end, x, tab_w, m)
}

/// Position in einer Zeile zu einer x-Koordinate (Spaltenauswahl, ohne Umbruch gerechnet).
pub fn line_x_pos(doc: &Document, o: &EdOpts, m: &mut Metrics, line: usize, x: f32) -> usize {
    let s = doc.line_text(line);
    let n = s.chars().count();
    let tab_w = m.tab_w(o);
    doc.line_start(line) + x_to_col(&s, 0, n, x, tab_w, m)
}

pub fn line_pos_x(doc: &Document, o: &EdOpts, m: &mut Metrics, pos: usize) -> f32 {
    let line = doc.text.char_to_line(pos);
    let s = doc.line_text(line);
    let tab_w = m.tab_w(o);
    measure(&s, 0, pos - doc.line_start(line), tab_w, m)
}

/// Passende Klammer zu der am Cursor suchen (begrenzt, damit es flott bleibt).
pub fn matching_brace(doc: &Document, pos: usize) -> Option<(usize, usize)> {
    let t = &doc.text;
    let pick = |p: usize| -> Option<(usize, char)> {
        let c = t.get_char(p)?;
        matches!(c, '(' | ')' | '[' | ']' | '{' | '}').then_some((p, c))
    };
    let (p, c) = (if pos > 0 { pick(pos - 1) } else { None }).or_else(|| pick(pos))?;
    let (other, forward) = match c {
        '(' => (')', true),
        '[' => (']', true),
        '{' => ('}', true),
        ')' => ('(', false),
        ']' => ('[', false),
        _ => ('{', false),
    };
    let mut depth = 0i32;
    const LIMIT: usize = 200_000;
    if forward {
        for (i, ch) in t.chars_at(p).enumerate().take(LIMIT) {
            if ch == c {
                depth += 1;
            } else if ch == other {
                depth -= 1;
                if depth == 0 {
                    return Some((p, p + i));
                }
            }
        }
    } else {
        let mut i = p + 1;
        let mut steps = 0;
        while i > 0 && steps < LIMIT {
            i -= 1;
            steps += 1;
            let ch = t.char(i);
            if ch == c {
                depth += 1;
            } else if ch == other {
                depth -= 1;
                if depth == 0 {
                    return Some((i, p));
                }
            }
        }
    }
    None
}

#[derive(Default)]
pub struct EditorOut {
    pub focused: bool,
    pub clipboard: Option<String>,
    pub zoom: f32,
    pub context_menu: bool,
    pub scrolled: Option<(f64, f32)>,
}

struct VisRow {
    line: usize,
    start: usize,
    end: usize,
    y: f32,
    sub: usize,
}

const SCROLL_W: f32 = 16.0;
/// Takt des blinkenden Cursors (Windows-Standard 530 ms) und seine Breite.
const CARET_BLINK_SECS: f64 = 0.53;
const CARET_WIDTH: f32 = 1.0;

pub fn show(ui: &mut Ui, doc: &mut Document, v: &mut View, o: &EdOpts, m: &mut Metrics, pal: &Palette) -> (EditorOut, egui::Response) {
    let mut out = EditorOut { zoom: 1.0, ..Default::default() };
    let rect = ui.max_rect();
    let ctx = ui.ctx().clone();
    let base_id = egui::Id::new(("editor", v.uid));
    v.sync(doc);

    // Ränder
    let digits = (doc.lines() as f64).log10().floor() as usize + 1;
    let num_w = if o.line_numbers { digits.max(3) as f32 * m.ch + 10.0 } else { 0.0 };
    let margin_w = 30.0_f32;
    let bookmark_w = 16.0_f32;
    let gutter_w = num_w + margin_w;

    let total_before = v.map.total_rows(doc);
    let visible_rows = |h: f32, m: &Metrics| (h / m.line_h).floor().max(1.0) as u64;
    let need_v = total_before > visible_rows(rect.height(), m);
    let need_h = !o.wrap && v.max_x + 20.0 > rect.width() - gutter_w - if need_v { SCROLL_W } else { 0.0 };
    let bar_w = if need_v { SCROLL_W } else { 0.0 };
    let bar_h = if need_h { SCROLL_W } else { 0.0 };
    let text_rect = Rect::from_min_max(
        pos2(rect.left() + gutter_w, rect.top()),
        pos2(rect.right() - bar_w, rect.bottom() - bar_h),
    );
    let gutter_rect = Rect::from_min_max(rect.min, pos2(rect.left() + gutter_w, rect.bottom() - bar_h));
    let text_pad = 2.0;
    v.text_w = (text_rect.width() - text_pad * 2.0).max(m.ch * 8.0);
    v.page_rows = visible_rows(text_rect.height(), m) as usize;
    v.map.update(doc, o, v.text_w, m);

    let resp = ui.interact(text_rect, base_id, Sense::click_and_drag());
    let gresp = ui.interact(gutter_rect, base_id.with("g"), Sense::click_and_drag());
    if resp.hovered() {
        ctx.set_cursor_icon(egui::CursorIcon::Text);
    }
    if v.want_focus {
        v.want_focus = false;
        resp.request_focus();
    }
    let any_press = resp.is_pointer_button_down_on() || gresp.is_pointer_button_down_on();
    if any_press || resp.secondary_clicked() {
        resp.request_focus();
    }
    let has_focus = resp.has_focus();
    out.focused = has_focus;
    let now = ui.input(|i| i.time);

    // Mausrad
    if ui.rect_contains_pointer(rect) {
        let (delta, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
        if delta.y != 0.0 {
            v.top_row -= (delta.y / m.line_h) as f64;
        }
        if delta.x != 0.0 && !o.wrap {
            v.scroll_x -= delta.x;
        }
        if zoom != 1.0 {
            out.zoom = zoom;
        }
    }

    let tab_w = m.tab_w(o);
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let text_pos = |p: Pos2, v: &View, line_h: f32| -> (u64, f32) {
        let y = ((p.y - text_rect.top()) / line_h) as f64 + v.top_row;
        (y.max(0.0).floor() as u64, p.x - text_rect.left() - text_pad + v.scroll_x)
    };

    // Maus im Text
    let (shift, ctrl, alt) = ui.input(|i| (i.modifiers.shift, i.modifiers.ctrl, i.modifiers.alt));
    let (pressed, primary_down) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down()));
    if let Some(p) = pointer {
        if pressed && resp.is_pointer_button_down_on() {
            let (row, x) = text_pos(p, v, m.line_h);
            let total = v.map.total_rows(doc);
            let pos = row_x_pos(doc, v, o, m, row.min(total.saturating_sub(1)), x);
            v.goal_x = None;
            v.complete = None;
            v.blink_base = now;
            if alt {
                let line = doc.text.char_to_line(pos);
                let b = BoxSel { anchor_line: line, anchor_x: x.max(0.0), head_line: line, head_x: x.max(0.0) };
                Ed { doc, v, o, m }.apply_box(b);
                v.drag_anchor = None;
            } else if shift {
                let main = v.sels.main();
                v.box_sel = None;
                v.sels = SelSet::range(main.anchor, pos);
                v.drag_anchor = Some(Sel::caret(main.anchor));
            } else if ctrl {
                v.box_sel = None;
                v.sels.sels.push(Sel::caret(pos));
                v.sels.primary = v.sels.sels.len() - 1;
                v.sels.normalize();
                v.drag_anchor = None;
            } else {
                v.box_sel = None;
                v.sels = SelSet::single(pos);
                v.drag_anchor = Some(Sel::caret(pos));
            }
        } else if primary_down && resp.is_pointer_button_down_on() {
            // Beim Ziehen über den Rand hinaus mitscrollen.
            if p.y < text_rect.top() {
                v.top_row -= 0.4;
                ctx.request_repaint();
            } else if p.y > text_rect.bottom() {
                v.top_row += 0.4;
                ctx.request_repaint();
            }
            let (row, x) = text_pos(p, v, m.line_h);
            let total = v.map.total_rows(doc);
            let pos = row_x_pos(doc, v, o, m, row.min(total.saturating_sub(1)), x);
            if let (Some(mut b), None) = (v.box_sel, v.drag_anchor) {
                let line = doc.text.char_to_line(pos);
                if b.head_line != line || (b.head_x - x).abs() > 0.5 {
                    b.head_line = line;
                    b.head_x = x.max(0.0);
                    Ed { doc, v, o, m }.apply_box(b);
                }
            } else if let Some(a) = v.drag_anchor {
                v.sels = SelSet::range(a.anchor, pos);
                reveal_horizontal(doc, v, o, m, pos);
            }
        }
    }
    if resp.double_clicked() && !alt {
        let pos = v.sels.main().head;
        let (a, b) = Ed { doc, v, o, m }.word_at(pos);
        v.sels = SelSet::range(a, b);
        v.drag_anchor = None;
    }
    if resp.triple_clicked() && !alt {
        let line = doc.text.char_to_line(v.sels.main().head);
        let a = doc.line_start(line);
        let b = if line + 1 < doc.lines() { doc.line_start(line + 1) } else { doc.line_end(line) };
        v.sels = SelSet::range(a, b);
        v.drag_anchor = None;
    }
    if resp.secondary_clicked() {
        if let Some(p) = pointer {
            let (row, x) = text_pos(p, v, m.line_h);
            let total = v.map.total_rows(doc);
            let pos = row_x_pos(doc, v, o, m, row.min(total.saturating_sub(1)), x);
            let inside = v.sels.sels.iter().any(|s| s.start() <= pos && pos <= s.end() && !s.is_empty());
            if !inside {
                v.sels = SelSet::single(pos);
            }
            out.context_menu = true;
        }
    }

    // Maus im Rand
    if let Some(p) = pointer {
        if pressed && gresp.is_pointer_button_down_on() {
            let row = (((p.y - gutter_rect.top()) / m.line_h) as f64 + v.top_row).max(0.0).floor() as u64;
            let total = v.map.total_rows(doc);
            if row < total {
                let (line, _) = v.map.row_to_line(row, doc);
                let gx = p.x - gutter_rect.left();
                if gx < num_w {
                    v.box_sel = None;
                    let a = doc.line_start(line);
                    let b = if line + 1 < doc.lines() { doc.line_start(line + 1) } else { doc.line_end(line) };
                    v.sels = SelSet::range(a, b);
                    v.gutter_drag = Some(line);
                } else if gx < num_w + bookmark_w {
                    if !doc.bookmarks.remove(&line) {
                        doc.bookmarks.insert(line);
                    }
                } else {
                    doc.toggle_fold(line);
                    v.map.update(doc, o, v.text_w, m);
                }
            }
        } else if gresp.dragged_by(egui::PointerButton::Primary) {
            if let Some(a) = v.gutter_drag {
                let row = (((p.y - gutter_rect.top()) / m.line_h) as f64 + v.top_row).max(0.0).floor() as u64;
                let total = v.map.total_rows(doc);
                let (line, _) = v.map.row_to_line(row.min(total.saturating_sub(1)), doc);
                let (l0, l1) = (a.min(line), a.max(line));
                let s = doc.line_start(l0);
                let e = if l1 + 1 < doc.lines() { doc.line_start(l1 + 1) } else { doc.line_end(l1) };
                v.sels = if line < a { SelSet::range(e, s) } else { SelSet::range(s, e) };
            }
        }
    }
    if !gresp.is_pointer_button_down_on() {
        v.gutter_drag = None;
    }

    // Tastatur
    if has_focus {
        ctx.memory_mut(|mem| {
            mem.set_focus_lock_filter(
                base_id,
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            )
        });
        let events = ui.input(|i| i.events.clone());
        for ev in events {
            let cmd = match ev {
                Event::Text(t) => t.chars().all(|c| !c.is_control()).then_some(Cmd::Insert(t)),
                Event::Ime(egui::ImeEvent::Commit(t)) if !t.is_empty() => Some(Cmd::Insert(t)),
                Event::Copy => Some(Cmd::Copy),
                Event::Cut => Some(Cmd::Cut),
                Event::Paste(t) => Some(Cmd::Paste(t)),
                Event::Key { key, pressed: true, modifiers, .. } => {
                    if let Some(c) = v.complete.as_mut() {
                        match key {
                            Key::ArrowDown => {
                                c.selected = (c.selected + 1).min(c.items.len().saturating_sub(1));
                                continue;
                            }
                            Key::ArrowUp => {
                                c.selected = c.selected.saturating_sub(1);
                                continue;
                            }
                            Key::Escape => {
                                v.complete = None;
                                continue;
                            }
                            Key::Enter | Key::Tab => {
                                if let Some(item) = c.items.get(c.selected).cloned() {
                                    let start = c.start;
                                    accept_completion(doc, v, o, m, start, &item);
                                }
                                v.complete = None;
                                continue;
                            }
                            _ => {}
                        }
                    }
                    key_to_cmd(key, modifiers, v)
                }
                _ => None,
            };
            if let Some(cmd) = cmd {
                v.blink_base = now;
                let is_typing = matches!(cmd, Cmd::Insert(_) | Cmd::Backspace);
                let typed_word = matches!(&cmd, Cmd::Insert(t)
                    if !t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || c == '_'));
                if !is_typing {
                    v.complete = None;
                }
                let rev_before = doc.rev;
                let clip = Ed { doc, v, o, m }.apply(cmd);
                if clip.is_some() {
                    out.clipboard = clip;
                }
                if is_typing && v.complete.is_some() {
                    refresh_completion(doc, v);
                } else if typed_word && doc.rev != rev_before {
                    auto_completion(doc, v, o);
                }
            }
        }
    }

    // Cursor sichtbar halten
    if v.want_reveal {
        v.want_reveal = false;
        for s in v.sels.sels.clone() {
            let line = doc.text.char_to_line(s.head);
            doc.unfold_containing(line);
        }
        v.map.update(doc, o, v.text_w, m);
        let (row, _) = pos_row_x(doc, v, o, m, v.sels.main().head);
        let rows = (text_rect.height() / m.line_h).floor().max(1.0) as f64;
        if (row as f64) < v.top_row {
            v.top_row = row as f64;
        } else if row as f64 + 1.0 > v.top_row + rows {
            v.top_row = row as f64 + 1.0 - rows;
        }
        reveal_horizontal(doc, v, o, m, v.sels.main().head);
    }

    let total_rows = v.map.total_rows(doc);
    let rows_fit = (text_rect.height() / m.line_h).floor().max(1.0) as f64;
    let max_top = (total_rows as f64 - rows_fit).max(0.0);
    v.top_row = v.top_row.clamp(0.0, max_top);
    if o.wrap {
        v.scroll_x = 0.0;
    }
    v.scroll_x = v.scroll_x.max(0.0);

    // Sichtbare Reihen ermitteln. Pixelgenau gesetzt, damit die Schrift scharf bleibt.
    let first_row = v.top_row.floor() as u64;
    let frac = (v.top_row - first_row as f64) as f32;
    let y0 = (text_rect.top() - frac * m.line_h).round();
    let mut rows: Vec<VisRow> = Vec::new();
    let mut texts: HashMap<usize, (String, Vec<usize>)> = HashMap::new();
    let (mut line, mut sub) = v.map.row_to_line(first_row, doc);
    let mut y = y0;
    let lines = doc.lines();
    while y < text_rect.bottom() && line < lines {
        let entry = texts.entry(line).or_insert_with(|| (doc.line_text(line), Vec::new()));
        if o.wrap && entry.1.is_empty() {
            entry.1 = wrap_points(&entry.0, v.text_w, tab_w, m);
        }
        let n = entry.0.chars().count();
        let breaks = &entry.1;
        let start = if sub == 0 { 0 } else { breaks.get(sub - 1).copied().unwrap_or(n) };
        let end = breaks.get(sub).copied().unwrap_or(n);
        rows.push(VisRow { line, start, end, y, sub });
        y += m.line_h;
        if sub < breaks.len() {
            sub += 1;
        } else {
            line = v.map.next_visible(line, doc);
            sub = 0;
        }
    }

    // Farben in kleinen Portionen berechnen
    if let (Some(a), Some(b)) = (rows.first().map(|r| r.line), rows.last().map(|r| r.line)) {
        if !doc.hl.ensure(&doc.text, a, b + 1, Duration::from_millis(12)) {
            ctx.request_repaint();
        }
    }

    // Zeichnen
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, pal.bg);
    let clip_text = painter.with_clip_rect(text_rect);
    let origin_x = text_rect.left() + text_pad - v.scroll_x;
    let main = v.sels.main();
    let main_line = doc.text.char_to_line(main.head);
    let any_selection = v.sels.sels.iter().any(|s| !s.is_empty());

    // Wort unter der Auswahl hervorheben
    let smart_word: Option<String> = if o.smart_highlight && !doc.large && !main.is_empty() && v.sels.sels.len() == 1 {
        if main.end() - main.start() < 200 {
            let t = doc.slice(main.start(), main.end());
            (!t.contains('\n') && !t.trim().is_empty()).then_some(t)
        } else {
            None
        }
    } else {
        None
    };

    let brace = if o.brace_match && !doc.large && !any_selection { matching_brace(doc, main.head) } else { None };

    for r in &rows {
        let (s, _) = &texts[&r.line];
        let line_start = doc.line_start(r.line);
        let row_rect = Rect::from_min_size(pos2(text_rect.left(), r.y), vec2(text_rect.width(), m.line_h));

        if o.highlight_line && r.line == main_line && !any_selection {
            clip_text.rect_filled(row_rect, 0.0, pal.line_bg);
        }

        // Spaltenpositionen dieser Reihe
        let mut xs: Vec<f32> = Vec::with_capacity(r.end - r.start + 1);
        let mut x = 0.0_f32;
        let row_chars: Vec<char> = s.chars().skip(r.start).take(r.end - r.start).collect();
        for c in &row_chars {
            xs.push(x);
            x = m.advance(x, *c, tab_w);
        }
        xs.push(x);
        v.max_x = v.max_x.max(x);
        let col_x = |col: usize| xs[col.saturating_sub(r.start).min(xs.len() - 1)];
        let abs_start = line_start + r.start;
        let abs_end = line_start + r.end;

        let paint_range = |a: usize, b: usize, color: Color32, painter: &egui::Painter| {
            if b <= abs_start || a >= abs_end {
                return;
            }
            let x0 = col_x(a.max(abs_start) - line_start);
            let x1 = col_x(b.min(abs_end) - line_start);
            painter.rect_filled(
                Rect::from_min_max(pos2(origin_x + x0, r.y), pos2(origin_x + x1, r.y + m.line_h)),
                0.0,
                color,
            );
        };

        // Dauerhafte Markierungen
        if !doc.marks.is_empty() {
            let idx = doc.marks.partition_point(|mk| mk.1 <= abs_start);
            for mk in doc.marks[idx..].iter().take_while(|mk| mk.0 < abs_end) {
                paint_range(mk.0, mk.1, pal.mark_bg, &clip_text);
            }
        }
        // Gleiche Wörter
        if let Some(word) = &smart_word {
            // Ohne Beachtung der Groß-/Kleinschreibung, nur ganze Wörter.
            let needle: Vec<char> = word.chars().collect();
            let wlen = needle.len();
            let same = |a: char, b: char| a == b || a.to_lowercase().eq(b.to_lowercase());
            for ci in 0..row_chars.len().saturating_sub(wlen.saturating_sub(1)) {
                if !row_chars[ci..ci + wlen].iter().zip(&needle).all(|(a, b)| same(*a, *b)) {
                    continue;
                }
                let a = abs_start + ci;
                if a == main.start() {
                    continue;
                }
                let before = ci.checked_sub(1).and_then(|k| row_chars.get(k));
                let after = row_chars.get(ci + wlen);
                let is_word = |c: &char| c.is_alphanumeric() || *c == '_';
                if before.is_none_or(|c| !is_word(c)) && after.is_none_or(|c| !is_word(c)) {
                    paint_range(a, a + wlen, pal.smart_bg, &clip_text);
                }
            }
        }
        // Auswahl
        for sel in &v.sels.sels {
            if sel.is_empty() {
                continue;
            }
            paint_range(sel.start(), sel.end(), pal.sel_bg, &clip_text);
            // Zeilenumbruch mit in die Auswahl einzeichnen
            if sel.start() <= abs_end && abs_end < sel.end() && r.end == s.chars().count() {
                let ex = col_x(r.end);
                clip_text.rect_filled(
                    Rect::from_min_size(pos2(origin_x + ex, r.y), vec2(m.ch * 0.8, m.line_h)),
                    0.0,
                    pal.sel_bg,
                );
            }
        }
        // Klammerpaar
        if let Some((a, b)) = brace {
            paint_range(a, a + 1, pal.brace_bg, &clip_text);
            paint_range(b, b + 1, pal.brace_bg, &clip_text);
        }

        // Einzugslinien
        if o.indent_guides && r.sub == 0 {
            let lead = row_chars.iter().take_while(|c| **c == ' ' || **c == '\t').count();
            let lead_x = xs[lead];
            let mut gx = tab_w;
            while gx < lead_x {
                let px = (origin_x + gx).round() + 0.5;
                clip_text.line_segment(
                    [pos2(px, r.y), pos2(px, r.y + m.line_h)],
                    Stroke::new(1.0, pal.guide),
                );
                gx += tab_w;
            }
        }

        // Text, in gleichfarbigen Stücken
        let spans = doc.hl.spans(r.line);
        let spans_ref = spans.as_deref();
        let mut span_i = 0usize;
        let mut span_left = spans_ref.and_then(|sp| sp.first()).map(|sp| sp.chars).unwrap_or(u32::MAX);
        // Vorspulen bis zum Reihenanfang
        if let Some(sp) = spans_ref {
            let mut to_skip = r.start as u32;
            while to_skip > 0 && span_i < sp.len() {
                if to_skip >= span_left {
                    to_skip -= span_left;
                    span_i += 1;
                    span_left = sp.get(span_i).map(|x| x.chars).unwrap_or(u32::MAX);
                } else {
                    span_left -= to_skip;
                    to_skip = 0;
                }
            }
        }
        let mut run = String::new();
        let mut run_x = 0.0_f32;
        let mut run_color = pal.fg;
        let mut run_bold = false;
        let left_limit = v.scroll_x - m.ch * 4.0;
        let right_limit = v.scroll_x + text_rect.width() + m.ch * 2.0;
        let flush = |run: &mut String, x: f32, color: Color32, bold: bool| {
            if run.is_empty() {
                return;
            }
            let font = if bold { m.bold.clone() } else { m.font.clone() };
            let galley = ctx.fonts(|f| f.layout_no_wrap(std::mem::take(run), font, color));
            let pos = pos2(origin_x + x, r.y + (m.line_h - galley.size().y) * 0.5).round();
            clip_text.galley(pos, galley, color);
        };
        for (i, c) in row_chars.iter().enumerate() {
            let cx = xs[i];
            let (color, bold) = match spans_ref.and_then(|sp| sp.get(span_i)) {
                Some(sp) => (sp.color, sp.bold),
                None => (pal.fg, false),
            };
            if spans_ref.is_some() {
                span_left = span_left.saturating_sub(1);
                if span_left == 0 {
                    span_i += 1;
                    span_left = spans_ref.and_then(|sp| sp.get(span_i)).map(|x| x.chars).unwrap_or(u32::MAX);
                }
            }
            if cx > right_limit {
                break;
            }
            if xs[i + 1] < left_limit {
                continue;
            }
            if *c == '\t' {
                flush(&mut run, run_x, run_color, run_bold);
                if o.show_ws {
                    let mid = r.y + m.line_h * 0.5;
                    let (x0, x1) = (origin_x + cx + 2.0, origin_x + xs[i + 1] - 2.0);
                    let st = Stroke::new(1.0, pal.symbol);
                    clip_text.line_segment([pos2(x0, mid), pos2(x1, mid)], st);
                    clip_text.line_segment([pos2(x1 - 3.0, mid - 3.0), pos2(x1, mid)], st);
                    clip_text.line_segment([pos2(x1 - 3.0, mid + 3.0), pos2(x1, mid)], st);
                }
                continue;
            }
            if *c == ' ' && o.show_ws {
                flush(&mut run, run_x, run_color, run_bold);
                clip_text.circle_filled(pos2(origin_x + cx + m.ch * 0.5, r.y + m.line_h * 0.5), 1.2, pal.symbol);
                continue;
            }
            if run.is_empty() {
                run_x = cx;
                run_color = color;
                run_bold = bold;
            } else if color != run_color || bold != run_bold {
                flush(&mut run, run_x, run_color, run_bold);
                run_x = cx;
                run_color = color;
                run_bold = bold;
            }
            run.push(*c);
        }
        flush(&mut run, run_x, run_color, run_bold);

        // Zeilenende-Zeichen
        if o.show_eol && r.line + 1 < lines && r.end == s.chars().count() {
            let label = match doc.eol {
                crate::fileio::Eol::Crlf => "CRLF",
                crate::fileio::Eol::Lf => "LF",
                crate::fileio::Eol::Cr => "CR",
            };
            let ex = origin_x + col_x(r.end) + 3.0;
            let font = FontId::proportional((m.font.size * 0.6).max(8.0));
            let g = ctx.fonts(|f| f.layout_no_wrap(label.into(), font, Color32::WHITE));
            let bx = Rect::from_min_size(
                pos2(ex, r.y + (m.line_h - g.size().y) * 0.5 - 1.0),
                g.size() + vec2(4.0, 2.0),
            );
            clip_text.rect_filled(bx, 2.0, pal.symbol);
            clip_text.galley(bx.min + vec2(2.0, 1.0), g, Color32::WHITE);
        }
        // Gefaltete Zeile: Linie darunter
        if r.sub == 0 && doc.folded.contains_key(&r.line) {
            let by = r.y + m.line_h - 1.0;
            clip_text.line_segment(
                [pos2(text_rect.left(), by), pos2(text_rect.right(), by)],
                Stroke::new(1.0, pal.fold),
            );
        }
    }

    // Cursor
    let blink_on = ((now - v.blink_base) % (2.0 * CARET_BLINK_SECS)) < CARET_BLINK_SECS;
    let mut caret_rect = None;
    for sel in &v.sels.sels {
        let (row, x) = pos_row_x(doc, v, o, m, sel.head);
        let y = y0 + ((row as f64 - first_row as f64) as f32) * m.line_h;
        let px = origin_x + x;
        if sel.head == main.head {
            caret_rect = Some(Rect::from_min_size(pos2(px, y), vec2(1.0, m.line_h)));
        }
        if has_focus && blink_on && y + m.line_h > text_rect.top() && y < text_rect.bottom() {
            let r = if v.overwrite {
                Rect::from_min_size(pos2(px, y + m.line_h - 2.0), vec2(m.ch, 2.0))
            } else {
                Rect::from_min_size(pos2(px.round(), y), vec2(CARET_WIDTH, m.line_h))
            };
            clip_text.rect_filled(r, 0.0, pal.caret);
        }
    }
    if has_focus {
        let wait = CARET_BLINK_SECS - ((now - v.blink_base) % CARET_BLINK_SECS);
        ctx.request_repaint_after(Duration::from_secs_f64(wait.max(0.02)));
        if let Some(c) = caret_rect {
            ctx.output_mut(|o| {
                o.ime = Some(egui::output::IMEOutput { rect: text_rect, cursor_rect: c });
            });
        }
    }

    // Ränder zeichnen
    let gp = ui.painter_at(gutter_rect);
    if num_w > 0.0 {
        gp.rect_filled(
            Rect::from_min_max(gutter_rect.min, pos2(gutter_rect.left() + num_w, gutter_rect.bottom())),
            0.0,
            pal.gutter_bg,
        );
    }
    let margin_rect = Rect::from_min_max(pos2(gutter_rect.left() + num_w, gutter_rect.top()), gutter_rect.max);
    gp.rect_filled(margin_rect, 0.0, pal.margin_bg);
    let gut_font = FontId::new((m.font.size * 0.86).max(8.0), m.font.family.clone());
    for r in &rows {
        if r.sub != 0 {
            continue;
        }
        if num_w > 0.0 {
            gp.text(
                pos2(gutter_rect.left() + num_w - 8.0, r.y + m.line_h * 0.5),
                Align2::RIGHT_CENTER,
                (r.line + 1).to_string(),
                gut_font.clone(),
                pal.gutter_fg,
            );
        }
        if doc.bookmarks.contains(&r.line) {
            gp.circle_filled(
                pos2(margin_rect.left() + bookmark_w * 0.5, r.y + m.line_h * 0.5),
                (m.line_h * 0.32).min(6.0),
                pal.bookmark,
            );
        }
        let is_folded = doc.folded.contains_key(&r.line);
        if is_folded || doc.fold_end(r.line, false).is_some() {
            let c = pos2(margin_rect.left() + bookmark_w + (margin_w - bookmark_w) * 0.5, r.y + m.line_h * 0.5);
            let box_r = Rect::from_center_size(c, vec2(9.0, 9.0));
            gp.rect_filled(box_r, 0.0, pal.bg);
            gp.rect_stroke(box_r, 0.0, Stroke::new(1.0, pal.fold), StrokeKind::Inside);
            gp.line_segment([c - vec2(2.0, 0.0), c + vec2(2.5, 0.0)], Stroke::new(1.0, pal.fold));
            if is_folded {
                gp.line_segment([c - vec2(0.0, 2.0), c + vec2(0.0, 2.5)], Stroke::new(1.0, pal.fold));
            }
        }
    }

    // Autovervollständigung
    if let (Some(c), Some(cr)) = (v.complete.as_ref(), caret_rect) {
        let mut chosen = None;
        egui::Area::new(base_id.with("complete"))
            .order(egui::Order::Foreground)
            .fixed_pos(cr.left_bottom())
            .show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).inner_margin(2.0).show(ui, |ui| {
                    ui.set_max_height(200.0);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (i, item) in c.items.iter().enumerate() {
                            let r = ui.selectable_label(i == c.selected, item);
                            if i == c.selected {
                                r.scroll_to_me(None);
                            }
                            if r.clicked() {
                                chosen = Some(item.clone());
                            }
                        }
                    });
                });
            });
        if let Some(item) = chosen {
            let start = c.start;
            accept_completion(doc, v, o, m, start, &item);
            v.complete = None;
            v.want_focus = true;
        }
    }

    // Bildlaufleisten
    let before = (v.top_row, v.scroll_x);
    if need_v {
        let track = Rect::from_min_max(
            pos2(rect.right() - SCROLL_W, rect.top()),
            pos2(rect.right(), rect.bottom() - bar_h),
        );
        let sresp = ui.interact(track, base_id.with("vs"), Sense::click_and_drag());
        let p = ui.painter_at(track);
        p.rect_filled(track, 0.0, pal.scroll_bg);
        let frac_vis = (rows_fit / total_rows.max(1) as f64).min(1.0) as f32;
        let thumb_h = (track.height() * frac_vis).max(24.0);
        let range = (track.height() - thumb_h).max(1.0);
        let pos_frac = if max_top > 0.0 { (v.top_row / max_top) as f32 } else { 0.0 };
        let thumb = Rect::from_min_size(
            pos2(track.left() + 2.0, track.top() + pos_frac * range),
            vec2(SCROLL_W - 4.0, thumb_h),
        );
        paint_thumb(&p, thumb, pal);
        if let Some(pp) = sresp.interact_pointer_pos() {
            if ui.input(|i| i.pointer.primary_pressed()) || v.scroll_drag.is_none() {
                v.scroll_drag = Some(if thumb.contains(pp) { pp.y - thumb.top() } else { thumb_h * 0.5 });
            }
            if let Some(off) = v.scroll_drag {
                let f = ((pp.y - off - track.top()) / range).clamp(0.0, 1.0);
                v.top_row = (f as f64 * max_top).round();
                ctx.request_repaint();
            }
        }
        if sresp.drag_stopped() || sresp.clicked() {
            v.scroll_drag = None;
        }
    }
    if need_h {
        let track = Rect::from_min_max(
            pos2(rect.left() + gutter_w, rect.bottom() - SCROLL_W),
            pos2(rect.right() - bar_w, rect.bottom()),
        );
        let sresp = ui.interact(track, base_id.with("hs"), Sense::click_and_drag());
        let p = ui.painter_at(track);
        p.rect_filled(track, 0.0, pal.scroll_bg);
        let content = (v.max_x + 40.0).max(track.width());
        let thumb_w = (track.width() * track.width() / content).max(24.0);
        let range = (track.width() - thumb_w).max(1.0);
        let max_x = content - track.width();
        let thumb = Rect::from_min_size(
            pos2(track.left() + (v.scroll_x / max_x.max(1.0)).min(1.0) * range, track.top() + 2.0),
            vec2(thumb_w, SCROLL_W - 4.0),
        );
        paint_thumb(&p, thumb, pal);
        if let Some(pp) = sresp.interact_pointer_pos() {
            let f = ((pp.x - thumb_w * 0.5 - track.left()) / range).clamp(0.0, 1.0);
            v.scroll_x = f * max_x;
            ctx.request_repaint();
        }
    } else {
        v.scroll_x = 0.0;
    }
    if before != (v.top_row, v.scroll_x) {
        out.scrolled = Some((v.top_row, v.scroll_x));
    }
    (out, resp)
}

fn reveal_horizontal(doc: &Document, v: &mut View, o: &EdOpts, m: &mut Metrics, pos: usize) {
    if o.wrap {
        v.scroll_x = 0.0;
        return;
    }
    let x = line_pos_x(doc, o, m, pos);
    v.max_x = v.max_x.max(x);
    let w = v.text_w;
    if x < v.scroll_x {
        v.scroll_x = (x - m.ch * 8.0).max(0.0);
    } else if x + m.ch * 2.0 > v.scroll_x + w {
        v.scroll_x = x + m.ch * 8.0 - w;
    }
}

fn key_to_cmd(key: Key, md: egui::Modifiers, v: &View) -> Option<Cmd> {
    let (shift, ctrl, alt) = (md.shift, md.ctrl || md.command, md.alt);
    let motion = |plain: Motion, word: Option<Motion>| -> Option<Cmd> {
        if alt && shift {
            return matches!(plain, Motion::Up | Motion::Down | Motion::Left | Motion::Right)
                .then_some(Cmd::BoxMove(plain));
        }
        let m = if ctrl { word.unwrap_or(plain) } else { plain };
        Some(Cmd::Move(m, shift))
    };
    match key {
        Key::ArrowLeft => motion(Motion::Left, Some(Motion::WordLeft)),
        Key::ArrowRight => motion(Motion::Right, Some(Motion::WordRight)),
        Key::ArrowUp => motion(Motion::Up, None),
        Key::ArrowDown => motion(Motion::Down, None),
        Key::Home => motion(Motion::Home, Some(Motion::DocStart)),
        Key::End => motion(Motion::End, Some(Motion::DocEnd)),
        Key::PageUp => motion(Motion::PageUp, None),
        Key::PageDown => motion(Motion::PageDown, None),
        Key::Enter => Some(Cmd::Newline),
        Key::Tab if !ctrl => Some(if shift { Cmd::Outdent } else { Cmd::Indent }),
        Key::Backspace => Some(if ctrl { Cmd::WordBackspace } else { Cmd::Backspace }),
        Key::Delete => Some(if ctrl { Cmd::WordDelete } else { Cmd::Delete }),
        Key::Insert if !ctrl && !shift => Some(Cmd::ToggleOverwrite),
        Key::Escape if v.sels.sels.len() > 1 => Some(Cmd::CollapseSelections),
        _ => None,
    }
}

fn refresh_completion(doc: &Document, v: &mut View) {
    let head = v.sels.main().head;
    let Some(start) = v.complete.as_ref().map(|c| c.start) else { return };
    if head <= start {
        v.complete = None;
        return;
    }
    let prefix = doc.slice(start, head);
    let items = word_list(doc, &prefix, (start, head));
    if items.is_empty() {
        v.complete = None;
    } else if let Some(c) = v.complete.as_mut() {
        c.items = items;
        c.selected = 0;
    }
}

/// Wörter des Dokuments mit diesem Anfang.
pub fn word_list(doc: &Document, prefix: &str, skip: (usize, usize)) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let lower = prefix.to_lowercase();
    let plen = prefix.chars().count();
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut cur = String::new();
    let mut cur_start = 0usize;
    for (i, c) in doc.text.chars().chain(std::iter::once(' ')).enumerate() {
        if is_word(c) {
            if cur.is_empty() {
                cur_start = i;
            }
            cur.push(c);
        } else if !cur.is_empty() {
            let n = cur.chars().count();
            if n > plen && (cur_start, cur_start + n) != skip && cur.to_lowercase().starts_with(&lower) {
                seen.insert(std::mem::take(&mut cur));
                if seen.len() >= 200 {
                    break;
                }
            }
            cur.clear();
        }
    }
    seen.into_iter().collect()
}

fn accept_completion(doc: &mut Document, v: &mut View, o: &EdOpts, m: &mut Metrics, start: usize, item: &str) {
    let head = v.sels.main().head;
    let cmd = Cmd::Insert(item.to_string());
    v.sels = SelSet::range(start, head);
    Ed { doc, v, o, m }.apply(cmd);
}

/// Vervollständigung beim Tippen: ab der eingestellten Zeichenzahl (1 bis 9), reine Zahlen
/// werden übergangen. Sehr lange Dokumente bleiben ausgenommen, damit das Tippen flüssig bleibt.
fn auto_completion(doc: &Document, v: &mut View, o: &EdOpts) {
    const MAX_CHARS: usize = 2000000;
    if !o.auto_complete || doc.len() > MAX_CHARS || v.sels.sels.len() != 1 {
        return;
    }
    let head = v.sels.main().head;
    let mut start = head;
    while start > 0 {
        let c = doc.text.char(start - 1);
        if !(c.is_alphanumeric() || c == '_') {
            break;
        }
        start -= 1;
    }
    let prefix = doc.slice(start, head);
    if prefix.chars().count() < o.auto_complete_from.clamp(1, 9) {
        return;
    }
    if o.auto_complete_ignore_numbers && prefix.chars().all(|c| c.is_ascii_digit()) {
        return;
    }
    start_completion(doc, v);
}

/// Startet die Autovervollständigung am Cursor.
pub fn start_completion(doc: &Document, v: &mut View) {
    if doc.large {
        return;
    }
    let head = v.sels.main().head;
    let mut start = head;
    while start > 0 {
        let c = doc.text.char(start - 1);
        if c.is_alphanumeric() || c == '_' {
            start -= 1;
        } else {
            break;
        }
    }
    let prefix = doc.slice(start, head);
    let items = word_list(doc, &prefix, (start, head));
    v.complete = (!items.is_empty()).then_some(Complete { items, selected: 0, start });
}

/// Schieber der Bildlaufleiste: abgerundet, oder als erhabener Knopf im klassischen Look.
fn paint_thumb(p: &egui::Painter, thumb: Rect, pal: &Palette) {
    if pal.scroll_bevel {
        // Klassisch füllt der Schieber die ganze Breite der Leiste.
        let full = if thumb.width() < thumb.height() { thumb.expand2(egui::vec2(2.0, 0.0)) } else { thumb.expand2(egui::vec2(0.0, 2.0)) };
        let t = Rect::from_min_max(full.min.round(), full.max.round());
        p.rect_filled(t, 0.0, pal.scroll_fg);
        crate::theme::bevel(p, t, true);
    } else {
        p.rect_filled(thumb, 2.0, pal.scroll_fg);
    }
}
