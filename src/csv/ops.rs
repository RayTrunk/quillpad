//! Tabellenaktionen als minimale Textersetzungen. Nicht angefasste Felder
//! bleiben dadurch Byte für Byte erhalten. Positionen sind Zeichenindizes.

use super::parse::{field_value, Dialect, Field, Parsed};

/// Zeichenbereich im alten Text, der durch `text` ersetzt wird.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Reihenfolge, in der Änderungen angewandt werden: von hinten nach vorn, damit die
/// Positionen vorderer Änderungen gültig bleiben. Bei gleichem `start` kommt die spätere
/// Änderung der Liste zuerst dran, die frühere landet so im Ergebnis weiter vorn.
pub fn back_to_front(changes: &[Change]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..changes.len()).collect();
    order.sort_by(|&a, &b| changes[b].start.cmp(&changes[a].start).then(b.cmp(&a)));
    order
}

/// Wendet nicht überlappende Änderungen an, in derselben Reihenfolge wie das Dokument.
#[cfg(test)]
pub fn apply(text: &str, changes: &[Change]) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    for i in back_to_front(changes) {
        let c = &changes[i];
        let end = c.end.min(chars.len());
        let start = c.start.min(end);
        chars.splice(start..end, c.text.chars());
    }
    chars.into_iter().collect()
}

/// Feldwert als CSV-Text; quotet nur, wenn nötig oder erzwungen.
pub fn encode(value: &str, delim: char, force_quote: bool) -> String {
    let needs = force_quote
        || value.chars().any(|c| c == delim || c == '"' || c == '\r' || c == '\n')
        || value.starts_with(' ')
        || value.ends_with(' ');
    if needs {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Wahr, wenn mindestens 90 % der nicht-leeren Felder in Anführungszeichen stehen.
pub fn file_quotes(text: &str, p: &Parsed) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let (mut quoted, mut total) = (0usize, 0usize);
    for f in p.records.iter().flat_map(|r| &r.fields) {
        if field_value(&chars, f).is_empty() {
            continue;
        }
        total += 1;
        if f.quoted {
            quoted += 1;
        }
    }
    total > 0 && quoted * 10 >= total * 9
}

/// Spaltenzahl für neue Zeilen: Kopfzeile, sonst häufigste Feldanzahl.
fn col_count(p: &Parsed, d: Dialect) -> usize {
    if d.header {
        if let Some(r) = p.records.first() {
            return r.fields.len();
        }
    }
    let mut counts: std::collections::HashMap<usize, usize> = Default::default();
    for r in &p.records {
        *counts.entry(r.fields.len()).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|&(cols, freq)| (freq, cols))
        .map_or(1, |(cols, _)| cols)
}

fn slice(chars: &[char], start: usize, end: usize) -> String {
    chars[start..end].iter().collect()
}

fn new_value(value: &str, delim: char, fq: bool) -> String {
    encode(value, delim, fq && !value.is_empty())
}

fn field_change(chars: &[char], f: &Field, delim: char, value: &str) -> Option<Change> {
    if field_value(chars, f) == value {
        return None;
    }
    Some(Change { start: f.start, end: f.end, text: encode(value, delim, f.quoted) })
}

/// Leert viele Zellen auf einmal; der Text wird nur einmal als Zeichen gebraucht.
/// Fehlende Felder bleiben fehlend, schon leere Felder ergeben keine Änderung.
pub fn clear_cells(chars: &[char], p: &Parsed, delim: char, cells: &[(usize, usize)]) -> Vec<Change> {
    cells
        .iter()
        .filter_map(|&(rec, col)| p.records.get(rec)?.fields.get(col))
        .filter_map(|f| field_change(chars, f, delim, ""))
        .collect()
}

pub fn set_cell(text: &str, p: &Parsed, d: Dialect, rec: usize, col: usize, value: &str) -> Vec<Change> {
    let Some(r) = p.records.get(rec) else { return vec![] };
    let chars: Vec<char> = text.chars().collect();
    if let Some(f) = r.fields.get(col) {
        return field_change(&chars, f, d.delim, value).into_iter().collect();
    }
    if value.is_empty() {
        return vec![];
    }
    let fq = file_quotes(text, p);
    let mut t = d.delim.to_string().repeat(col + 1 - r.fields.len());
    t.push_str(&new_value(value, d.delim, fq));
    vec![Change { start: r.end, end: r.end, text: t }]
}

/// Fügt einen leeren Datensatz ein, der danach Index `at` hat (`at` gleich
/// Datensatzzahl hängt ans Ende an).
pub fn insert_row(text: &str, p: &Parsed, d: Dialect, at: usize) -> Vec<Change> {
    let row = d.delim.to_string().repeat(col_count(p, d).saturating_sub(1));
    let n = text.chars().count();
    if let Some(r) = p.records.get(at) {
        return vec![Change { start: r.start, end: r.start, text: format!("{row}\n") }];
    }
    let t = if p.ends_with_newline { format!("{row}\n") } else { format!("\n{row}") };
    vec![Change { start: n, end: n, text: t }]
}

/// Löscht Datensätze nach rohem Index (Kopfzeile ist Index 0); den Schutz der
/// Kopfzeile übernimmt der Aufrufer.
pub fn delete_rows(text: &str, p: &Parsed, recs: &[usize]) -> Vec<Change> {
    let n = text.chars().count();
    let len = p.records.len();
    let mut del = vec![false; len];
    for &i in recs {
        if i < len {
            del[i] = true;
        }
    }
    let mut out = Vec::new();
    let mut a = 0;
    while a < len {
        if !del[a] {
            a += 1;
            continue;
        }
        let mut b = a;
        while b + 1 < len && del[b + 1] {
            b += 1;
        }
        let last = b + 1 == len;
        let (start, end) = if !last || p.ends_with_newline {
            (p.records[a].start, (p.records[b].end + 1).min(n))
        } else if a > 0 {
            // Ohne abschließendes '\n' muss der davor mit weg.
            (p.records[a - 1].end, p.records[b].end)
        } else {
            (p.records[a].start, p.records[b].end)
        };
        out.push(Change { start, end, text: String::new() });
        a = b + 1;
    }
    out
}

/// Tauscht zwei Bereiche (a vor b) gegeneinander aus.
fn swap(chars: &[char], a: (usize, usize), b: (usize, usize)) -> Vec<Change> {
    vec![
        Change { start: a.0, end: a.1, text: slice(chars, b.0, b.1) },
        Change { start: b.0, end: b.1, text: slice(chars, a.0, a.1) },
    ]
}

/// Tauscht einen Datensatz mit dem Nachbarn (roher Index; Kopfzeilenschutz ist
/// Sache des Aufrufers).
pub fn move_row(text: &str, p: &Parsed, rec: usize, up: bool) -> Vec<Change> {
    let i = if up {
        match rec.checked_sub(1) {
            Some(i) => i,
            None => return vec![],
        }
    } else {
        rec
    };
    if rec >= p.records.len() || i + 1 >= p.records.len() {
        return vec![];
    }
    let chars: Vec<char> = text.chars().collect();
    let (a, b) = (&p.records[i], &p.records[i + 1]);
    swap(&chars, (a.start, a.end), (b.start, b.end))
}

pub fn insert_col(_text: &str, p: &Parsed, d: Dialect, at: usize) -> Vec<Change> {
    let delim = d.delim.to_string();
    p.records
        .iter()
        .map(|r| match r.fields.get(at) {
            Some(f) => Change { start: f.start, end: f.start, text: delim.clone() },
            None => Change { start: r.end, end: r.end, text: delim.repeat(at + 1 - r.fields.len()) },
        })
        .collect()
}

/// Entfernt Spalte `col`; ein Datensatz mit nur einem Feld wird zur leeren Zeile.
pub fn delete_col(_text: &str, p: &Parsed, col: usize) -> Vec<Change> {
    let mut out = Vec::new();
    for r in &p.records {
        let Some(f) = r.fields.get(col) else { continue };
        let (start, end) = if r.fields.len() == 1 {
            (f.start, f.end)
        } else if col + 1 < r.fields.len() {
            (f.start, r.fields[col + 1].start)
        } else {
            (r.fields[col - 1].end, f.end)
        };
        out.push(Change { start, end, text: String::new() });
    }
    out
}

/// Tauscht Spalte `col` mit ihrem Nachbarn. Hat ein Datensatz nur die linke der
/// beiden Spalten, wandert deren Wert per Trennzeichen in die rechte Spalte.
pub fn move_col(text: &str, p: &Parsed, d: Dialect, col: usize, left: bool) -> Vec<Change> {
    let i = if left {
        match col.checked_sub(1) {
            Some(i) => i,
            None => return vec![],
        }
    } else {
        col
    };
    if !p.records.iter().any(|r| r.fields.len() > i + 1) {
        return vec![];
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    for r in &p.records {
        match (r.fields.get(i), r.fields.get(i + 1)) {
            (Some(a), Some(b)) => out.extend(swap(&chars, (a.start, a.end), (b.start, b.end))),
            (Some(a), None) if a.start < a.end => {
                out.push(Change { start: a.start, end: a.end, text: String::new() });
                let t = format!("{}{}", d.delim, slice(&chars, a.start, a.end));
                out.push(Change { start: r.end, end: r.end, text: t });
            }
            _ => {}
        }
    }
    out
}

/// `order` ist eine vollständige Permutation der Datensatzindizes: ab 1, wenn
/// `d.header` gesetzt ist (Kopfzeile bleibt), sonst ab 0.
pub fn apply_order(text: &str, p: &Parsed, d: Dialect, order: &[usize]) -> Vec<Change> {
    let first = usize::from(d.header);
    let len = p.records.len();
    if first + order.len() != len {
        return vec![];
    }
    let mut seen = vec![false; len];
    for &o in order {
        if o < first || o >= len || seen[o] {
            return vec![];
        }
        seen[o] = true;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    for (k, &src) in order.iter().enumerate() {
        let slot = first + k;
        if src != slot {
            let (s, t) = (&p.records[slot], &p.records[src]);
            out.push(Change { start: s.start, end: s.end, text: slice(&chars, t.start, t.end) });
        }
    }
    out
}

/// Fehlende Felder am Ende eines Datensatzes ergänzen; `tail` aufsteigend nach Spalte.
fn tail_change(r: &super::parse::Record, delim: char, fq: bool, tail: &[(usize, &String)]) -> Option<Change> {
    let &(maxc, _) = tail.last()?;
    let mut t = String::new();
    for c in r.fields.len()..=maxc {
        t.push(delim);
        if let Some((_, v)) = tail.iter().find(|(tc, _)| *tc == c) {
            t.push_str(&new_value(v, delim, fq));
        }
    }
    Some(Change { start: r.end, end: r.end, text: t })
}

/// Setzt viele Zellen (Datensatz, Spalte, Wert) auf einmal, z. B. eine Auswahl füllen oder
/// in eine sortierte Ansicht einfügen. Spalten jenseits der Spaltenzahl fallen weg (`true`).
pub fn set_cells(text: &str, p: &Parsed, d: Dialect, cells: &[(usize, usize, String)]) -> (Vec<Change>, bool) {
    let chars: Vec<char> = text.chars().collect();
    let ncols = col_count(p, d);
    let fq = file_quotes(text, p);
    let mut by_rec: std::collections::BTreeMap<usize, Vec<(usize, &String)>> = Default::default();
    let mut cut = false;
    for (rec, col, v) in cells {
        if *col >= ncols {
            cut = true;
        } else if *rec < p.records.len() {
            by_rec.entry(*rec).or_default().push((*col, v));
        }
    }
    let mut out = Vec::new();
    for (rec, mut vals) in by_rec {
        let r = &p.records[rec];
        vals.sort_by_key(|&(c, _)| c);
        vals.dedup_by_key(|&mut (c, _)| c);
        let mut tail = Vec::new();
        for (c, v) in vals {
            match r.fields.get(c) {
                Some(f) => out.extend(field_change(&chars, f, d.delim, v)),
                None => tail.push((c, v)),
            }
        }
        out.extend(tail_change(r, d.delim, fq, &tail));
    }
    (out, cut)
}

pub fn paste_block(
    text: &str,
    p: &Parsed,
    d: Dialect,
    rec: usize,
    col: usize,
    block: &[Vec<String>],
) -> (Vec<Change>, bool) {
    let chars: Vec<char> = text.chars().collect();
    let ncols = col_count(p, d);
    let fq = file_quotes(text, p);
    let mut out = Vec::new();
    let mut truncated = false;
    let mut new_rows: Vec<String> = Vec::new();
    for (ri, row) in block.iter().enumerate() {
        let target = rec + ri;
        let cells = row.iter().enumerate().map(|(j, v)| (col + j, v));
        if let Some(r) = p.records.get(target) {
            let mut tail: Vec<(usize, &String)> = Vec::new();
            for (c, v) in cells {
                if c >= ncols {
                    truncated = true;
                } else if let Some(f) = r.fields.get(c) {
                    out.extend(field_change(&chars, f, d.delim, v));
                } else {
                    tail.push((c, v));
                }
            }
            out.extend(tail_change(r, d.delim, fq, &tail));
        } else {
            let mut vals = vec![String::new(); ncols];
            for (c, v) in cells {
                if c < ncols {
                    vals[c] = new_value(v, d.delim, fq);
                } else {
                    truncated = true;
                }
            }
            new_rows.push(vals.join(&d.delim.to_string()));
        }
    }
    if !new_rows.is_empty() {
        let n = chars.len();
        let t = if p.ends_with_newline {
            format!("{}\n", new_rows.join("\n"))
        } else {
            format!("\n{}", new_rows.join("\n"))
        };
        // Zuletzt, damit es bei gleichem Start hinter einer Zelländerung landet.
        out.push(Change { start: n, end: n, text: t });
    }
    (out, truncated)
}

/// Zellen für die Zwischenablage (Excel-kompatibel, Tab/Zeilenumbruch).
pub fn to_tsv(values: &[Vec<String>]) -> String {
    values
        .iter()
        .map(|row| {
            row.iter()
                .map(|v| {
                    if v.contains(['\t', '"', '\n', '\r']) {
                        format!("\"{}\"", v.replace('"', "\"\""))
                    } else {
                        v.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn from_tsv(s: &str) -> Vec<Vec<String>> {
    let c: Vec<char> = s.chars().collect();
    let n = c.len();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut i = 0;
    loop {
        let mut row = Vec::new();
        loop {
            let mut v = String::new();
            if i < n && c[i] == '"' {
                i += 1;
                while i < n {
                    if c[i] == '"' {
                        if i + 1 < n && c[i + 1] == '"' {
                            v.push('"');
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    v.push(c[i]);
                    i += 1;
                }
            }
            while i < n && c[i] != '\t' && c[i] != '\n' && !(c[i] == '\r' && c.get(i + 1) == Some(&'\n')) {
                v.push(c[i]);
                i += 1;
            }
            row.push(v);
            if i < n && c[i] == '\t' {
                i += 1;
            } else {
                break;
            }
        }
        rows.push(row);
        if i < n {
            i += if c[i] == '\r' { 2 } else { 1 };
            if i >= n {
                break; // abschließender Zeilenumbruch ergibt keine Leerzeile
            }
        } else {
            break;
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csv::parse::parse;

    #[test]
    fn clear_cells_keeps_quote_style_and_skips_missing() {
        let t = "a;\"b\";c\nx;;z\nonly\n";
        let p = parse(t, ';');
        let chars: Vec<char> = t.chars().collect();
        let ch = clear_cells(&chars, &p, ';', &[(0, 0), (0, 1), (1, 1), (2, 0), (2, 2)]);
        assert_eq!(apply(t, &ch), ";\"\";c\nx;;z\n\n");
    }

    #[test]
    fn set_cells_fills_existing_and_missing_fields() {
        let t = "a,b,c
1,2,3
4
";
        let p = parse(t, ',');
        let v = |r, c| (r, c, "x".to_string());
        let (ch, cut) = set_cells(t, &p, d(','), &[v(1, 0), v(1, 1), v(2, 0), v(2, 2), v(2, 5)]);
        assert!(cut, "Spalte 5 gibt es nicht");
        assert_eq!(apply(t, &ch), "a,b,c
x,x,3
x,,x
");
        let (ch, _) = set_cells(t, &p, d(','), &[(1, 0, "1".into())]);
        assert!(ch.is_empty(), "gleicher Wert: keine Änderung");
    }

    fn d(delim: char) -> Dialect {
        Dialect { delim, header: true }
    }

    fn set(text: &str, delim: char, rec: usize, col: usize, v: &str) -> String {
        let p = parse(text, delim);
        apply(text, &set_cell(text, &p, d(delim), rec, col, v))
    }

    #[test]
    fn set_cell_touches_only_that_field() {
        let t = "\"a\",\"b\",\"c\"\n\"d\",\"e\",\"f\"\n";
        assert_eq!(set(t, ',', 1, 1, "x"), "\"a\",\"b\",\"c\"\n\"d\",\"x\",\"f\"\n");
    }

    #[test]
    fn set_cell_quotes_when_needed() {
        assert_eq!(set("a;b\n", ';', 0, 1, "x;y"), "a;\"x;y\"\n");
        assert_eq!(set("a;b\n", ';', 0, 1, "say \"hi\""), "a;\"say \"\"hi\"\"\"\n");
    }

    #[test]
    fn set_cell_same_value_is_noop() {
        let t = "a,\"b\"\n";
        let p = parse(t, ',');
        assert!(set_cell(t, &p, d(','), 0, 0, "a").is_empty());
        assert!(set_cell(t, &p, d(','), 0, 1, "b").is_empty());
    }

    #[test]
    fn set_cell_beyond_ragged_row_appends_delims() {
        assert_eq!(set("a,b,c\nd\n", ',', 1, 2, "z"), "a,b,c\nd,,z\n");
    }

    #[test]
    fn insert_row_at_end_without_trailing_newline() {
        let t = "h1,h2\na,b";
        let p = parse(t, ',');
        assert_eq!(apply(t, &insert_row(t, &p, d(','), 2)), "h1,h2\na,b\n,");
        let t = "\"h1\",\"h2\"\n\"a\",\"b\"";
        let p = parse(t, ',');
        assert!(file_quotes(t, &p));
        assert_eq!(apply(t, &insert_row(t, &p, d(','), 2)), "\"h1\",\"h2\"\n\"a\",\"b\"\n,");
        let t = "h1,h2\na,b\n";
        let p = parse(t, ',');
        assert_eq!(apply(t, &insert_row(t, &p, d(','), 1)), "h1,h2\n,\na,b\n");
        assert_eq!(apply(t, &insert_row(t, &p, d(','), 2)), "h1,h2\na,b\n,\n");
    }

    #[test]
    fn delete_last_row_without_trailing_newline() {
        let t = "h\na\nb";
        let p = parse(t, ',');
        assert_eq!(apply(t, &delete_rows(t, &p, &[2])), "h\na");
        assert_eq!(apply(t, &delete_rows(t, &p, &[1, 2])), "h");
        let t = "h\na\nb\n";
        let p = parse(t, ',');
        assert_eq!(apply(t, &delete_rows(t, &p, &[1])), "h\nb\n");
        assert_eq!(apply(t, &delete_rows(t, &p, &[2, 0])), "a\n");
    }

    #[test]
    fn move_multiline_record() {
        let t = "id;n\n1;\"x\ny\"\n2;z\n";
        let p = parse(t, ';');
        assert_eq!(apply(t, &move_row(t, &p, 1, false)), "id;n\n2;z\n1;\"x\ny\"\n");
        assert_eq!(apply(t, &move_row(t, &p, 2, true)), "id;n\n2;z\n1;\"x\ny\"\n");
    }

    #[test]
    fn insert_delete_move_col() {
        let t = "a,b\nc,d\n";
        let p = parse(t, ',');
        assert_eq!(apply(t, &insert_col(t, &p, d(','), 1)), "a,,b\nc,,d\n");
        assert_eq!(apply(t, &insert_col(t, &p, d(','), 2)), "a,b,\nc,d,\n");
        assert_eq!(apply(t, &delete_col(t, &p, 0)), "b\nd\n");
        assert_eq!(apply(t, &delete_col(t, &p, 1)), "a\nc\n");
        assert_eq!(apply(t, &move_col(t, &p, d(','), 1, true)), "b,a\nd,c\n");
        assert_eq!(apply(t, &move_col(t, &p, d(','), 0, false)), "b,a\nd,c\n");
        // Ungleich lange Zeilen: der Wert wandert mit in die andere Spalte
        let t = "a,b\nc\n";
        let p = parse(t, ',');
        assert_eq!(apply(t, &move_col(t, &p, d(','), 0, false)), "b,a\n,c\n");
        assert_eq!(apply(t, &move_col(t, &p, d(','), 1, true)), "b,a\n,c\n");
    }

    #[test]
    fn apply_order_keeps_records_bytewise_and_trailing_state() {
        let t = "h\n\"b\"\na\nc";
        let p = parse(t, ',');
        assert_eq!(apply(t, &apply_order(t, &p, d(','), &[2, 1, 3])), "h\na\n\"b\"\nc");
        let t = "h\n\"b\"\na\nc\n";
        let p = parse(t, ',');
        assert_eq!(apply(t, &apply_order(t, &p, d(','), &[3, 2, 1])), "h\nc\na\n\"b\"\n");
    }

    #[test]
    fn paste_block_appends_rows_and_clips_cols() {
        let t = "a,b\n1,2\n";
        let p = parse(t, ',');
        let block = vec![
            vec!["x".to_string(), "y".to_string(), "z".to_string()],
            vec!["q".to_string(), "r".to_string(), "s".to_string()],
        ];
        let (ch, truncated) = paste_block(t, &p, d(','), 1, 1, &block);
        assert!(truncated);
        assert_eq!(apply(t, &ch), "a,b\n1,x\n,q\n");
        let (ch, truncated) = paste_block(t, &p, d(','), 0, 0, &[vec!["m".into(), "n".into()]]);
        assert!(!truncated);
        assert_eq!(apply(t, &ch), "m,n\n1,2\n");
    }

    #[test]
    fn tsv_roundtrip() {
        let v = vec![
            vec!["a\tb".to_string(), "say \"hi\"".to_string()],
            vec!["x\ny".to_string(), String::new(), "plain".to_string()],
        ];
        assert_eq!(from_tsv(&to_tsv(&v)), v);
        assert_eq!(from_tsv("a\tb\r\nc\td\r\n"), vec![vec!["a", "b"], vec!["c", "d"]]);
    }
}
