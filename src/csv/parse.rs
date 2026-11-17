//! Parser und Dialekt-Erkennung für CSV-Text.
//! Alle Positionen sind Zeichenindizes (wie im Rope), keine Bytes.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dialect {
    pub delim: char,
    pub header: bool,
}

/// Zeichenbereich eines Feldes, einschließlich der Anführungszeichen.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub start: usize,
    pub end: usize,
    pub quoted: bool,
}

/// `end` liegt vor dem '\n' (exklusiv).
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub start: usize,
    pub end: usize,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, Default)]
pub struct Parsed {
    /// Enthält auch die Kopfzeile (Index 0).
    pub records: Vec<Record>,
    pub ends_with_newline: bool,
    /// Index des Datensatzes, dessen Anführungszeichen nie geschlossen wird.
    pub unclosed_line: Option<usize>,
}

pub const DELIMS: [char; 4] = [',', ';', '\t', '|'];

/// Zerlegt den (bereits auf `\n` normalisierten) Text in Datensätze und Felder.
pub fn parse(text: &str, delim: char) -> Parsed {
    parse_chars(&text.chars().collect::<Vec<char>>(), delim)
}

/// Wie `parse`, für Text, der schon als Zeichen vorliegt.
pub fn parse_chars(chars: &[char], delim: char) -> Parsed {
    let n = chars.len();
    let mut out = Parsed::default();
    let mut i = 0;
    loop {
        let rec_start = i;
        let mut fields = Vec::new();
        loop {
            let fstart = i;
            let quoted = i < n && chars[i] == '"';
            if quoted {
                i += 1;
                loop {
                    if i >= n {
                        if out.unclosed_line.is_none() {
                            out.unclosed_line = Some(out.records.len());
                        }
                        break;
                    }
                    if chars[i] == '"' {
                        if i + 1 < n && chars[i + 1] == '"' {
                            i += 2;
                        } else {
                            i += 1;
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            // Rest des Feldes (bei Anführungszeichen: Text nach dem Schluss-")
            while i < n && chars[i] != delim && chars[i] != '\n' {
                i += 1;
            }
            fields.push(Field { start: fstart, end: i, quoted });
            if i < n && chars[i] == delim {
                i += 1;
            } else {
                break;
            }
        }
        out.records.push(Record { start: rec_start, end: i, fields });
        if i < n {
            i += 1; // '\n'
            if i == n {
                out.ends_with_newline = true;
                break;
            }
        } else {
            break;
        }
    }
    out
}

/// Wert eines Feldes ohne äußere Anführungszeichen; `""` wird zu `"`.
pub fn field_value(text_chars: &[char], f: &Field) -> String {
    let s = &text_chars[f.start..f.end];
    if !f.quoted {
        return s.iter().collect();
    }
    let mut out = String::new();
    let mut i = 1;
    while i < s.len() {
        if s[i] == '"' {
            if i + 1 < s.len() && s[i + 1] == '"' {
                out.push('"');
                i += 2;
                continue;
            }
            // Schluss-": Reste dahinter unverändert anhängen
            out.extend(&s[i + 1..]);
            return out;
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

/// Ist der Wert leer? Wie `field_value(..).is_empty()`, nur ohne den Wert zu bauen.
pub fn field_is_empty(text_chars: &[char], f: &Field) -> bool {
    let s = &text_chars[f.start..f.end];
    if !f.quoted {
        return s.is_empty();
    }
    // Nur das öffnende Anführungszeichen oder genau "": sonst folgt ein Zeichen, ein
    // verdoppeltes Anführungszeichen oder Text hinter dem schließenden.
    s.len() == 1 || (s.len() == 2 && s[1] == '"')
}

const DETECT_RECORDS: usize = 50;
const DETECT_CHARS: usize = 100_000;

/// Zellwert als Zahl, `,` oder `.` als Dezimalzeichen; ohne Ziffer keine Zahl.
pub fn parse_num(s: &str) -> Option<f64> {
    let t = s.trim().replace(',', ".");
    if t.chars().any(|c| c.is_ascii_digit()) { t.parse().ok() } else { None }
}

/// Erkennt Trennzeichen und Kopfzeile. `ext` ist die Dateiendung ohne Punkt.
pub fn detect(text: &str, ext: Option<&str>) -> Dialect {
    // Nur ein Anfang des Textes wird untersucht; ein abgeschnittener
    // letzter Datensatz zählt nicht mit.
    let cut = text.char_indices().nth(DETECT_CHARS).map(|(b, _)| b);
    let sample = &text[..cut.unwrap_or(text.len())];

    let delim = if ext.is_some_and(|e| e.eq_ignore_ascii_case("tsv") || e.eq_ignore_ascii_case("tab")) {
        '\t'
    } else {
        let mut best: Option<(char, usize)> = None;
        for &d in &DELIMS {
            let mut p = parse(sample, d);
            if cut.is_some() && p.records.len() > 1 {
                p.records.pop();
            }
            p.records.truncate(DETECT_RECORDS);
            let mut counts: std::collections::HashMap<usize, usize> = Default::default();
            for r in &p.records {
                *counts.entry(r.fields.len()).or_default() += 1;
            }
            let top = counts
                .iter()
                .filter(|(&cols, _)| cols > 1)
                .map(|(_, &freq)| freq)
                .max();
            if let Some(freq) = top {
                if best.map_or(true, |(_, b)| freq > b) {
                    best = Some((d, freq));
                }
            }
        }
        best.map_or(',', |(d, _)| d)
    };

    let p = parse(sample, delim);
    let chars: Vec<char> = sample.chars().collect();
    let header = p.records.first().is_some_and(|r| {
        r.fields.iter().all(|f| {
            let v = field_value(&chars, f);
            !v.trim().is_empty() && parse_num(&v).is_none()
        })
    });
    Dialect { delim, header }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vals(text: &str, delim: char, rec: usize) -> Vec<String> {
        let p = parse(text, delim);
        let chars: Vec<char> = text.chars().collect();
        p.records[rec].fields.iter().map(|f| field_value(&chars, f)).collect()
    }

    #[test]
    fn parses_quoted_fields_with_delims_and_doubled_quotes() {
        let t = "\"a,b\",\"c\"\"d\",e\n";
        let p = parse(t, ',');
        assert_eq!(p.records.len(), 1);
        assert_eq!(p.records[0].fields.len(), 3);
        assert_eq!(vals(t, ',', 0), vec!["a,b", "c\"d", "e"]);
        let q: Vec<bool> = p.records[0].fields.iter().map(|f| f.quoted).collect();
        assert_eq!(q, vec![true, true, false]);
        assert!(p.ends_with_newline);
    }

    #[test]
    fn multiline_field_is_one_record() {
        let t = "id;note\n1;\"x\ny\"\n2;z";
        let p = parse(t, ';');
        assert_eq!(p.records.len(), 3);
        assert_eq!(vals(t, ';', 1)[1], "x\ny");
        assert!(!p.ends_with_newline);
        assert_eq!(p.unclosed_line, None);
    }

    #[test]
    fn unclosed_quote_runs_to_end() {
        let t = "a,\"b\nc";
        let p = parse(t, ',');
        assert_eq!(p.unclosed_line, Some(0));
        assert_eq!(p.records.len(), 1);
        assert_eq!(vals(t, ',', 0), vec!["a", "b\nc"]);
    }

    #[test]
    fn empty_text_is_one_empty_cell() {
        let p = parse("", ',');
        assert_eq!(p.records.len(), 1);
        assert_eq!(p.records[0].fields, vec![Field { start: 0, end: 0, quoted: false }]);
        assert!(!p.ends_with_newline);
    }

    #[test]
    fn blank_line_is_record_with_empty_field() {
        let p = parse("a\n\nb", ',');
        assert_eq!(p.records.len(), 3);
        assert_eq!(p.records[1].fields.len(), 1);
    }

    #[test]
    fn field_ranges_cover_record_exactly() {
        for t in ["ä,€,x\n1,\"ä,€\",3\n", "a,,\n,\n\n\"q\"\"\",ö", "ä€", "\"a\nb\",c"] {
            let d = ',';
            let p = parse(t, d);
            let chars: Vec<char> = t.chars().collect();
            for r in &p.records {
                assert_eq!(r.fields.first().unwrap().start, r.start);
                assert_eq!(r.fields.last().unwrap().end, r.end);
                for w in r.fields.windows(2) {
                    assert_eq!(w[1].start, w[0].end + 1);
                    assert_eq!(chars[w[0].end], d);
                }
                if r.end < chars.len() {
                    assert_eq!(chars[r.end], '\n');
                }
            }
        }
    }

    #[test]
    fn field_is_empty_matches_value() {
        for t in ["a,,\"\",\"\"\"\",\"x\"y,\"\"z,\"", "\"", "\"\"", ",\"\"\"\"", "\"\"\n\"a"] {
            let p = parse(t, ',');
            let chars: Vec<char> = t.chars().collect();
            for f in p.records.iter().flat_map(|r| &r.fields) {
                assert_eq!(field_is_empty(&chars, f), field_value(&chars, f).is_empty(), "{t:?} {f:?}");
            }
        }
    }

    #[test]
    fn detects_delimiters() {
        for d in DELIMS {
            let t = format!("a{d}b{d}c\n1{d}2{d}3\n4{d}5{d}6\n");
            assert_eq!(detect(&t, None).delim, d, "Trennzeichen {d:?}");
        }
        assert_eq!(detect("a b\nc d\n", Some("tsv")).delim, '\t');
        assert_eq!(detect("a,b\nc,d\n", Some("tab")).delim, '\t', "auch .tab ist Tab-getrennt");
        assert_eq!(detect("a,b\nc,d\n", Some("TAB")).delim, '\t');
        assert_eq!(detect("hallo\nwelt\n", None).delim, ',');
    }

    #[test]
    fn detects_header() {
        assert!(detect("Name;Phone\nAnna;123", None).header);
        assert!(!detect("1;2\n3;4", None).header);
        assert!(!detect("Name;\nx;y", None).header);
    }
}
