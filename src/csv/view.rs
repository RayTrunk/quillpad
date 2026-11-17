//! Ansichtszustand der Tabelle: Sortierung und Filter.
//! Ändert nie den Dokumenttext, sondern nur, welche Datensätze in
//! welcher Reihenfolge sichtbar sind.

use super::parse::{field_value, Dialect, Parsed};
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Clone, Debug, Default)]
pub struct RowView {
    pub sort: Option<(usize, SortDir)>,
    pub filter: String,
    pub filter_col: Option<usize>,
    /// Sichtbare Datensatzindizes (ohne Kopfzeile) in Anzeigereihenfolge.
    pub rows: Vec<usize>,
    built_rev: u64,
}

impl RowView {
    /// Sortiert und filtert neu; `rev` ist die Textrevision, auf die sich die Ansicht bezieht.
    pub fn rebuild(&mut self, text: &[char], p: &Parsed, d: Dialect, rev: u64) {
        let first = usize::from(d.header);
        let cell = |rec: usize, col: usize| -> String {
            p.records[rec].fields.get(col).map(|f| field_value(text, f)).unwrap_or_default()
        };
        let needle = self.filter.to_lowercase();
        let mut rows: Vec<usize> = (first..p.records.len())
            .filter(|&r| {
                if needle.is_empty() {
                    return true;
                }
                match self.filter_col {
                    Some(c) => cell(r, c).to_lowercase().contains(&needle),
                    None => p.records[r]
                        .fields
                        .iter()
                        .any(|f| field_value(text, f).to_lowercase().contains(&needle)),
                }
            })
            .collect();
        if let Some((col, dir)) = self.sort {
            // Schlüssel einmal berechnen; Numerik gilt für die ganze Spalte.
            let keys: std::collections::HashMap<usize, String> =
                rows.iter().map(|&r| (r, cell(r, col))).collect();
            let all: Vec<&str> = keys.values().map(String::as_str).collect();
            let numeric = column_is_numeric(&all);
            rows.sort_by(|a, b| {
                let (ka, kb) = (&keys[a], &keys[b]);
                match (ka.is_empty(), kb.is_empty()) {
                    (true, true) => Ordering::Equal,
                    (true, false) => Ordering::Greater,
                    (false, true) => Ordering::Less,
                    _ => {
                        let o = compare_col(ka, kb, numeric);
                        if dir == SortDir::Desc { o.reverse() } else { o }
                    }
                }
            });
        }
        self.rows = rows;
        self.built_rev = rev;
    }

    /// Eigene Zeile eingefügt: Indizes ab `new_rec` rücken auf, die neue Zeile steht bei `at_view`.
    pub fn after_own_insert(&mut self, at_view: usize, new_rec: usize, rev: u64) {
        for r in &mut self.rows {
            if *r >= new_rec {
                *r += 1;
            }
        }
        let at = at_view.min(self.rows.len());
        self.rows.insert(at, new_rec);
        self.built_rev = rev;
    }

    /// Eigene Zeilen gelöscht: sie fallen weg, die übrigen Indizes rücken nach.
    pub fn after_own_delete(&mut self, recs: &[usize], rev: u64) {
        // Sortiert nachschlagen: viele gelöschte Zeilen bleiben so schnell.
        let mut del = recs.to_vec();
        del.sort_unstable();
        del.dedup();
        self.rows.retain(|r| del.binary_search(r).is_err());
        for r in &mut self.rows {
            *r -= del.partition_point(|&d| d < *r);
        }
        self.built_rev = rev;
    }

    /// Eigene Änderung ohne Strukturänderung (Zelle).
    pub fn mark_own(&mut self, rev: u64) {
        self.built_rev = rev;
    }

    /// Fremde Änderung am Text: Ansicht muss neu aufgebaut werden.
    pub fn needs_rebuild(&self, rev: u64) -> bool {
        rev != self.built_rev
    }

    /// Aufsteigend, absteigend, aus; andere Spalte beginnt wieder aufsteigend.
    pub fn cycle_sort(&mut self, col: usize) {
        self.sort = match self.sort {
            Some((c, SortDir::Asc)) if c == col => Some((col, SortDir::Desc)),
            Some((c, SortDir::Desc)) if c == col => None,
            _ => Some((col, SortDir::Asc)),
        };
    }

    pub fn is_identity(&self) -> bool {
        self.sort.is_none() && self.filter.is_empty()
    }
}

fn parse_num(s: &str) -> Option<f64> {
    let t = s.trim().replace(',', ".");
    if t.chars().any(|c| c.is_ascii_digit()) { t.parse().ok() } else { None }
}

/// Vergleicht zwei Zellwerte: numerisch oder natürlich ohne Groß/Klein.
pub fn compare_col(a: &str, b: &str, numeric: bool) -> Ordering {
    if numeric {
        if let (Some(x), Some(y)) = (parse_num(a), parse_num(b)) {
            return x.total_cmp(&y);
        }
    }
    natural_cmp(a, b)
}

/// Alle nicht-leeren Werte sind Zahlen (`,` oder `.` als Dezimalzeichen).
pub fn column_is_numeric(values: &[&str]) -> bool {
    values.iter().filter(|v| !v.trim().is_empty()).all(|v| parse_num(v).is_some())
}

/// Zerlegt in Ziffern- und Textläufe; Ziffern zählen als Zahl.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let (da, db) = (a[i].is_ascii_digit(), b[j].is_ascii_digit());
        let o = if da && db {
            let si = i;
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            let sj = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let na: String = a[si..i].iter().collect::<String>().trim_start_matches('0').into();
            let nb: String = b[sj..j].iter().collect::<String>().trim_start_matches('0').into();
            na.len()
                .cmp(&nb.len())
                .then_with(|| na.cmp(&nb))
                .then_with(|| (i - si).cmp(&(j - sj)))
        } else if !da && !db {
            let si = i;
            while i < a.len() && !a[i].is_ascii_digit() {
                i += 1;
            }
            let sj = j;
            while j < b.len() && !b[j].is_ascii_digit() {
                j += 1;
            }
            let ta: String = a[si..i].iter().collect::<String>().to_lowercase();
            let tb: String = b[sj..j].iter().collect::<String>().to_lowercase();
            ta.cmp(&tb)
        } else {
            // Ziffern vor Text
            if da { Ordering::Less } else { Ordering::Greater }
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csv::parse::parse;

    fn built(text: &str, header: bool, sort: Option<(usize, SortDir)>, filter: &str, fcol: Option<usize>) -> RowView {
        let d = Dialect { delim: ',', header };
        let p = parse(text, d.delim);
        let chars: Vec<char> = text.chars().collect();
        let mut v = RowView { sort, filter: filter.into(), filter_col: fcol, ..Default::default() };
        v.rebuild(&chars, &p, d, 1);
        v
    }

    #[test]
    fn natural_order() {
        assert_eq!(compare_col("a2", "a10", false), Ordering::Less);
        assert_eq!(compare_col("B", "a", false), Ordering::Greater);
        assert_eq!(compare_col("A", "a", false), Ordering::Equal);
        assert_eq!(compare_col("x1y", "x1z", false), Ordering::Less);
    }

    #[test]
    fn numeric_with_comma() {
        assert_eq!(compare_col("2,5", "10", true), Ordering::Less);
        assert_eq!(compare_col("2.5", "2,4", true), Ordering::Greater);
        assert!(column_is_numeric(&["1", "2,5", "", "3.0"]));
        assert!(!column_is_numeric(&["1", "abc"]));
    }

    #[test]
    fn empties_last_both_directions() {
        let t = "n\nb\n\na\n";
        let v = built(t, true, Some((0, SortDir::Asc)), "", None);
        assert_eq!(v.rows, vec![3, 1, 2]);
        let v = built(t, true, Some((0, SortDir::Desc)), "", None);
        assert_eq!(v.rows, vec![1, 3, 2]);
    }

    #[test]
    fn stable_sort() {
        let t = "k,i\na,1\nb,2\na,3\nb,4\n";
        let v = built(t, true, Some((0, SortDir::Asc)), "", None);
        assert_eq!(v.rows, vec![1, 3, 2, 4]);
        let v = built(t, true, Some((0, SortDir::Desc)), "", None);
        assert_eq!(v.rows, vec![2, 4, 1, 3]);
    }

    #[test]
    fn filter_case_insensitive_and_column() {
        let t = "n,m\nAnna,bob\nBOB,x\nc,ANNA\n";
        let v = built(t, true, None, "anna", None);
        assert_eq!(v.rows, vec![1, 3]);
        let v = built(t, true, None, "anna", Some(0));
        assert_eq!(v.rows, vec![1]);
        let v = built(t, false, None, "", None);
        assert_eq!(v.rows, vec![0, 1, 2, 3]);
    }

    #[test]
    fn own_insert_keeps_position_external_change_rebuilds() {
        let t = "n\nb\na\nc\n";
        let mut v = built(t, true, Some((0, SortDir::Asc)), "", None);
        assert_eq!(v.rows, vec![2, 1, 3]);
        assert!(!v.needs_rebuild(1));
        v.after_own_insert(1, 2, 2);
        assert_eq!(v.rows, vec![3, 2, 1, 4]);
        assert!(!v.needs_rebuild(2));
        assert!(v.needs_rebuild(3));
    }

    #[test]
    fn own_delete_and_mark_and_cycle() {
        let t = "n\nb\na\nc\nd\n";
        let mut v = built(t, true, Some((0, SortDir::Asc)), "", None);
        assert_eq!(v.rows, vec![2, 1, 3, 4]);
        v.after_own_delete(&[1, 3], 2);
        assert_eq!(v.rows, vec![1, 2]);
        assert!(!v.needs_rebuild(2));
        v.mark_own(5);
        assert!(!v.needs_rebuild(5));
        let mut w = RowView::default();
        assert!(w.is_identity());
        w.cycle_sort(2);
        assert_eq!(w.sort, Some((2, SortDir::Asc)));
        w.cycle_sort(2);
        assert_eq!(w.sort, Some((2, SortDir::Desc)));
        w.cycle_sort(2);
        assert_eq!(w.sort, None);
        w.cycle_sort(1);
        w.cycle_sort(3);
        assert_eq!(w.sort, Some((3, SortDir::Asc)));
        assert!(!w.is_identity());
    }
}
