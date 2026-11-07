//! Reine Textumformungen (ohne Bezug zur Oberfläche), einzeln getestet.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaseKind {
    Upper,
    Lower,
    Proper,
    Invert,
}

pub fn change_case(s: &str, k: CaseKind) -> String {
    match k {
        CaseKind::Upper => s.to_uppercase(),
        CaseKind::Lower => s.to_lowercase(),
        CaseKind::Invert => s
            .chars()
            .flat_map(|c| {
                if c.is_uppercase() {
                    c.to_lowercase().collect::<Vec<_>>()
                } else {
                    c.to_uppercase().collect::<Vec<_>>()
                }
            })
            .collect(),
        CaseKind::Proper => {
            let mut out = String::with_capacity(s.len());
            let mut start = true;
            for c in s.chars() {
                if c.is_alphanumeric() {
                    if start {
                        out.extend(c.to_uppercase());
                    } else {
                        out.extend(c.to_lowercase());
                    }
                    start = false;
                } else {
                    out.push(c);
                    start = c != '\'';
                }
            }
            out
        }
    }
}

pub fn sort_lines(s: &str, descending: bool, ignore_case: bool) -> String {
    let mut v: Vec<&str> = s.split('\n').collect();
    if ignore_case {
        v.sort_by_cached_key(|l| l.to_lowercase());
    } else {
        v.sort();
    }
    if descending {
        v.reverse();
    }
    v.join("\n")
}

/// Entfernt doppelte Zeilen und behält jeweils die erste.
pub fn dedup_lines(s: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    s.split('\n')
        .filter(|l| seen.insert(*l))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn remove_empty_lines(s: &str, blank_too: bool) -> String {
    s.split('\n')
        .filter(|l| {
            if blank_too {
                !l.trim().is_empty()
            } else {
                !l.is_empty()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn trim_trailing(s: &str) -> String {
    s.split('\n')
        .map(|l| l.trim_end_matches([' ', '\t']))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn trim_leading(s: &str) -> String {
    s.split('\n')
        .map(|l| l.trim_start_matches([' ', '\t']))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Verbindet Zeilen mit je einem Leerzeichen.
pub fn join_lines(s: &str) -> String {
    let mut out = String::new();
    for (i, l) in s.split('\n').enumerate() {
        if i == 0 {
            out.push_str(l.trim_end());
        } else {
            let t = l.trim();
            if !t.is_empty() {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(t);
            }
        }
    }
    out
}

pub fn tabs_to_spaces(s: &str, tab: usize) -> String {
    let mut out = String::with_capacity(s.len());
    let mut col = 0;
    for c in s.chars() {
        match c {
            '\t' => {
                let n = tab - col % tab;
                out.extend(std::iter::repeat(' ').take(n));
                col += n;
            }
            '\n' => {
                out.push(c);
                col = 0;
            }
            _ => {
                out.push(c);
                col += 1;
            }
        }
    }
    out
}

/// Ersetzt führende Leerzeichen einer Zeile durch Tabs, soweit sie volle Stopps ergeben.
pub fn leading_spaces_to_tabs(s: &str, tab: usize) -> String {
    s.split('\n')
        .map(|l| {
            let n = l.chars().take_while(|c| *c == ' ').count();
            let tabs = n / tab;
            format!("{}{}", "\t".repeat(tabs), &l[tabs * tab..])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Einrückung: fügt jeder nichtleeren Zeile eine Einheit hinzu.
pub fn indent(s: &str, unit: &str) -> String {
    s.split('\n')
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("{unit}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn outdent(s: &str, tab: usize) -> String {
    s.split('\n')
        .map(|l| {
            if let Some(r) = l.strip_prefix('\t') {
                r.to_string()
            } else {
                let n = l.chars().take(tab).take_while(|c| *c == ' ').count();
                l[n..].to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Schaltet den Zeilenkommentar um. mode: None = umschalten, Some(true) = setzen, Some(false) = entfernen.
pub fn comment_lines(s: &str, token: &str, mode: Option<bool>) -> String {
    let lines: Vec<&str> = s.split('\n').collect();
    let all_commented = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .all(|l| l.trim_start().starts_with(token));
    let add = mode.unwrap_or(!all_commented);
    lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                return l.to_string();
            }
            let ws = l.len() - l.trim_start().len();
            let (a, b) = l.split_at(ws);
            if add {
                format!("{a}{token} {b}")
            } else if let Some(r) = b.strip_prefix(token) {
                format!("{a}{}", r.strip_prefix(' ').unwrap_or(r))
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocStats {
    pub chars: usize,
    pub words: usize,
    pub lines: usize,
    pub bytes: usize,
}

pub fn stats(s: &str) -> DocStats {
    DocStats {
        chars: s.chars().count(),
        words: s.split_whitespace().count(),
        lines: s.split('\n').count(),
        bytes: s.len(),
    }
}

/// Faltbereiche aus Einrückung ableiten. Rückgabe je Zeile: letzte Zeile des Blocks, 0 = keiner.
pub fn compute_folds<'a>(lines: impl Iterator<Item = std::borrow::Cow<'a, str>>, n: usize) -> Vec<u32> {
    let mut ends = vec![0u32; n];
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let mut last_nb = 0usize;
    for (l, text) in lines.enumerate() {
        let t = text.trim_end_matches('\n');
        let mut ind = 0;
        let mut first = None;
        for c in t.chars() {
            match c {
                ' ' => ind += 1,
                '\t' => ind += 4,
                _ => {
                    first = Some(c);
                    break;
                }
            }
        }
        let Some(_) = first else { continue };
        let trimmed = t.trim_start();
        let closer = trimmed.starts_with(['}', ')', ']'])
            || trimmed.starts_with("</")
            || trimmed.starts_with("end");
        while let Some(&(si, sl)) = stack.last() {
            if si < ind {
                break;
            }
            stack.pop();
            let end = if si == ind && closer && last_nb > sl { l } else { last_nb };
            if end > sl {
                ends[sl] = end as u32;
            }
        }
        stack.push((ind, l));
        last_nb = l;
    }
    while let Some((_, sl)) = stack.pop() {
        if last_nb > sl {
            ends[sl] = last_nb as u32;
        }
    }
    ends
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn cases() {
        assert_eq!(change_case("aB ç", CaseKind::Upper), "AB Ç");
        assert_eq!(change_case("aB", CaseKind::Lower), "ab");
        assert_eq!(change_case("hello wORLD", CaseKind::Proper), "Hello World");
        assert_eq!(change_case("aB", CaseKind::Invert), "Ab");
    }

    #[test]
    fn line_tools() {
        assert_eq!(sort_lines("b\na\nC", false, false), "C\na\nb");
        assert_eq!(sort_lines("b\na\nC", false, true), "a\nb\nC");
        assert_eq!(sort_lines("a\nb", true, false), "b\na");
        assert_eq!(dedup_lines("a\nb\na"), "a\nb");
        assert_eq!(remove_empty_lines("a\n\n \nb", false), "a\n \nb");
        assert_eq!(remove_empty_lines("a\n\n \nb", true), "a\nb");
        assert_eq!(trim_trailing("a \t\nb "), "a\nb");
        assert_eq!(trim_leading(" a\n\tb"), "a\nb");
        assert_eq!(join_lines("a \n  b\n\nc"), "a b c");
    }

    #[test]
    fn tabs() {
        assert_eq!(tabs_to_spaces("a\tb\n\tc", 4), "a   b\n    c");
        assert_eq!(leading_spaces_to_tabs("     x", 4), "\t x");
        assert_eq!(indent("a\n\nb", "\t"), "\ta\n\n\tb");
        assert_eq!(outdent("\ta\n    b\n  c", 4), "a\nb\nc");
    }

    #[test]
    fn comments_toggle_back_and_forth() {
        let on = comment_lines("  a\n\n  b", "//", None);
        assert_eq!(on, "  // a\n\n  // b");
        assert_eq!(comment_lines(&on, "//", None), "  a\n\n  b");
        assert_eq!(comment_lines("//a", "//", Some(false)), "a");
    }

    #[test]
    fn statistics() {
        let s = stats("zwei Wörter\nnoch");
        assert_eq!((s.chars, s.words, s.lines), (16, 3, 2));
    }

    #[test]
    fn folds_from_indent_include_closer() {
        let text = ["fn a() {", "    x", "    if y {", "        z", "    }", "}", "b"];
        let ends = compute_folds(text.iter().map(|l| Cow::Borrowed(*l)), text.len());
        assert_eq!(ends, vec![5, 0, 4, 0, 0, 0, 0]);
    }
}
