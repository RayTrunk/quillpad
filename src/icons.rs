//! Symbole der Oberfläche. Unter Windows kommen sie aus der Systemschrift „Segoe Fluent Icons“
//! (Windows 11), sonst aus „Segoe MDL2 Assets“ (Windows 10). Fehlen beide, etwa unter Linux,
//! werden die Symbole selbst gezeichnet (Koordinaten in einem 16er-Raster).
use egui::{pos2, Align2, Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind};
use std::sync::atomic::{AtomicU8, Ordering};

/// Name der Schriftfamilie für die Symbolschrift.
pub const FAMILY: &str = "icons";

/// Welche Symbolschrift geladen ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconFont {
    None = 0,
    Fluent = 1,
    Mdl2 = 2,
}

static ICON_FONT: AtomicU8 = AtomicU8::new(0);

pub fn set_icon_font(f: IconFont) {
    ICON_FONT.store(f as u8, Ordering::Relaxed);
}

pub fn icon_font() -> IconFont {
    match ICON_FONT.load(Ordering::Relaxed) {
        1 => IconFont::Fluent,
        2 => IconFont::Mdl2,
        _ => IconFont::None,
    }
}

/// Feste Zeichen der Segoe-Symbolschriften für kleine Bedienelemente.
pub const GLYPH_CLOSE: char = '\u{E711}';
pub const GLYPH_CHEVRON_DOWN: char = '\u{E70D}';
pub const GLYPH_CHEVRON_RIGHT: char = '\u{E76C}';

/// Schrift für ein Symbolzeichen, nur gültig, wenn eine Symbolschrift geladen ist.
pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY.into()))
}

/// Text für kleine Pfeile und Kreuze: Segoe-Zeichen, wenn vorhanden, sonst ein Unicode-Ersatz.
pub fn rich(glyph: char, fallback: &str, size: f32) -> egui::RichText {
    if icon_font() == IconFont::None {
        egui::RichText::new(fallback).size(size)
    } else {
        egui::RichText::new(glyph.to_string()).font(font(size))
    }
}

/// Zeichnet ein Symbolzeichen mittig in `r`. false, wenn keine Symbolschrift geladen ist.
pub fn paint_glyph(p: &Painter, r: Rect, glyph: char, size: f32, color: Color32) -> bool {
    if icon_font() == IconFont::None {
        return false;
    }
    p.text(r.center(), Align2::CENTER_CENTER, glyph, font(size), color);
    true
}

/// Zeichen für ein Symbol der Werkzeugleiste.
fn glyph(icon: Icon) -> char {
    let mdl2 = icon_font() == IconFont::Mdl2;
    let cp = match icon {
        Icon::New => 0xE8A5,
        Icon::Open => 0xE838,
        Icon::Save => 0xE74E,
        Icon::SaveAll => 0xEA35,
        Icon::Close => 0xE711,
        Icon::CloseAll => 0xEA39,
        Icon::Print => 0xE749,
        Icon::Cut => 0xE8C6,
        Icon::Copy => 0xE8C8,
        Icon::Paste => 0xE77F,
        Icon::Undo => 0xE7A7,
        Icon::Redo => 0xE7A6,
        Icon::Find => 0xE721,
        Icon::Replace => 0xE8EE,
        Icon::ZoomIn => 0xE8A3,
        Icon::ZoomOut => 0xE71F,
        Icon::SyncV => 0xE8CB,
        Icon::SyncH => 0xE8AB,
        Icon::Wrap => 0xE751,
        // Absatzzeichen aus Segoe UI (Rückfall der Familie), passt besser als die Varianten mit Pfeil.
        Icon::ShowAll => 0x00B6,
        Icon::Guides => 0xE8E4,
        Icon::Split => 0xE736,
        // MDL2 hat unter E800 kein Kartensymbol.
        Icon::Map => if mdl2 { 0xE826 } else { 0xE800 },
        Icon::Funcs => 0xE943,
        Icon::Folder => 0xE8B7,
        Icon::Monitor => 0xE890,
    };
    char::from_u32(cp).unwrap_or('?')
}

/// Farbe eines Schriftsymbols: einfarbig wie Fluent.
fn glyph_color(enabled: bool, on_dark: bool) -> Color32 {
    match (enabled, on_dark) {
        (false, true) => Color32::from_gray(0x70),
        (false, false) => Color32::from_gray(0xA8),
        (true, true) => Color32::from_gray(0xE0),
        (true, false) => Color32::from_gray(0x20),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    New,
    Open,
    Save,
    SaveAll,
    Close,
    CloseAll,
    Print,
    Cut,
    Copy,
    Paste,
    Undo,
    Redo,
    Find,
    Replace,
    ZoomIn,
    ZoomOut,
    SyncV,
    SyncH,
    Wrap,
    ShowAll,
    Guides,
    Split,
    Map,
    Funcs,
    Folder,
    Monitor,
}

struct G<'a> {
    p: &'a Painter,
    r: Rect,
    dim: bool,
    on_dark: bool,
}

impl G<'_> {
    fn pt(&self, x: f32, y: f32) -> Pos2 {
        pos2(
            self.r.left() + x / 16.0 * self.r.width(),
            self.r.top() + y / 16.0 * self.r.height(),
        )
    }
    fn col(&self, c: u32) -> Color32 {
        let (r, g, b) = ((c >> 16) as u8, (c >> 8) as u8, c as u8);
        let lum = (r as u32 * 3 + g as u32 * 6 + b as u32) / 10;
        // Sehr dunkle Teile bleiben auf dunklem Hintergrund sichtbar.
        if self.on_dark && !self.dim && lum < 0x68 {
            return Color32::from_rgb(r.saturating_add(0x78), g.saturating_add(0x78), b.saturating_add(0x78));
        }
        if self.dim {
            let l = ((r as u32 * 3 + g as u32 * 6 + b as u32) / 10) as u8;
            let v = l / 2 + 110;
            Color32::from_rgb(v, v, v)
        } else {
            Color32::from_rgb(r, g, b)
        }
    }
    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, fill: u32) {
        self.p.rect_filled(Rect::from_min_max(self.pt(x0, y0), self.pt(x1, y1)), 0.0, self.col(fill));
    }
    fn frame(&self, x0: f32, y0: f32, x1: f32, y1: f32, fill: u32, line: u32) {
        let r = Rect::from_min_max(self.pt(x0, y0), self.pt(x1, y1));
        self.p.rect(r, CornerRadius::ZERO, self.col(fill), Stroke::new(1.0, self.col(line)), StrokeKind::Inside);
    }
    fn line(&self, pts: &[(f32, f32)], w: f32, c: u32) {
        let v: Vec<Pos2> = pts.iter().map(|(x, y)| self.pt(*x, *y)).collect();
        self.p.add(Shape::line(v, Stroke::new(w, self.col(c))));
    }
    fn poly(&self, pts: &[(f32, f32)], c: u32) {
        let v: Vec<Pos2> = pts.iter().map(|(x, y)| self.pt(*x, *y)).collect();
        self.p.add(Shape::convex_polygon(v, self.col(c), Stroke::NONE));
    }
    fn dot(&self, x: f32, y: f32, rad: f32, fill: u32) {
        self.p.circle_filled(self.pt(x, y), rad / 16.0 * self.r.width(), self.col(fill));
    }
    fn ring(&self, x: f32, y: f32, rad: f32, w: f32, c: u32) {
        self.p.circle_stroke(self.pt(x, y), rad / 16.0 * self.r.width(), Stroke::new(w, self.col(c)));
    }
    fn page(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.frame(x0, y0, x1, y1, 0xFAFAFA, 0x707070);
    }
    fn floppy(&self, x: f32, y: f32, s: f32, body: u32) {
        let f = s / 14.0;
        self.frame(x, y, x + 14.0 * f, y + 14.0 * f, body, 0x2A4A7A);
        self.rect(x + 3.0 * f, y + 1.0 * f, x + 11.0 * f, y + 6.0 * f, 0xF4F4F4);
        self.rect(x + 3.5 * f, y + 9.0 * f, x + 10.5 * f, y + 13.0 * f, 0xD8D8E8);
        self.rect(x + 8.0 * f, y + 10.0 * f, x + 9.5 * f, y + 12.5 * f, 0x505070);
    }
    fn folder(&self, a: u32, b: u32) {
        self.poly(&[(1.0, 3.0), (6.0, 3.0), (7.5, 5.0), (14.0, 5.0), (14.0, 7.0), (1.0, 7.0)], b);
        self.frame(1.0, 6.0, 15.0, 14.0, a, 0x9A7A20);
    }
    fn red_x(&self, cx: f32, cy: f32) {
        self.dot(cx, cy, 3.6, 0xD84030);
        self.line(&[(cx - 1.6, cy - 1.6), (cx + 1.6, cy + 1.6)], 1.3, 0xFFFFFF);
        self.line(&[(cx + 1.6, cy - 1.6), (cx - 1.6, cy + 1.6)], 1.3, 0xFFFFFF);
    }
    fn lens(&self, cx: f32, cy: f32) {
        self.dot(cx, cy, 4.6, 0xDDEEFF);
        self.ring(cx, cy, 4.6, 1.5, 0x406080);
        self.line(&[(cx + 3.3, cy + 3.3), (cx + 7.0, cy + 7.0)], 2.2, 0x8A6A30);
    }
    fn binoculars(&self) {
        self.frame(1.5, 5.0, 7.0, 13.5, 0x404040, 0x202020);
        self.frame(9.0, 5.0, 14.5, 13.5, 0x404040, 0x202020);
        self.rect(6.5, 6.0, 9.5, 9.0, 0x303030);
        self.rect(3.0, 2.5, 6.5, 5.0, 0x606060);
        self.rect(9.5, 2.5, 13.0, 5.0, 0x606060);
        self.dot(4.2, 12.0, 1.3, 0x9AB8E0);
        self.dot(11.8, 12.0, 1.3, 0x9AB8E0);
    }
}

/// `classic`: die bunten, selbst gezeichneten Symbole statt der Segoe-Schrift (Windows-98-Look).
pub fn paint(p: &Painter, r: Rect, icon: Icon, enabled: bool, on_dark: bool, classic: bool) {
    // Absatzzeichen ist ein Textzeichen und braucht etwas mehr Größe als die Symbole.
    let size = if icon == Icon::ShowAll { r.height() * 1.2 } else { r.height() * 0.89 };
    if !classic && paint_glyph(p, r, glyph(icon), size, glyph_color(enabled, on_dark)) {
        return;
    }
    let g = G { p, r, dim: !enabled, on_dark };
    match icon {
        Icon::New => {
            g.page(3.0, 1.0, 12.0, 15.0);
            g.dot(11.5, 12.0, 3.6, 0x30A040);
            g.line(&[(9.6, 12.0), (13.4, 12.0)], 1.4, 0xFFFFFF);
            g.line(&[(11.5, 10.1), (11.5, 13.9)], 1.4, 0xFFFFFF);
        }
        Icon::Open => g.folder(0xF4D060, 0xE0B840),
        Icon::Save => g.floppy(1.0, 1.0, 14.0, 0x3A78C8),
        Icon::SaveAll => {
            g.floppy(0.5, 0.5, 11.0, 0x7AA0D8);
            g.floppy(4.5, 4.5, 11.0, 0x3A78C8);
        }
        Icon::Close => {
            g.page(3.0, 1.0, 12.0, 15.0);
            g.red_x(11.5, 12.0);
        }
        Icon::CloseAll => {
            g.page(1.5, 0.5, 9.5, 12.5);
            g.page(5.0, 2.5, 13.0, 15.0);
            g.red_x(12.0, 12.0);
        }
        Icon::Print => {
            g.page(4.0, 1.0, 12.0, 7.0);
            g.frame(1.0, 6.0, 15.0, 12.0, 0xA8A8A8, 0x505050);
            g.page(4.0, 10.0, 12.0, 15.0);
            g.dot(13.0, 8.0, 0.8, 0x30C040);
        }
        Icon::Cut => {
            g.line(&[(4.0, 1.5), (10.5, 11.0)], 1.5, 0x707070);
            g.line(&[(12.0, 1.5), (5.5, 11.0)], 1.5, 0x707070);
            g.ring(4.0, 12.5, 2.3, 1.6, 0xD03030);
            g.ring(12.0, 12.5, 2.3, 1.6, 0xD03030);
        }
        Icon::Copy => {
            g.frame(1.5, 1.0, 9.5, 11.5, 0xD8E6F6, 0x3A6AA8);
            g.frame(6.0, 4.5, 14.5, 15.0, 0xF4F8FC, 0x3A6AA8);
            for y in [7.0, 9.5, 12.0] {
                g.line(&[(8.0, y), (12.5, y)], 1.0, 0x8AA0C0);
            }
        }
        Icon::Paste => {
            g.frame(2.0, 2.5, 12.0, 15.0, 0xB89060, 0x6A4A20);
            g.frame(5.0, 1.0, 9.0, 4.0, 0xC8C8C8, 0x606060);
            g.frame(7.0, 6.0, 14.5, 15.0, 0xFAFAFA, 0x707070);
        }
        Icon::Undo => {
            g.line(&[(4.0, 6.0), (10.0, 6.0), (13.0, 8.0), (13.0, 11.0), (10.5, 13.0)], 2.2, 0x606060);
            g.poly(&[(5.5, 2.0), (5.5, 10.0), (1.0, 6.0)], 0x606060);
        }
        Icon::Redo => {
            g.line(&[(12.0, 6.0), (6.0, 6.0), (3.0, 8.0), (3.0, 11.0), (5.5, 13.0)], 2.2, 0x606060);
            g.poly(&[(10.5, 2.0), (15.0, 6.0), (10.5, 10.0)], 0x606060);
        }
        Icon::Find => g.binoculars(),
        Icon::Replace => {
            g.binoculars();
            g.frame(8.5, 8.5, 15.5, 15.5, 0x4A90D0, 0x20508A);
            g.line(&[(10.0, 12.0), (14.0, 12.0)], 1.4, 0xFFFFFF);
            g.line(&[(12.0, 10.0), (14.0, 12.0), (12.0, 14.0)], 1.4, 0xFFFFFF);
        }
        Icon::ZoomIn => {
            g.lens(6.5, 6.5);
            g.line(&[(4.2, 6.5), (8.8, 6.5)], 1.6, 0x208030);
            g.line(&[(6.5, 4.2), (6.5, 8.8)], 1.6, 0x208030);
        }
        Icon::ZoomOut => {
            g.lens(6.5, 6.5);
            g.line(&[(4.2, 6.5), (8.8, 6.5)], 1.6, 0xC03030);
        }
        Icon::SyncV => {
            g.frame(1.0, 1.0, 15.0, 7.0, 0xF4F4F4, 0x606060);
            g.rect(1.0, 1.0, 15.0, 3.0, 0x8090A8);
            g.frame(1.0, 9.0, 15.0, 15.0, 0xF4F4F4, 0x606060);
            g.rect(1.0, 9.0, 15.0, 11.0, 0x8090A8);
        }
        Icon::SyncH => {
            g.frame(1.0, 1.0, 7.0, 15.0, 0xF4F4F4, 0x606060);
            g.rect(1.0, 1.0, 7.0, 3.5, 0x8090A8);
            g.frame(9.0, 1.0, 15.0, 15.0, 0xF4F4F4, 0x606060);
            g.rect(9.0, 1.0, 15.0, 3.5, 0x8090A8);
        }
        Icon::Wrap => {
            g.line(&[(1.0, 3.0), (15.0, 3.0)], 1.5, 0x404040);
            g.line(&[(1.0, 7.5), (12.0, 7.5), (14.5, 9.5), (12.0, 12.0), (8.0, 12.0)], 1.5, 0x3070C0);
            g.poly(&[(8.5, 9.5), (8.5, 14.5), (5.5, 12.0)], 0x3070C0);
            g.line(&[(1.0, 12.0), (4.5, 12.0)], 1.5, 0x404040);
        }
        Icon::ShowAll => {
            g.dot(6.0, 5.5, 3.8, 0x3070C0);
            g.line(&[(9.0, 1.5), (9.0, 15.0)], 1.5, 0x3070C0);
            g.line(&[(12.0, 1.5), (12.0, 15.0)], 1.5, 0x3070C0);
            g.line(&[(6.0, 1.5), (14.0, 1.5)], 1.5, 0x3070C0);
        }
        Icon::Guides => {
            for (x, y, w) in [(4.0, 2.0, 10.0), (6.5, 5.5, 7.5), (6.5, 9.0, 5.0), (4.0, 12.5, 8.0)] {
                g.line(&[(x, y), (x + w, y)], 1.8, 0x2060B0);
            }
            g.line(&[(2.0, 1.0), (2.0, 15.0)], 1.5, 0x404040);
        }
        Icon::Split => {
            g.frame(1.0, 2.0, 15.0, 14.0, 0xFAFAFA, 0x606060);
            g.line(&[(8.0, 2.0), (8.0, 14.0)], 1.5, 0x3070C0);
        }
        Icon::Map => {
            g.poly(&[(1.0, 3.0), (5.5, 1.5), (5.5, 13.0), (1.0, 14.5)], 0xF0D060);
            g.poly(&[(5.5, 1.5), (10.5, 3.0), (10.5, 14.5), (5.5, 13.0)], 0xD8B840);
            g.poly(&[(10.5, 3.0), (15.0, 1.5), (15.0, 13.0), (10.5, 14.5)], 0xF0D060);
            g.line(&[(2.5, 11.0), (7.0, 6.5), (10.0, 9.5), (13.5, 4.5)], 1.2, 0xC03030);
        }
        Icon::Funcs => {
            g.page(2.0, 1.0, 14.0, 15.0);
            g.line(&[(10.0, 3.5), (8.5, 3.0), (7.5, 4.5), (7.0, 12.0), (5.5, 13.0), (4.5, 12.5)], 1.6, 0xC03030);
            g.line(&[(5.5, 7.0), (10.0, 7.0)], 1.4, 0xC03030);
        }
        Icon::Folder => g.folder(0xE8A0B0, 0xD08098),
        Icon::Monitor => {
            g.poly(&[(1.0, 8.0), (4.0, 4.5), (8.0, 3.5), (12.0, 4.5), (15.0, 8.0), (12.0, 11.5), (8.0, 12.5), (4.0, 11.5)], 0x808080);
            g.dot(8.0, 8.0, 2.8, 0xE8E8E8);
            g.dot(8.0, 8.0, 1.3, 0x303030);
        }
    }
}

/// Kleines Schloss für schreibgeschützte Tabs.
pub fn paint_lock(p: &Painter, r: Rect, color: Color32) {
    if paint_glyph(p, r, '\u{E72E}', r.height() * 0.75, color) {
        return;
    }
    let c = r.center();
    let body = Rect::from_center_size(c + egui::vec2(0.0, 2.0), egui::vec2(9.0, 7.0));
    p.rect_filled(body, 1.0, color);
    p.circle_stroke(c + egui::vec2(0.0, -1.5), 3.0, Stroke::new(1.4, color));
}
