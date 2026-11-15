//! Drucken: plattformunabhängiger Seitenaufbau. Vorschau, PDF und Windows-GDI
//! zeichnen nur, was `layout::paginate` liefert, und rechnen nichts selbst.

// Unter Windows nur für die Tests der Auswertung übersetzt.
#[cfg(any(test, all(unix, not(target_os = "macos"))))]
pub mod cups;
pub mod font;
pub mod job;
pub mod layout;
pub mod pdf;
#[cfg(all(unix, not(target_os = "macos")))]
mod sys_linux;
#[cfg(windows)]
mod sys_windows;

#[cfg(all(unix, not(target_os = "macos")))]
pub use sys_linux::{paper_for, print_pages, printers};
#[cfg(windows)]
pub use sys_windows::{paper_for, print_pages, printers, properties};

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

/// Farbe oder Schwarzweiß steckt schon in den Abschnitten (`paper_spans`).
pub struct LayoutOpts {
    pub size: f32,
    pub line_numbers: bool,
    pub header_footer: bool,
    pub tab_size: usize,
    pub header_left: String,
    pub header_right: String,
    /// Mit `{page}` und `{total}`.
    pub footer: String,
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

/// Ein Drucker, wie ihn das System meldet.
#[derive(Clone, Debug, PartialEq)]
pub struct Printer {
    pub name: String,
    pub is_default: bool,
}

/// Papier eines Druckers. Die Ränder von `geom_full` sind der nicht bedruckbare Bereich.
#[derive(Clone, Debug, PartialEq)]
pub struct Paper {
    /// "A4" … oder der Formularname des Treibers.
    pub name: String,
    pub geom_full: Geom,
}

/// Wählbare Formate (Name, Breite, Höhe in pt).
pub const PAPERS: [(&str, f32, f32); 4] =
    [("A4", 595.0, 842.0), ("Letter", 612.0, 792.0), ("A5", 420.0, 595.0), ("Legal", 612.0, 1008.0)];

/// Papier aus `PAPERS` nach Namen (Groß-/Kleinschreibung egal), ohne Rand.
pub fn paper_named(name: &str) -> Option<Paper> {
    PAPERS.iter().find(|p| p.0.eq_ignore_ascii_case(name)).map(|&(n, w, h)| Paper {
        name: n.to_string(),
        geom_full: Geom { w, h, left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 },
    })
}

/// Vorgabe, wenn weder Wahl noch Drucker ein Format liefern: Letter bei en-US, sonst A4.
pub fn default_paper() -> Paper {
    let us = sys_locale::get_locale().is_some_and(|l| l.eq_ignore_ascii_case("en-US"));
    paper_named(if us { "Letter" } else { "A4" }).expect("Format in PAPERS")
}
