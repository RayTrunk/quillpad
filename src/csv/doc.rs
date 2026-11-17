//! Anbindung der Tabelle ans Dokument: Dialekt, Parse-Cache, Änderungen als eine Gruppe.

use super::ops::{back_to_front, file_quotes, Change};
use super::parse::{detect, field_value, parse, parse_chars, Dialect, Field, Parsed};
use crate::doc::{Document, Kind, SelSet};
use ropey::Rope;
use std::path::Path;
use std::sync::Arc;

type Cache = (u64, Dialect, Arc<Parsed>, Arc<Vec<char>>);

pub struct DocCsv {
    pub dialect: Dialect,
    /// Vom Nutzer gewählt: wird beim Neuladen nicht mehr überschrieben.
    pub manual: bool,
    cache: Option<Cache>,
    /// Stil der Datei (`file_quotes`) für (Revision, Dialekt); erst bei Bedarf berechnet.
    quotes: Option<(u64, Dialect, bool)>,
    /// Spaltennamen aus der Kopfzeile für die Textansicht: (Revision, Dialekt, Namen).
    head: Option<(u64, Dialect, Arc<Vec<String>>)>,
}

/// So viele Zeichen am Anfang reichen für die Kopfzeile der Textansicht.
const HEAD_CHARS: usize = 8192;

impl DocCsv {
    pub fn new(dialect: Dialect) -> DocCsv {
        DocCsv { dialect, manual: false, cache: None, quotes: None, head: None }
    }

    /// Neu geparst wird nur bei geänderter Revision oder geändertem Dialekt.
    pub fn parsed(&mut self, doc_text: &Rope, rev: u64) -> (Arc<Parsed>, Arc<Vec<char>>) {
        if let Some((r, d, p, c)) = &self.cache {
            if *r == rev && *d == self.dialect {
                return (p.clone(), c.clone());
            }
        }
        // Zeichen einmal sammeln: der Parser und alle Tabellenaktionen arbeiten darauf.
        let mut c = Vec::with_capacity(doc_text.len_chars());
        for chunk in doc_text.chunks() {
            c.extend(chunk.chars());
        }
        let c = Arc::new(c);
        let p = Arc::new(parse_chars(&c, self.dialect.delim));
        self.cache = Some((rev, self.dialect, p.clone(), c.clone()));
        (p, c)
    }

    /// Revision des zwischengespeicherten Parse-Ergebnisses (für Tests).
    #[cfg(test)]
    pub fn parsed_rev(&self) -> Option<u64> {
        self.cache.as_ref().map(|c| c.0)
    }

    /// Stehen die Felder der Datei überwiegend in Anführungszeichen? Je Revision und Dialekt
    /// einmal berechnet.
    pub fn file_quotes(&mut self, doc_text: &Rope, rev: u64) -> bool {
        if let Some((r, d, q)) = self.quotes {
            if r == rev && d == self.dialect {
                return q;
            }
        }
        let (p, c) = self.parsed(doc_text, rev);
        let q = file_quotes(&c, &p);
        self.quotes = Some((rev, self.dialect, q));
        q
    }

    /// Spaltennamen aus der Kopfzeile (erste Textzeile je Wert, leer = keiner), ohne den
    /// ganzen Text zu parsen; leer ohne Kopfzeile oder wenn sie über den Anfang hinausreicht.
    /// Zwischengespeichert je Revision und Dialekt.
    pub fn header_names(&mut self, text: &Rope, rev: u64) -> Arc<Vec<String>> {
        if let Some((r, d, names)) = &self.head {
            if *r == rev && *d == self.dialect {
                return names.clone();
            }
        }
        let mut names = Vec::new();
        if self.dialect.header {
            let n = text.len_chars().min(HEAD_CHARS);
            let head = text.slice(..n).to_string();
            let p = parse(&head, self.dialect.delim);
            if n == text.len_chars() || p.records.len() > 1 {
                let chars: Vec<char> = head.chars().collect();
                if let Some(r) = p.records.first() {
                    names = r.fields.iter().map(|f| head_label(&chars, f)).collect();
                }
            }
        }
        let names = Arc::new(names);
        self.head = Some((rev, self.dialect, names.clone()));
        names
    }

    pub fn set_dialect(&mut self, d: Dialect) {
        self.dialect = d;
        self.manual = true;
    }
}

/// Spaltenname aus einem Feld der Kopfzeile: die erste Textzeile des Werts.
pub fn head_label(chars: &[char], f: &Field) -> String {
    let v = field_value(chars, f);
    match v.find('\n') {
        Some(i) => v[..i].to_string(),
        None => v,
    }
}

/// Wendet Änderungen als eine Undo-Gruppe an. `false`: nichts zu tun.
pub fn apply_changes(doc: &mut Document, changes: Vec<Change>, before: &SelSet, caret_after: usize) -> bool {
    if changes.is_empty() {
        return false;
    }
    // Eine offene Gruppe (z. B. vom Tippen) würde die Änderungen sonst still aufnehmen.
    debug_assert!(!doc.group_open(), "apply_changes mit offener Undo-Gruppe");
    doc.begin(Kind::Other, before);
    for i in back_to_front(&changes) {
        let c = &changes[i];
        doc.splice(c.start, c.end, &c.text);
    }
    doc.commit(&SelSet::single(caret_after));
    true
}

/// Dateiendung ohne Punkt, klein geschrieben.
fn ext_of(path: Option<&Path>) -> Option<String> {
    path.and_then(|p| p.extension()).map(|e| e.to_string_lossy().to_lowercase())
}

/// Dialekt eines Dokuments aus Text und Dateiendung erkennen.
pub fn detect_doc(doc: &Document) -> Dialect {
    detect(&doc.full_text(), ext_of(doc.path.as_deref()).as_deref())
}

/// Endung csv/tsv/tab oder Sprache CSV/TSV.
pub fn is_table_file(path: Option<&Path>, language: Option<&str>) -> bool {
    let ext_ok = ext_of(path).is_some_and(|e| matches!(e.as_str(), "csv" | "tsv" | "tab"));
    ext_ok || language.is_some_and(|l| l.eq_ignore_ascii_case("csv") || l.eq_ignore_ascii_case("tsv"))
}

/// Trennzeichen, mit dem die Textansicht spaltenweise färbt: nur bei Tabellendateien
/// (Endung oder Sprache CSV/TSV) oder normalem Text. Ein als Tabelle gezeigtes Rust- oder
/// JSON-Dokument behält im Text seine Sprachfarben.
pub fn hl_delim(doc: &Document) -> Option<char> {
    let c = doc.csv.as_ref()?;
    let lang = doc.hl.language.as_deref();
    (lang.is_none() || is_table_file(doc.path.as_deref(), lang)).then_some(c.dialect.delim)
}

/// Feld, das `pos` enthält; hinter dem letzten Datensatz das letzte Feld.
pub fn cell_at(p: &Parsed, pos: usize) -> (usize, usize) {
    if p.records.is_empty() {
        return (0, 0);
    }
    let rec = match p.records.iter().position(|r| pos <= r.end) {
        Some(i) => i,
        None => p.records.len() - 1,
    };
    let r = &p.records[rec];
    if r.fields.is_empty() {
        return (rec, 0);
    }
    let col = r.fields.iter().position(|f| pos <= f.end).unwrap_or(r.fields.len() - 1);
    (rec, col)
}

/// Anfang des Feldinhalts (hinter dem öffnenden Anführungszeichen).
pub fn cell_pos(p: &Parsed, rec: usize, col: usize) -> usize {
    match p.records.get(rec).and_then(|r| r.fields.get(col)) {
        Some(f) => f.start + usize::from(f.quoted),
        None => p.records.get(rec).map_or(0, |r| r.end),
    }
}

/// Setzt `doc.csv` beim Öffnen/Neuladen: Erkennung nur ohne manuelle Wahl.
pub fn setup(doc: &mut Document) {
    if doc.large || !is_table_file(doc.path.as_deref(), doc.hl.language.as_deref()) {
        doc.csv = None;
        return;
    }
    if doc.csv.as_ref().is_some_and(|c| c.manual) {
        return;
    }
    doc.csv = Some(DocCsv::new(detect_doc(doc)));
}
