//! Hintergrundarbeit für das Druckfenster: Einfärben, Seitenaufbau und Ausgabe.
//! Nichts davon läuft im UI-Thread; jedes Ergebnis kommt über einen Kanal und
//! weckt die Oberfläche mit `request_repaint`.
use super::font::PrintFont;
use super::{layout::paginate_until, Geom, LayoutOpts, Page, SrcLine};
use crate::highlight::{paper_spans_until, Span};
use egui::Color32;
use ropey::Rope;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

/// Ab so vielen Zeilen wird schwarz gedruckt (das Einfärben ab Zeile 0 dauert sonst zu lange).
pub const COLOR_MAX_LINES: usize = 200_000;

/// Eingaben für das Einfärben: hängt nur von Text, Markierung, Farbe, Tabweite und Sprache ab.
pub struct SrcReq {
    pub text: Rope,
    pub language: Option<String>,
    /// Zeichenbereich der Markierung; None = ganzes Dokument.
    pub range: Option<(usize, usize)>,
    pub color: bool,
    pub tab_size: usize,
}

/// Eingaben für den Seitenaufbau aus schon eingefärbten Zeilen.
pub struct LayoutReq {
    pub src: Arc<Vec<SrcLine>>,
    pub opts: LayoutOpts,
    pub geom: Geom,
    pub font: Arc<PrintFont>,
}

/// Führt `f` in einem eigenen Thread aus und weckt danach die Oberfläche.
pub fn spawn<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static, ctx: egui::Context) -> Receiver<T> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        // Ist das Fenster inzwischen zu, gibt es keinen Empfänger mehr; das ist kein Fehler.
        let _ = tx.send(f());
        ctx.request_repaint();
    });
    rx
}

/// Abbruchprüfung für einen Auftrag: veraltet, sobald jemand `generation` weiterzählt.
fn stale_check(generation: Arc<AtomicU64>) -> impl Fn() -> bool {
    let mine = generation.load(Ordering::SeqCst);
    move || generation.load(Ordering::SeqCst) != mine
}

/// Färbt die Zeilen ein. None = abgebrochen, weil ein neuerer Auftrag läuft.
pub fn spawn_src(req: SrcReq, generation: Arc<AtomicU64>, ctx: egui::Context) -> Receiver<Option<Arc<Vec<SrcLine>>>> {
    let stop = stale_check(generation);
    spawn(
        move || {
            src_lines_until(&req.text, req.language.as_deref(), req.range, req.color, req.tab_size, &stop).map(Arc::new)
        },
        ctx,
    )
}

/// Umbricht die Zeilen in Seiten. None = abgebrochen, weil ein neuerer Auftrag läuft.
pub fn spawn_layout(req: LayoutReq, generation: Arc<AtomicU64>, ctx: egui::Context) -> Receiver<Option<Arc<Vec<Page>>>> {
    let stop = stale_check(generation);
    spawn(move || paginate_until(&req.src, &req.opts, &req.geom, req.font.as_ref(), &stop).map(Arc::new), ctx)
}

pub fn spawn_output(f: impl FnOnce() -> Result<String, String> + Send + 'static, ctx: egui::Context) -> Receiver<Result<String, String>> {
    spawn(f, ctx)
}

/// Zeichen der Zeile ohne Zeilenende.
fn line_chars(text: &Rope, l: usize) -> Vec<char> {
    let mut v: Vec<char> = text.line(l).chars().collect();
    while matches!(v.last(), Some('\n' | '\r')) {
        v.pop();
    }
    v
}

/// Teil `from..to` (in Zeichen) einer Abschnittsliste.
fn cut(spans: &[Span], from: usize, to: usize) -> Vec<Span> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    for s in spans {
        let (a, b) = (pos.max(from), (pos + s.chars as usize).min(to));
        if a < b {
            out.push(Span { chars: (b - a) as u32, ..*s });
        }
        pos += s.chars as usize;
    }
    out
}

/// Bildschirmspalte des Zeichens `n` (Tabs bis zum nächsten Tabstopp).
fn column(chars: &[char], n: usize, tab: usize) -> usize {
    let tab = tab.max(1);
    chars[..n].iter().fold(0, |col, &c| if c == '\t' { (col / tab + 1) * tab } else { col + 1 })
}

/// Die zu druckenden Zeilen mit Papierfarben. Bei einer Markierung beginnt die erste
/// Zeile am Auswahlstart, mit Leerzeichen bis zu dessen Spalte aufgefüllt; die
/// Zeilennummern bleiben die des Dokuments.
#[cfg(test)]
pub fn src_lines(text: &Rope, language: Option<&str>, range: Option<(usize, usize)>, color: bool, tab: usize) -> Vec<SrcLine> {
    src_lines_until(text, language, range, color, tab, &|| false).expect("ohne Abbruch")
}

/// Wie `src_lines`, fragt aber alle 256 Zeilen `stop`; true bricht ab (None).
pub fn src_lines_until(
    text: &Rope,
    language: Option<&str>,
    range: Option<(usize, usize)>,
    color: bool,
    tab: usize,
    stop: &dyn Fn() -> bool,
) -> Option<Vec<SrcLine>> {
    let total = text.len_lines();
    let language = language.filter(|_| total < COLOR_MAX_LINES);
    let (first, last, a, b) = match range {
        Some((a, b)) => {
            let (a, b) = (a.min(b).min(text.len_chars()), a.max(b).min(text.len_chars()));
            let first = text.char_to_line(a);
            let mut last = text.char_to_line(b);
            // Endet die Auswahl am Zeilenanfang, gehört diese Zeile nicht mehr dazu.
            if last > first && text.line_to_char(last) == b {
                last -= 1;
            }
            (first, last, Some(a), Some(b))
        }
        None => {
            // Die leere Zeile nach dem letzten Zeilenende wird nicht gedruckt.
            let mut last = total.saturating_sub(1);
            if last > 0 && text.line(last).len_chars() == 0 {
                last -= 1;
            }
            (0, last, None, None)
        }
    };
    let spans = paper_spans_until(text, language, !color, first..last + 1, stop)?;
    let mut out = Vec::with_capacity(last + 1 - first);
    for (l, sp) in (first..=last).zip(spans) {
        if l % 256 == 0 && stop() {
            return None;
        }
        let chars = line_chars(text, l);
        let start = text.line_to_char(l);
        let from = if l == first { a.map_or(0, |a| (a - start).min(chars.len())) } else { 0 };
        let to = if l == last { b.map_or(chars.len(), |b| (b - start).min(chars.len())) } else { chars.len() };
        let pad = column(&chars, from, tab);
        let mut line_spans = Vec::new();
        if pad > 0 {
            line_spans.push(Span { chars: pad as u32, color: Color32::BLACK, bold: false });
        }
        line_spans.extend(cut(&sp, from, to));
        let body: String = chars[from..to.max(from)].iter().collect();
        out.push(SrcLine { number: l + 1, text: format!("{}{body}", " ".repeat(pad)), spans: line_spans });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_src_lines_selection_pads_and_cuts() {
        let text = Rope::from_str("eins\n\tzwei drei\nvier\n");
        // Ab "drei" bis Anfang Zeile 3: nur Zeile 2, ab Spalte von "drei".
        let a = text.line_to_char(1) + 6;
        let v = src_lines(&text, None, Some((a, text.line_to_char(2))), true, 4);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].number, 2);
        assert_eq!(v[0].text, format!("{}drei", " ".repeat(9)));
        assert_eq!(v[0].spans.iter().map(|s| s.chars).sum::<u32>(), 13);
    }

    #[test]
    fn print_src_lines_skips_trailing_empty_line() {
        let text = Rope::from_str("a\nb\n");
        let v = src_lines(&text, None, None, true, 4);
        assert_eq!(v.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(src_lines(&Rope::from_str(""), None, None, true, 4).len(), 1);
    }

    #[test]
    fn print_src_lines_until_stops_when_stale() {
        let text = Rope::from_str(&"fn a() {}\n".repeat(1000));
        let generation = Arc::new(AtomicU64::new(0));
        let stop = stale_check(generation.clone());
        assert!(!stop());
        assert!(src_lines_until(&text, Some("Rust"), None, true, 4, &stop).is_some());
        generation.fetch_add(1, Ordering::SeqCst);
        assert!(stop());
        assert!(src_lines_until(&text, Some("Rust"), None, true, 4, &stop).is_none());
    }
}
