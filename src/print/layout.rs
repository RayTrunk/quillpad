use super::*;

type Attr = ([u8; 3], bool);
const BLACK: [u8; 3] = [0, 0, 0];
/// Toleranz gegen Rundungsfehler beim Aufsummieren der Zeichenbreiten.
const EPS: f32 = 0.01;

/// Sammelt Zeichen zu `Run`s; ein neuer Run beginnt bei Farbe, Fett oder Schrift.
struct Runs<'a> {
    out: &'a mut Vec<Run>,
    y: f32,
    size: f32,
    cur: Option<Run>,
}

impl<'a> Runs<'a> {
    fn new(out: &'a mut Vec<Run>, y: f32, size: f32) -> Self {
        Runs { out, y, size, cur: None }
    }

    fn push(&mut self, c: char, x: f32, (color, bold): Attr, face: u8) {
        if let Some(r) = &mut self.cur {
            if r.color == color && r.bold == bold && r.face == face {
                r.text.push(c);
                return;
            }
        }
        self.flush();
        self.cur = Some(Run { x, y: self.y, text: c.to_string(), color, bold, face, size: self.size });
    }

    /// Beendet den laufenden Run (Umbruch oder Tabulator).
    fn flush(&mut self) {
        if let Some(r) = self.cur.take() {
            self.out.push(r);
        }
    }
}

fn text_width(t: &str, size: f32, m: &dyn Measure) -> f32 {
    t.chars().map(|c| m.advance(c, false) * size).sum()
}

/// Zeichnet einfachen (grauen) Text ab `x`.
fn put(out: &mut Vec<Run>, t: &str, x: f32, y: f32, size: f32, m: &dyn Measure) {
    let mut r = Runs::new(out, y, size);
    let mut cx = x;
    for c in t.chars() {
        r.push(c, cx, (GRAY, false), m.face(c));
        cx += m.advance(c, false) * size;
    }
    r.flush();
}

/// Farbe und Fett je Zeichen; fehlende Abschnitte gelten als Schwarz.
fn attrs(l: &SrcLine, n: usize) -> Vec<Attr> {
    let mut v: Vec<Attr> = Vec::with_capacity(n);
    for s in &l.spans {
        let c = [s.color.r(), s.color.g(), s.color.b()];
        let k = (s.chars as usize).min(n - v.len());
        v.extend(std::iter::repeat((c, s.bold)).take(k));
    }
    v.resize(n, (BLACK, false));
    v
}

/// Zerlegt eine Zeile in Anzeigezeilen (Bereiche in `chars`). Bricht am letzten
/// Leerzeichen, das noch passt, sonst hart; mindestens ein Zeichen pro Zeile.
fn wrap(chars: &[char], at: &[Attr], avail: f32, tab: f32, size: f32, m: &dyn Measure) -> Vec<(usize, usize)> {
    let mut rows = Vec::new();
    let mut start = 0;
    loop {
        let (mut x, mut space, mut i) = (0.0f32, None, start);
        while i < chars.len() {
            let c = chars[i];
            let nx = if c == '\t' { (x / tab).floor() * tab + tab } else { x + m.advance(c, at[i].1) * size };
            if nx > avail + EPS && i > start {
                break;
            }
            if c == ' ' {
                space = Some(i);
            }
            x = nx;
            i += 1;
        }
        let end = match space {
            Some(s) if i < chars.len() => s + 1,
            _ => i,
        };
        rows.push((start, end));
        if end >= chars.len() {
            return rows;
        }
        start = end;
    }
}

pub fn paginate(src: &[SrcLine], o: &LayoutOpts, g: &Geom, m: &dyn Measure) -> Vec<Page> {
    let size = o.size;
    let lh = 1.2 * size;
    let asc = m.ascent() * size;
    let total_lines = (((g.h - g.top - g.bottom) / lh + EPS).floor() as usize).max(1);
    // Kopf: Zeile + Leerzeile; Fuß: letzte Zeile.
    let (first, cap) = if o.header_footer { (2, total_lines.saturating_sub(3).max(1)) } else { (0, total_lines) };
    let text_right = g.w - g.right;

    let digits = src.iter().map(|l| l.number).max().unwrap_or(1).to_string().len();
    let num_w = digits as f32 * m.advance('0', false) * size;
    let gutter = if o.line_numbers { num_w + 2.0 * m.advance(' ', false) * size } else { 0.0 };
    let col = g.left + gutter;
    let avail = (text_right - col).max(0.0);
    let tab = (o.tab_size.max(1) as f32 * m.advance(' ', false) * size).max(EPS);

    let mut pages = vec![Page::default()];
    let mut slot = 0;
    for l in src {
        let chars: Vec<char> = l.text.chars().collect();
        let at = attrs(l, chars.len());
        for (n, &(a, b)) in wrap(&chars, &at, avail, tab, size, m).iter().enumerate() {
            if slot == cap {
                pages.push(Page::default());
                slot = 0;
            }
            let y = g.top + (first + slot) as f32 * lh + asc;
            let page = pages.last_mut().expect("mindestens eine Seite");
            if n == 0 && o.line_numbers {
                let s = l.number.to_string();
                put(&mut page.runs, &s, g.left + num_w - text_width(&s, size, m), y, size, m);
            }
            let mut r = Runs::new(&mut page.runs, y, size);
            let mut x = 0.0f32;
            for i in a..b {
                let c = chars[i];
                if c == '\t' {
                    r.flush();
                    x = (x / tab).floor() * tab + tab;
                } else {
                    r.push(c, col + x, at[i], m.face(c));
                    x += m.advance(c, at[i].1) * size;
                }
            }
            r.flush();
            slot += 1;
        }
    }

    if o.header_footer {
        let total = pages.len();
        for (i, p) in pages.iter_mut().enumerate() {
            let y = g.top + asc;
            put(&mut p.runs, &o.header_left, g.left, y, size, m);
            let hr = text_width(&o.header_right, size, m);
            put(&mut p.runs, &o.header_right, text_right - hr, y, size, m);
            p.rules.push([g.left, text_right, g.top + lh]);
            let f = o.footer.replace("{page}", &(i + 1).to_string()).replace("{total}", &total.to_string());
            let fx = g.left + (text_right - g.left - text_width(&f, size, m)) / 2.0;
            put(&mut p.runs, &f, fx, g.top + (total_lines - 1) as f32 * lh + asc, size, m);
        }
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Color32;

    struct Fixed;
    impl Measure for Fixed {
        fn advance(&self, c: char, bold: bool) -> f32 {
            if ('\u{4E00}'..='\u{9FFF}').contains(&c) { 1.2 } else if bold { 0.7 } else { 0.6 }
        }
        fn face(&self, _c: char) -> u8 { 0 }
        fn ascent(&self) -> f32 { 0.8 }
    }

    fn a4() -> Geom {
        Geom { w: 595.0, h: 842.0, left: 15.0 * MM, top: 15.0 * MM, right: 15.0 * MM, bottom: 15.0 * MM }
    }

    fn opts() -> LayoutOpts {
        LayoutOpts {
            size: 10.0, line_numbers: false, header_footer: false, tab_size: 4,
            header_left: String::new(), header_right: String::new(),
            footer: "{page}/{total}".into(), mono: true,
        }
    }

    fn line(number: usize, text: &str) -> SrcLine {
        SrcLine { number, text: text.into(), spans: Vec::new() }
    }

    /// Geometrie mit genau `cols` Zeichen Textbreite hinter einer Nummernspalte von 3 Zeichen.
    fn narrow(cols: f32) -> Geom {
        Geom { w: 10.0 + 10.0 + 18.0 + cols * 6.0, h: 842.0, left: 10.0, top: 10.0, right: 10.0, bottom: 10.0 }
    }

    #[test]
    fn paginate_empty_document_gives_one_page() {
        let mut o = opts();
        o.header_footer = true;
        let pages = paginate(&[], &o, &a4(), &Fixed);
        assert_eq!(pages.len(), 1);
        assert!(pages[0].runs.iter().any(|r| r.text == "1/1"));
    }

    #[test]
    fn paginate_breaks_pages_by_line_height() {
        let src: Vec<SrcLine> = (0..200).map(|i| line(i + 1, &format!("L{i}"))).collect();
        let per = ((842.0 - 2.0 * 15.0 * MM) / 12.0).floor() as usize;
        let pages = paginate(&src, &opts(), &a4(), &Fixed);
        assert_eq!(pages.len(), (200 + per - 1) / per);
        assert_eq!(pages[1].runs[0].text, format!("L{per}"));
    }

    #[test]
    fn paginate_wraps_at_spaces_and_hard() {
        let mut o = opts();
        o.line_numbers = true;
        let src = [line(1, "aaaa bbbb cccc dddd eeee ffff"), line(2, &"x".repeat(50))];
        let pages = paginate(&src, &o, &narrow(20.0), &Fixed);
        let texts: Vec<&str> = pages[0].runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, ["1", "aaaa bbbb cccc dddd ", "eeee ffff", "2",
            &"x".repeat(20), &"x".repeat(20), &"x".repeat(10)]);
    }

    #[test]
    fn paginate_wraps_with_bold_width() {
        let g = narrow(20.0);
        let src = [SrcLine {
            number: 1,
            text: "x".repeat(50),
            spans: vec![crate::highlight::Span { chars: 50, color: Color32::BLACK, bold: true }],
        }];
        let runs = &paginate(&src, &opts(), &g, &Fixed)[0].runs;
        let lens: Vec<usize> = runs.iter().map(|r| r.text.chars().count()).collect();
        // Ohne Nummernspalte: 138 pt Platz, fett 7 pt je Zeichen (nicht fett wären es 23 Zeichen).
        assert_eq!(lens, [19, 19, 12]);
        assert!(runs.iter().all(|r| r.x + r.text.chars().count() as f32 * 7.0 <= g.w - g.right + 0.01));
    }

    #[test]
    fn paginate_expands_tabs() {
        let pages = paginate(&[line(1, "\tx")], &opts(), &a4(), &Fixed);
        let r = &pages[0].runs[0];
        assert_eq!(r.text, "x");
        assert!((r.x - (15.0 * MM + 24.0)).abs() < 0.01);
    }

    #[test]
    fn paginate_keeps_document_numbers() {
        let mut o = opts();
        o.line_numbers = true;
        let pages = paginate(&[line(41, "a"), line(42, "b")], &o, &a4(), &Fixed);
        let nums: Vec<&Run> = pages[0].runs.iter().filter(|r| r.color == GRAY).collect();
        assert_eq!((nums[0].text.as_str(), nums[1].text.as_str()), ("41", "42"));
        assert!((nums[0].x - nums[1].x).abs() < 0.01);
    }

    #[test]
    fn paginate_survives_one_huge_line() {
        let src = [line(1, &"{".repeat(200_000))];
        let t = std::time::Instant::now();
        let pages = paginate(&src, &opts(), &a4(), &Fixed);
        assert!(t.elapsed().as_secs_f32() < 2.0);
        let n: usize = pages.iter().flat_map(|p| &p.runs).map(|r| r.text.chars().count()).sum();
        assert_eq!(n, 200_000);
    }

    #[test]
    fn paginate_extreme_sizes_progress() {
        let mut o = opts();
        o.size = 24.0;
        let g = Geom { w: 420.0, h: 595.0, left: 40.0 * MM, top: 40.0 * MM, right: 40.0 * MM, bottom: 40.0 * MM };
        let pages = paginate(&[line(1, &"w".repeat(500))], &o, &g, &Fixed);
        assert!(!pages.is_empty());
        assert!(pages.iter().flat_map(|p| &p.runs).all(|r| !r.text.is_empty()));
        let n: usize = pages.iter().flat_map(|p| &p.runs).map(|r| r.text.chars().count()).sum();
        assert_eq!(n, 500);
    }

    #[test]
    fn paginate_uses_span_colors_and_measures_wide_chars() {
        let red = Color32::from_rgb(255, 0, 0);
        let blue = Color32::from_rgb(0, 0, 255);
        let src = [SrcLine {
            number: 1,
            text: "abcdefgh".into(),
            spans: vec![
                crate::highlight::Span { chars: 3, color: red, bold: false },
                crate::highlight::Span { chars: 5, color: blue, bold: true },
            ],
        }];
        let r = &paginate(&src, &opts(), &a4(), &Fixed)[0].runs;
        assert_eq!((r[0].text.as_str(), r[0].color, r[0].bold), ("abc", [255, 0, 0], false));
        assert_eq!((r[1].text.as_str(), r[1].color, r[1].bold), ("defgh", [0, 0, 255], true));
        let wide = SrcLine {
            number: 1,
            text: "一a".into(),
            spans: vec![
                crate::highlight::Span { chars: 1, color: red, bold: false },
                crate::highlight::Span { chars: 1, color: blue, bold: false },
            ],
        };
        let w = &paginate(&[wide], &opts(), &a4(), &Fixed)[0].runs;
        assert!((w[0].x + 1.2 * 10.0 - w[1].x).abs() < 0.01);
    }
}
