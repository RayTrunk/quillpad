//! PDF-Ausgabe: schreibt die fertig umbrochenen Seiten unverändert aus.
//! Jede Zeichenfolge steht mit absoluter Position im Inhalt, die Schrift ist als
//! Untermenge (Identity-H) eingebettet, Text bleibt kopier- und durchsuchbar.
use super::font::PrintFont;
use super::{Geom, Measure, Page, GRAY};
use pdf_writer::types::{FontFlags, SystemInfo, TextRenderingMode, UnicodeCmap};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};
use std::collections::{BTreeMap, BTreeSet};

/// Schlüssel einer Schrift im PDF: (Ersatzschrift-Nr., fett).
type Key = (u8, bool);

/// Fertig vorbereitete Schrift: neue Glyph-IDs je Zeichen und ihre Breiten.
struct Prepared {
    /// Zeichen -> Glyph-ID im eingebetteten Font (neu nummeriert, falls Untermenge).
    ids: BTreeMap<char, u16>,
    widths: BTreeMap<u16, f32>,
    font_file: Vec<u8>,
}

/// Fett ohne Fettschnitt wird durch Füllen und Umranden nachgebildet.
fn synthetic_bold(font: &PrintFont, key: Key) -> bool {
    key.1 && (key.0 != 0 || font.bold.is_none())
}

fn prepare(font: &PrintFont, key: Key, chars: &BTreeSet<char>) -> Prepared {
    let face = font.face_data(key.0, key.1);
    let old: BTreeMap<char, u16> = chars.iter().map(|&c| (c, font.glyph_id(key.0, key.1, c))).collect();
    let gids: Vec<u16> = old.values().copied().collect();
    let mapper = subsetter::GlyphRemapper::new_from_glyphs(&gids);
    // Schlägt die Untermenge fehl (z. B. exotisches Format), kommt die ganze Schrift mit Originalnummern.
    let (ids, font_file) = match subsetter::subset(&face.data, face.index, &mapper) {
        Ok(data) => (old.iter().map(|(&c, &g)| (c, mapper.get(g).unwrap_or(0))).collect(), data),
        Err(_) => (old, face.data.to_vec()),
    };
    let mut widths = BTreeMap::new();
    for (&c, &id) in &ids {
        widths.entry(id).or_insert(font.advance(c, key.1) * 1000.0);
    }
    Prepared { ids, widths, font_file }
}

pub fn write_pdf(pages: &[Page], g: &Geom, font: &PrintFont, title: &str) -> Result<Vec<u8>, String> {
    // Zeichen je verwendeter Schrift sammeln.
    let mut used: BTreeMap<Key, BTreeSet<char>> = BTreeMap::new();
    for r in pages.iter().flat_map(|p| &p.runs) {
        if (r.face as usize) > font.fallbacks.len() {
            return Err(format!("unbekannte Ersatzschrift {}", r.face));
        }
        used.entry((r.face, r.bold)).or_default().extend(r.text.chars());
    }
    let prepared: BTreeMap<Key, Prepared> = used.iter().map(|(&k, cs)| (k, prepare(font, k, cs))).collect();

    let mut next = 1;
    let mut new_ref = || {
        let r = Ref::new(next);
        next += 1;
        r
    };
    let (catalog, tree, info) = (new_ref(), new_ref(), new_ref());
    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(tree);
    pdf.document_info(info).title(TextStr(title));

    // Pro Schrift: Type0, CIDFont, Descriptor, Fontdatei, ToUnicode.
    let mut names: BTreeMap<Key, (String, Ref)> = BTreeMap::new();
    for (n, (&key, p)) in prepared.iter().enumerate() {
        let (type0, cid, desc, file, uni) = (new_ref(), new_ref(), new_ref(), new_ref(), new_ref());
        // Sechs Großbuchstaben als Untermengen-Kennung, je Schrift verschieden.
        let tag: String = (0..6).map(|i| char::from(b'A' + ((n / 26usize.pow(i)) % 26) as u8)).collect();
        let base = format!("{tag}+QuillFont{n}");
        let base_name = Name(base.as_bytes());
        let info_cid = SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"Identity"), supplement: 0 };

        pdf.type0_font(type0)
            .base_font(base_name)
            .encoding_predefined(Name(b"Identity-H"))
            .descendant_font(cid)
            .to_unicode(uni);
        let mut c = pdf.cid_font(cid);
        c.subtype(pdf_writer::types::CidFontType::Type2)
            .base_font(base_name)
            .system_info(info_cid)
            .font_descriptor(desc)
            .default_width(0.0)
            .cid_to_gid_map_predefined(Name(b"Identity"));
        let max = p.widths.keys().next_back().copied().unwrap_or(0);
        let all: Vec<f32> = (0..=max).map(|i| p.widths.get(&i).copied().unwrap_or(0.0)).collect();
        c.widths().consecutive(0, all);
        c.finish();
        let asc = font.ascent() * 1000.0;
        pdf.font_descriptor(desc)
            .name(base_name)
            .flags(FontFlags::NON_SYMBOLIC)
            .bbox(Rect::new(-200.0, -300.0, 1200.0, asc.max(900.0)))
            .italic_angle(0.0)
            .ascent(asc)
            .descent(-250.0)
            .cap_height(asc * 0.8)
            .stem_v(80.0)
            .font_file2(file);
        pdf.stream(file, &p.font_file);
        let mut cmap = UnicodeCmap::new(Name(b"Adobe-Identity-UCS"), SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"UCS"), supplement: 0 });
        let mut seen = BTreeSet::new();
        for (&ch, &id) in &p.ids {
            if seen.insert(id) {
                cmap.pair(id, ch);
            }
        }
        pdf.cmap(uni, &cmap.finish());
        names.insert(key, (format!("F{n}"), type0));
    }

    let page_ids: Vec<Ref> = pages.iter().map(|_| new_ref()).collect();
    let content_ids: Vec<Ref> = pages.iter().map(|_| new_ref()).collect();
    pdf.pages(tree).kids(page_ids.iter().copied()).count(page_ids.len() as i32);
    let (r, gr, b) = (GRAY[0] as f32 / 255.0, GRAY[1] as f32 / 255.0, GRAY[2] as f32 / 255.0);
    for (i, page) in pages.iter().enumerate() {
        let mut c = Content::new();
        // Linien: gefüllte Rechtecke von 0,5 pt Höhe.
        c.set_fill_rgb(r, gr, b);
        for &[x0, x1, y] in &page.rules {
            c.rect(x0, g.h - y - 0.25, x1 - x0, 0.5).fill_nonzero();
        }
        for run in &page.runs {
            let key = (run.face, run.bold);
            let (Some(p), Some((name, _))) = (prepared.get(&key), names.get(&key)) else { continue };
            let col = run.color.map(|v| v as f32 / 255.0);
            let mut bytes = Vec::with_capacity(run.text.len() * 2);
            for ch in run.text.chars() {
                bytes.extend_from_slice(&p.ids.get(&ch).copied().unwrap_or(0).to_be_bytes());
            }
            c.save_state();
            c.set_fill_rgb(col[0], col[1], col[2]);
            if synthetic_bold(font, key) {
                c.set_stroke_rgb(col[0], col[1], col[2]).set_line_width(run.size / 30.0);
            }
            c.begin_text();
            if synthetic_bold(font, key) {
                c.set_text_rendering_mode(TextRenderingMode::FillStroke);
            }
            c.set_font(Name(name.as_bytes()), run.size).next_line(run.x, g.h - run.y).show(Str(&bytes));
            c.end_text();
            c.restore_state();
        }
        pdf.stream(content_ids[i], &c.finish());
        let mut pg = pdf.page(page_ids[i]);
        pg.parent(tree).media_box(Rect::new(0.0, 0.0, g.w, g.h)).contents(content_ids[i]);
        let mut res = pg.resources();
        let mut fonts = res.fonts();
        for (name, type0) in names.values() {
            fonts.pair(Name(name.as_bytes()), *type0);
        }
    }
    Ok(pdf.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::print::{layout::paginate, LayoutOpts, SrcLine, MM};

    #[test]
    fn pdf_has_pages_and_extractable_text() {
        let Some(f) = PrintFont::load("Courier New") else {
            eprintln!("keine Schrift gefunden, Test übersprungen");
            return;
        };
        let g = Geom { w: 595.0, h: 842.0, left: 15.0 * MM, top: 15.0 * MM, right: 15.0 * MM, bottom: 15.0 * MM };
        let o = LayoutOpts {
            size: 10.0, line_numbers: true, header_footer: true, tab_size: 4,
            header_left: "datei.rs".into(), header_right: "heute".into(),
            footer: "{page}/{total}".into(),
        };
        let mut texts = vec!["fn main() {}".to_string(), "Grüße 一".to_string(), "Smiley \u{1F600} Ende".to_string()];
        texts.extend((0..80).map(|i| format!("Füllung {i}")));
        let src: Vec<SrcLine> = texts
            .iter()
            .enumerate()
            .map(|(i, t)| SrcLine { number: i + 1, text: t.clone(), spans: Vec::new() })
            .collect();
        let pages = paginate(&src, &o, &g, &f);
        assert!(pages.len() >= 2);
        let pdf = write_pdf(&pages, &g, &f, "Titel").expect("PDF");
        assert!(pdf.starts_with(b"%PDF"));
        let doc = lopdf::Document::load_mem(&pdf).expect("lesbar");
        assert_eq!(doc.get_pages().len(), pages.len());
        let text = doc.extract_text(&[1]).expect("Text");
        assert!(text.contains("fn main"), "{text:?}");
        assert!(text.contains("Grüße"), "{text:?}");
    }
}
