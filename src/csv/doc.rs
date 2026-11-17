//! Anbindung der Tabelle ans Dokument: Dialekt, Parse-Cache, Änderungen als eine Gruppe.

use super::ops::Change;
use super::parse::{detect, field_value, parse, Dialect, Parsed};
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
    /// Spaltennamen aus der Kopfzeile für die Textansicht: (Revision, Dialekt, Namen).
    head: Option<(u64, Dialect, Arc<Vec<String>>)>,
}

/// So viele Zeichen am Anfang reichen für die Kopfzeile der Textansicht.
const HEAD_CHARS: usize = 8192;

impl DocCsv {
    pub fn new(dialect: Dialect) -> DocCsv {
        DocCsv { dialect, manual: false, cache: None, head: None }
    }

    /// Neu geparst wird nur bei geänderter Revision oder geändertem Dialekt.
    pub fn parsed(&mut self, doc_text: &Rope, rev: u64) -> (Arc<Parsed>, Arc<Vec<char>>) {
        if let Some((r, d, p, c)) = &self.cache {
            if *r == rev && *d == self.dialect {
                return (p.clone(), c.clone());
            }
        }
        let s = doc_text.to_string();
        let p = Arc::new(parse(&s, self.dialect.delim));
        let c = Arc::new(s.chars().collect::<Vec<char>>());
        self.cache = Some((rev, self.dialect, p.clone(), c.clone()));
        (p, c)
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
                    names = r
                        .fields
                        .iter()
                        .map(|f| field_value(&chars, f).split('\n').next().unwrap_or("").to_string())
                        .collect();
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

/// Wendet Änderungen als eine Undo-Gruppe an. `false`: nichts zu tun.
pub fn apply_changes(doc: &mut Document, mut changes: Vec<Change>, before: &SelSet, caret_after: usize) -> bool {
    if changes.is_empty() {
        return false;
    }
    // Von hinten nach vorn, damit die Positionen vorderer Änderungen gültig bleiben.
    changes.sort_by(|a, b| b.start.cmp(&a.start));
    doc.begin(Kind::Other, before);
    for c in &changes {
        doc.splice(c.start, c.end, &c.text);
    }
    doc.commit(&SelSet::single(caret_after));
    true
}

/// Endung csv/tsv/tab oder Sprache CSV/TSV.
pub fn is_table_file(path: Option<&Path>, language: Option<&str>) -> bool {
    let ext_ok = path
        .and_then(|p| p.extension())
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| matches!(e.as_str(), "csv" | "tsv" | "tab"));
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
    let ext = doc.path.as_ref().and_then(|p| p.extension()).map(|e| e.to_string_lossy().to_lowercase());
    if doc.large || !is_table_file(doc.path.as_deref(), doc.hl.language.as_deref()) {
        doc.csv = None;
        return;
    }
    if doc.csv.as_ref().is_some_and(|c| c.manual) {
        return;
    }
    let d = detect(&doc.full_text(), ext.as_deref());
    doc.csv = Some(DocCsv::new(d));
}
