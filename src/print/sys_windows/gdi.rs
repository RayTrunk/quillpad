//! GDI-Ausgabe eines Druckauftrags. Jede Zeichenfolge steht an der Stelle, die
//! `layout` berechnet hat (`ExtTextOutW` mit `lpDx`, Rest der Rundung mitgeführt).
use super::{api_err, caps, devmode, from_wide, wide};
use crate::print::font::PrintFont;
use crate::print::{Geom, Measure, Page, GRAY};
use std::collections::HashMap;
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HANDLE, RECT};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::{AbortDoc, EndDoc, EndPage, StartDocW, StartPage, DOCINFOW};

/// Aufräumen eines Druckauftrags in jedem Fall, auch nach Fehlern.
struct Job {
    dc: HDC,
    old_font: HGDIOBJ,
    fonts: HashMap<(u8, bool, u32), HFONT>,
    brush: HBRUSH,
    mem_fonts: Vec<HANDLE>,
    open_doc: bool,
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: Alle Handles stammen aus diesem Auftrag; Schriften werden vor dem Löschen abgewählt.
        unsafe {
            if self.open_doc {
                AbortDoc(self.dc);
            }
            if !self.old_font.is_invalid() {
                SelectObject(self.dc, self.old_font);
            }
            for f in self.fonts.values() {
                let _ = DeleteObject(*f);
            }
            if !self.brush.is_invalid() {
                let _ = DeleteObject(self.brush);
            }
            let _ = DeleteDC(self.dc);
            for h in &self.mem_fonts {
                let _ = RemoveFontMemResourceEx(*h);
            }
        }
    }
}

fn rgb(c: [u8; 3]) -> COLORREF {
    COLORREF(c[0] as u32 | (c[1] as u32) << 8 | (c[2] as u32) << 16)
}

/// Familienname für CreateFontW, höchstens 31 Zeichen plus Null.
fn face_name(family: &str) -> Vec<u16> {
    let mut v: Vec<u16> = family.encode_utf16().take(31).collect();
    v.push(0);
    v
}

fn create_font(height: i32, bold: bool, face: &[u16]) -> HFONT {
    let weight = if bold { FW_BOLD } else { FW_NORMAL };
    // SAFETY: `face` ist nullterminiert; die übrigen Werte sind einfache Zahlen.
    unsafe {
        CreateFontW(
            height, 0, 0, 0, weight.0 as i32, 0, 0, 0,
            DEFAULT_CHARSET.0 as u32, OUT_TT_PRECIS.0 as u32, CLIP_DEFAULT_PRECIS.0 as u32,
            DEFAULT_QUALITY.0 as u32, DEFAULT_PITCH.0 as u32, PCWSTR(face.as_ptr()),
        )
    }
}

/// Wie `print_pages`; `output` lenkt den Auftrag in eine Datei (`DOCINFOW.lpszOutput`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn print_to(
    printer: &str,
    pages: &[Page],
    _g: &Geom,
    font: &PrintFont,
    title: &str,
    copies: u32,
    duplex: bool,
    output: Option<&Path>,
) -> Result<(), String> {
    let mut dm = devmode(printer)?;
    {
        let d = dm.dm();
        d.Anonymous1.Anonymous1.dmCopies = copies.clamp(1, i16::MAX as u32) as i16;
        d.dmFields = DEVMODE_FIELD_FLAGS(d.dmFields.0 | DM_COPIES.0);
        // Das Druckfenster hat unter Windows keine Duplex-Wahl: ohne `duplex` bleibt die
        // Einstellung des Treibers (oder aus „Eigenschaften…“) unangetastet.
        if duplex {
            d.dmDuplex = DMDUP_VERTICAL;
            d.dmFields = DEVMODE_FIELD_FLAGS(d.dmFields.0 | DM_DUPLEX.0);
        }
    }
    let name = wide(printer);
    // SAFETY: Name nullterminiert, DEVMODE vom Treiber mit geänderten Kopien/Duplex.
    let dc = unsafe { CreateDCW(PCWSTR::null(), PCWSTR(name.as_ptr()), PCWSTR::null(), Some(dm.ptr())) };
    if dc.is_invalid() {
        return Err(api_err("CreateDCW"));
    }
    let mut job = Job { dc, old_font: HGDIOBJ::default(), fonts: HashMap::new(), brush: HBRUSH::default(), mem_fonts: Vec::new(), open_doc: false };
    let k = caps(dc);

    // Familiennamen je (Schrift, fett); nicht installierte Schriften kommen privat aus dem Speicher.
    let mut families: HashMap<(u8, bool), Vec<u16>> = HashMap::new();
    for r in pages.iter().flat_map(|p| &p.runs) {
        if (r.face as usize) > font.fallbacks.len() {
            return Err(format!("unbekannte Ersatzschrift {}", r.face));
        }
        if families.contains_key(&(r.face, r.bold)) {
            continue;
        }
        let face = font.face_data(r.face, r.bold);
        let family = face.family_name().ok_or_else(|| "Schrift ohne Familiennamen".to_string())?;
        let wname = face_name(&family);
        if !installed(dc, &wname) {
            let mut n = 0u32;
            // `n` ist ein Ausgabewert, die Bindung erwartet aber `*const u32`.
            let pn = std::ptr::addr_of_mut!(n).cast_const();
            // SAFETY: GDI kopiert die Schriftdaten; `pn` zeigt auf eine beschreibbare Variable.
            let h = unsafe { AddFontMemResourceEx(face.data.as_ptr().cast(), face.data.len() as u32, None, pn) };
            if !h.is_invalid() {
                job.mem_fonts.push(h);
            }
        }
        families.insert((r.face, r.bold), wname);
    }

    let out_w = output.map(|p| wide(&p.to_string_lossy()));
    let title_w = wide(title);
    let di = DOCINFOW {
        cbSize: std::mem::size_of::<DOCINFOW>() as i32,
        lpszDocName: PCWSTR(title_w.as_ptr()),
        lpszOutput: out_w.as_ref().map_or(PCWSTR::null(), |w| PCWSTR(w.as_ptr())),
        ..Default::default()
    };
    // SAFETY: DOCINFOW und seine Zeichenketten leben bis nach dem Aufruf.
    if unsafe { StartDocW(dc, &di) } <= 0 {
        return Err(api_err("StartDocW"));
    }
    job.open_doc = true;
    // SAFETY: Farbe als einfacher Wert.
    job.brush = unsafe { CreateSolidBrush(rgb(GRAY)) };

    let px = |x: f32| x as f64 * k.dpi_x as f64 / 72.0 - k.off_x as f64;
    let py = |y: f32| y as f64 * k.dpi_y as f64 / 72.0 - k.off_y as f64;
    for page in pages {
        // SAFETY: `dc` ist ein offener Druckerkontext.
        if unsafe { StartPage(dc) } <= 0 {
            return Err(api_err("StartPage"));
        }
        // SAFETY: Einstellungen am gültigen Kontext; StartPage kann sie zurücksetzen, daher je Seite.
        unsafe {
            SetBkMode(dc, TRANSPARENT);
            SetTextAlign(dc, TEXT_ALIGN_OPTIONS(TA_BASELINE.0 | TA_LEFT.0));
        }
        let thick = (0.5 * k.dpi_y / 72.0).round().max(1.0) as i32;
        for &[x0, x1, y] in &page.rules {
            let top = (py(y) - thick as f64 / 2.0).round() as i32;
            let rc = RECT { left: px(x0).round() as i32, top, right: px(x1).round() as i32, bottom: top + thick };
            // SAFETY: Rechteck lebt während des Aufrufs, Pinsel gültig.
            unsafe { FillRect(dc, &rc, job.brush) };
        }
        for r in &page.runs {
            let key = (r.face, r.bold, r.size.to_bits());
            let hf = match job.fonts.get(&key) {
                Some(&f) => f,
                None => {
                    let f = create_font(-(r.size * k.dpi_y / 72.0).round() as i32, r.bold, &families[&(r.face, r.bold)]);
                    if f.is_invalid() {
                        return Err(api_err("CreateFontW"));
                    }
                    job.fonts.insert(key, f);
                    f
                }
            };
            // SAFETY: Schrift und Kontext gültig; die erste Auswahl merkt sich die Ausgangsschrift.
            let prev = unsafe { SelectObject(dc, hf) };
            if job.old_font.is_invalid() {
                job.old_font = prev;
            }
            // SAFETY: Farbe als einfacher Wert.
            unsafe { SetTextColor(dc, rgb(r.color)) };
            // Abstände je UTF-16-Einheit: exakte Position runden, Rest mitführen.
            let x0 = px(r.x);
            let start = x0.round() as i32;
            let scale = r.size as f64 * k.dpi_x as f64 / 72.0;
            let (mut pos, mut done) = (x0, start);
            let mut units = Vec::with_capacity(r.text.len());
            let mut dx = Vec::with_capacity(r.text.len());
            for c in r.text.chars() {
                pos += font.advance(c, r.bold) as f64 * scale;
                let step = pos.round() as i32 - done;
                done += step;
                let mut b = [0u16; 2];
                let enc = c.encode_utf16(&mut b);
                units.push(enc[0]);
                dx.push(step);
                if enc.len() == 2 {
                    // Ersatzpaar: der Vorschub steht auf der ersten Einheit.
                    units.push(enc[1]);
                    dx.push(0);
                }
            }
            // SAFETY: `units` und `dx` haben gleich viele Einträge und leben während des Aufrufs.
            let ok = unsafe {
                ExtTextOutW(dc, start, py(r.y).round() as i32, ETO_OPTIONS(0), None, PCWSTR(units.as_ptr()), units.len() as u32, Some(dx.as_ptr()))
            };
            if !ok.as_bool() {
                return Err(api_err("ExtTextOutW"));
            }
        }
        // SAFETY: Seite wurde mit StartPage begonnen.
        if unsafe { EndPage(dc) } <= 0 {
            return Err(api_err("EndPage"));
        }
    }
    // SAFETY: Auftrag wurde mit StartDocW begonnen.
    if unsafe { EndDoc(dc) } <= 0 {
        return Err(api_err("EndDoc"));
    }
    job.open_doc = false;
    Ok(())
}

/// Liefert GDI für diesen Namen genau diese Familie (und keinen Ersatz)?
fn installed(dc: HDC, face: &[u16]) -> bool {
    let f = create_font(-100, false, face);
    if f.is_invalid() {
        return false;
    }
    let mut got = [0u16; 32];
    // SAFETY: Probeschrift kurz auswählen, Namen lesen, Ausgangsschrift zurück und löschen.
    let n = unsafe {
        let old = SelectObject(dc, f);
        let n = GetTextFaceW(dc, Some(&mut got));
        SelectObject(dc, old);
        let _ = DeleteObject(f);
        n
    };
    n > 0 && from_wide(&got).eq_ignore_ascii_case(&from_wide(face))
}

#[cfg(test)]
mod tests {
    use super::super::{paper_for, printers};
    use super::*;
    use crate::print::{layout::paginate, LayoutOpts, SrcLine, MM};

    #[test]
    #[ignore]
    fn gdi_prints_to_pdf_file() {
        const PDF_PRINTER: &str = "Microsoft Print to PDF";
        let list = printers().expect("Druckerliste");
        if !list.iter().any(|p| p.name == PDF_PRINTER) {
            eprintln!("{PDF_PRINTER} fehlt, Test übersprungen");
            return;
        }
        let font = PrintFont::load("Courier New").expect("Schrift");
        let paper = paper_for(PDF_PRINTER, None);
        let m = 15.0 * MM;
        let g = Geom { w: paper.geom_full.w, h: paper.geom_full.h, left: m, top: m, right: m, bottom: m };
        let o = LayoutOpts {
            size: 10.0, line_numbers: true, header_footer: true, tab_size: 4,
            header_left: "gdi.rs".into(), header_right: "heute".into(),
            footer: "{page}/{total}".into(),
        };
        let mut texts = vec!["fn main() {}".to_string(), "Grüße 一 \u{1F600} Ende".to_string()];
        texts.extend((0..80).map(|i| format!("Füllung {i}")));
        let src: Vec<SrcLine> =
            texts.iter().enumerate().map(|(i, t)| SrcLine { number: i + 1, text: t.clone(), spans: Vec::new() }).collect();
        let pages = paginate(&src, &o, &g, &font);
        assert!(pages.len() >= 2);
        let out = std::env::temp_dir().join(format!("quillpad-gdi-test-{}.pdf", std::process::id()));
        let out = out.as_path();
        let _ = std::fs::remove_file(out);
        print_to(PDF_PRINTER, &pages[..2], &g, &font, "GDI-Test", 1, false, Some(out)).expect("Druck");
        // Der Spooler schreibt die Datei nach EndDoc, etwas Geduld.
        let mut data = Vec::new();
        for _ in 0..100 {
            data = std::fs::read(out).unwrap_or_default();
            if data.len() > 16 && data[data.len() - 16..].windows(5).any(|w| w == b"%%EOF") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        assert!(data.starts_with(b"%PDF"), "{} Bytes", data.len());
    }
}
