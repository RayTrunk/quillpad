// Wird entfernt, sobald der Druckdialog angebunden ist (Task 7).
#![allow(dead_code)]
//! Drucken: plattformunabhängiger Seitenaufbau. Vorschau, PDF und Windows-GDI
//! zeichnen nur, was `layout::paginate` liefert, und rechnen nichts selbst.

pub mod layout;

/// Punkt pro Millimeter.
pub const MM: f32 = 72.0 / 25.4;
/// Grau für Kopfzeile, Fußzeile und Zeilennummern.
pub const GRAY: [u8; 3] = [0x66, 0x66, 0x66];

/// Seitengröße und Ränder in Punkt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geom {
    pub w: f32,
    pub h: f32,
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// Eine Zeile des Dokuments.
pub struct SrcLine {
    /// 1-basierte Zeilennummer im Dokument.
    pub number: usize,
    /// Text ohne Zeilenende.
    pub text: String,
    pub spans: Vec<crate::highlight::Span>,
}

pub struct LayoutOpts {
    pub size: f32,
    pub line_numbers: bool,
    pub header_footer: bool,
    pub tab_size: usize,
    pub header_left: String,
    pub header_right: String,
    /// Mit `{page}` und `{total}`.
    pub footer: String,
    pub mono: bool,
}

/// Schriftmaße; alle Werte bei 1 pt Schriftgröße.
pub trait Measure {
    fn advance(&self, c: char, bold: bool) -> f32;
    /// 0 = Hauptschrift, 1.. = Ersatzschrift.
    fn face(&self, c: char) -> u8;
    fn ascent(&self) -> f32;
}

/// Zusammenhängender Text mit einheitlicher Darstellung. `x` absolut ab linker
/// Seitenkante, `y` Grundlinie ab Seitenoberkante.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub color: [u8; 3],
    pub bold: bool,
    pub face: u8,
    pub size: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    pub runs: Vec<Run>,
    /// x0, x1, y
    pub rules: Vec<[f32; 3]>,
}
