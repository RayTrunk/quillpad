//! Suchanfragen: normal, erweitert (\n, \t …) und reguläre Ausdrücke.
use regex::{Regex, RegexBuilder};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Normal,
    Extended,
    Regex,
}

#[derive(Clone, Debug)]
pub struct Query {
    pub pattern: String,
    pub mode: Mode,
    pub case: bool,
    pub whole: bool,
    pub dot_nl: bool,
}

/// Wandelt \n, \t, \r, \0, \\, \xHH und \uHHHH in die echten Zeichen um.
pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\n'),
            Some('0') => out.push('\0'),
            Some('\\') => out.push('\\'),
            Some(k @ ('x' | 'u')) => {
                let n = if k == 'x' { 2 } else { 4 };
                let hex: String = it.clone().take(n).collect();
                match (hex.len() == n)
                    .then(|| u32::from_str_radix(&hex, 16).ok())
                    .flatten()
                    .and_then(char::from_u32)
                {
                    Some(ch) => {
                        out.push(ch);
                        for _ in 0..n {
                            it.next();
                        }
                    }
                    None => {
                        out.push('\\');
                        out.push(k);
                    }
                }
            }
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

impl Query {
    pub fn compile(&self) -> Result<Regex, String> {
        if self.pattern.is_empty() {
            return Err(crate::t!("search.empty_pattern"));
        }
        let core = match self.mode {
            Mode::Normal => regex::escape(&self.pattern),
            Mode::Extended => regex::escape(&unescape(&self.pattern)),
            Mode::Regex => self.pattern.clone(),
        };
        let pat = if self.whole {
            format!(r"\b(?:{core})\b")
        } else {
            core
        };
        RegexBuilder::new(&pat)
            .case_insensitive(!self.case)
            .multi_line(true)
            .dot_matches_new_line(self.dot_nl && self.mode == Mode::Regex)
            .size_limit(64 * 1024 * 1024)
            .build()
            .map_err(|e| crate::t!("search.invalid_regex", error = e))
    }

    /// Ersetzt einen Treffer und liefert den neuen Text; Regex-Gruppen als $1 oder \1.
    pub fn expand(&self, re: &Regex, hay: &str, m: (usize, usize), with: &str) -> String {
        match self.mode {
            Mode::Normal => with.to_string(),
            Mode::Extended => unescape(with),
            Mode::Regex => {
                let templ = regex_template(with);
                match re.captures_at(hay, m.0) {
                    Some(c) if c.get(0).map(|g| g.start()) == Some(m.0) => {
                        let mut out = String::new();
                        c.expand(&templ, &mut out);
                        unescape(&out)
                    }
                    _ => with.to_string(),
                }
            }
        }
    }
}

/// „\1“ in die Schreibweise „${1}“ der Regex-Bibliothek übersetzen.
fn regex_template(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            if let Some(d) = it.peek().copied().filter(|d| d.is_ascii_digit() && *d != '0') {
                it.next();
                out.push_str(&format!("${{{d}}}"));
                continue;
            }
            out.push('\\');
            if let Some(n) = it.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Alle Treffer als Byte-Bereiche. Leere Treffer (z. B. `^`) zählen mit.
pub fn find_all(re: &Regex, hay: &str) -> Vec<(usize, usize)> {
    re.find_iter(hay).map(|m| (m.start(), m.end())).collect()
}

/// Ersetzt alle Treffer; gibt den Text und die Anzahl der Treffer zurück.
pub fn replace_all(q: &Query, re: &Regex, hay: &str, with: &str) -> (String, usize) {
    let mut out = String::with_capacity(hay.len());
    let mut last = 0;
    let mut n = 0;
    for (s, e) in find_all(re, hay) {
        out.push_str(&hay[last..s]);
        out.push_str(&q.expand(re, hay, (s, e), with));
        last = e;
        n += 1;
    }
    out.push_str(&hay[last..]);
    (out, n)
}

/// Dateifilter wie „*.cpp *.h“ oder „*.* !*.exe !\tests !+\log*“:
/// `!muster` schließt Dateien aus, `!\ordner` Ordner direkt im Suchordner, `!+\ordner` in jeder Tiefe.
#[derive(Clone, Debug, Default)]
pub struct FileFilter {
    include: Vec<String>,
    exclude: Vec<String>,
    exclude_top_dirs: Vec<String>,
    exclude_all_dirs: Vec<String>,
}

impl FileFilter {
    /// Schreibweise wie in .gitignore: `*.rs` nimmt auf, `!*.log` schließt Dateien aus,
    /// `!build/` (auch `!**/build/`) einen Ordner in jeder Tiefe, `!/build/` nur direkt im
    /// Suchordner. `\` gilt wie `/`.
    pub fn parse(filters: &str) -> FileFilter {
        let mut f = FileFilter::default();
        for part in filters.split([';', ',', ' ']).map(str::trim).filter(|p| !p.is_empty()) {
            let lower = part.to_lowercase().replace('\\', "/");
            let Some(rest) = lower.strip_prefix('!') else {
                f.include.push(lower);
                continue;
            };
            match rest.strip_suffix('/') {
                Some(dir) => {
                    if let Some(top) = dir.strip_prefix('/') {
                        if !top.is_empty() {
                            f.exclude_top_dirs.push(top.to_string());
                        }
                    } else {
                        let any = dir.trim_start_matches("**/");
                        if !any.is_empty() {
                            f.exclude_all_dirs.push(any.to_string());
                        }
                    }
                }
                None => f.exclude.push(rest.to_string()),
            }
        }
        f
    }

    pub fn file_ok(&self, name: &str) -> bool {
        let name = name.to_lowercase();
        let hit = |list: &[String]| list.iter().any(|p| p == "*.*" || p == "*" || glob(p, &name));
        !hit(&self.exclude) && (self.include.is_empty() || hit(&self.include))
    }

    /// depth 1 = Ordner direkt im Suchordner.
    pub fn dir_ok(&self, name: &str, depth: usize) -> bool {
        let name = name.to_lowercase();
        let hit = |list: &[String]| list.iter().any(|p| glob(p, &name));
        !hit(&self.exclude_all_dirs) && !(depth == 1 && hit(&self.exclude_top_dirs))
    }
}

#[cfg(test)]
pub fn name_matches(filters: &str, name: &str) -> bool {
    FileFilter::parse(filters).file_ok(name)
}

fn glob(p: &str, s: &str) -> bool {
    fn go(p: &[char], s: &[char]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some('*') => (0..=s.len()).any(|i| go(&p[1..], &s[i..])),
            Some('?') => !s.is_empty() && go(&p[1..], &s[1..]),
            Some(c) => s.first() == Some(c) && go(&p[1..], &s[1..]),
        }
    }
    let p: Vec<char> = p.chars().collect();
    let s: Vec<char> = s.chars().collect();
    go(&p, &s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(p: &str, mode: Mode) -> Query {
        Query {
            pattern: p.into(),
            mode,
            case: true,
            whole: false,
            dot_nl: false,
        }
    }

    #[test]
    fn normal_mode_treats_symbols_literally() {
        let re = q("a.b", Mode::Normal).compile().unwrap();
        assert_eq!(find_all(&re, "axb a.b").len(), 1);
    }

    #[test]
    fn extended_mode_understands_newline() {
        let re = q(r"a\nb", Mode::Extended).compile().unwrap();
        assert_eq!(find_all(&re, "a\nb").len(), 1);
    }

    #[test]
    fn whole_word_and_case() {
        let mut query = q("cat", Mode::Normal);
        query.whole = true;
        query.case = false;
        let re = query.compile().unwrap();
        assert_eq!(find_all(&re, "cat CAT concat").len(), 2);
    }

    #[test]
    fn regex_groups_both_spellings() {
        let query = q(r"(\w+)=(\w+)", Mode::Regex);
        let re = query.compile().unwrap();
        assert_eq!(replace_all(&query, &re, "a=b", r"\2=$1").0, "b=a");
    }

    #[test]
    fn replacement_is_literal_in_normal_mode() {
        let query = q("a", Mode::Normal);
        let re = query.compile().unwrap();
        assert_eq!(replace_all(&query, &re, "a", "$1\\n").0, "$1\\n");
    }

    #[test]
    fn bad_regex_reports_error() {
        assert!(q("(", Mode::Regex).compile().is_err());
        assert!(q("", Mode::Normal).compile().is_err());
    }

    #[test]
    fn filters_with_exclusions() {
        assert!(name_matches("*.* !*.exe !*.obj", "a.txt"));
        assert!(!name_matches("*.* !*.exe !*.obj", "A.EXE"));
        assert!(!name_matches("!*.log", "a.log"));
        assert!(name_matches("!*.log", "a.txt"));
        let f = FileFilter::parse("*.* !/tests/ !/bin*/ !log*/ !**/target/");
        assert!(!f.dir_ok("tests", 1));
        assert!(f.dir_ok("tests", 2), "/ordner/ gilt nur direkt im Suchordner");
        assert!(!f.dir_ok("bin64", 1));
        assert!(!f.dir_ok("logs", 3), "ordner/ gilt in jeder Tiefe");
        assert!(!f.dir_ok("target", 2), "**/ordner/ gilt in jeder Tiefe");
        assert!(f.dir_ok("src", 1));
        let w = FileFilter::parse("!\\tests\\ !obj\\");
        assert!(!w.dir_ok("tests", 1) && w.dir_ok("tests", 2), "Backslash wie Schrägstrich");
        assert!(!w.dir_ok("obj", 4));
        assert!(FileFilter::parse("!tests").file_ok("readme.txt"));
        assert!(!FileFilter::parse("!tests").file_ok("tests"), "ohne Schrägstrich am Ende ein Dateimuster");
        assert!(FileFilter::parse("!tests").dir_ok("tests", 1));
    }

    #[test]
    fn filters() {
        assert!(name_matches("*.rs;*.txt", "Main.RS"));
        assert!(!name_matches("*.rs", "main.c"));
        assert!(name_matches("", "x"));
        assert!(name_matches("*.*", "x"));
        assert!(name_matches("a?c.*", "abc.txt"));
    }
}
