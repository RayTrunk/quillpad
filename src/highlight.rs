//! Farbige Darstellung von Quelltext. Geparst wird stückweise, damit auch große
//! Dateien das Fenster nicht blockieren.
use crate::theme::ThemeId;
use egui::Color32;
use ropey::Rope;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use syntect::highlighting::{
    Color, FontStyle, HighlightIterator, HighlightState, Highlighter, ScopeSelectors, StyleModifier,
    Theme, ThemeItem, ThemeSettings,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxDefinition, SyntaxReference, SyntaxSet, SyntaxSetBuilder};

const CHECKPOINT: usize = 128;
const MAX_LINE: usize = 20_000;

pub struct Engine {
    pub ss: SyntaxSet,
    /// Eigene Sprachen, die in der großen Sammlung fehlen.
    pub own: SyntaxSet,
    /// Ein Farbsatz je Design, in der Reihenfolge von ThemeId::ALL.
    pub themes: Vec<Theme>,
    /// Farben für den Druck: [farbig, schwarzweiß].
    pub paper: [Theme; 2],
    pub names: Vec<String>,
}

impl Engine {
    /// Eine Sprache samt der Sammlung, zu der sie gehört.
    fn find(&self, name: &str) -> Option<(&SyntaxSet, &SyntaxReference)> {
        for set in [&self.ss, &self.own] {
            if let Some(found) = set.find_syntax_by_name(name) {
                return Some((set, found));
            }
        }
        None
    }

    fn by_extension(&self, ext: &str) -> Option<&SyntaxReference> {
        self.ss.find_syntax_by_extension(ext).or_else(|| self.own.find_syntax_by_extension(ext))
    }
}

static ENGINE: OnceLock<Engine> = OnceLock::new();

fn own_syntaxes() -> SyntaxSet {
    let mut b = SyntaxSetBuilder::new();
    for text in [
        include_str!("syntaxes/powershell.sublime-syntax"),
        include_str!("syntaxes/visualbasic.sublime-syntax"),
    ] {
        match SyntaxDefinition::load_from_str(text, true, None) {
            Ok(d) => b.add(d),
            Err(e) => eprintln!("Eigene Sprache fehlerhaft: {e}"),
        }
    }
    b.build()
}

pub fn engine() -> &'static Engine {
    ENGINE.get_or_init(|| {
        let ss: SyntaxSet = two_face::syntax::extra_newlines();
        let own = own_syntaxes();
        let mut names: Vec<String> = ss
            .syntaxes()
            .iter()
            .chain(own.syntaxes().iter())
            .filter(|s| !s.hidden && s.name != "Plain Text")
            .map(|s| s.name.clone())
            .collect();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        Engine {
            ss,
            own,
            themes: ThemeId::ALL
                .into_iter()
                .map(|t| make_theme_from(&crate::theme::syntax(t), &t.label()))
                .collect(),
            paper: [
                make_theme_from(&crate::theme::paper_syntax(false), "Papier"),
                make_theme_from(&crate::theme::paper_syntax(true), "Papier schwarzweiß"),
            ],
            names,
        }
    })
}

fn c(hex: u32) -> Color {
    Color {
        r: (hex >> 16) as u8,
        g: (hex >> 8) as u8,
        b: hex as u8,
        a: 255,
    }
}

fn make_theme_from(s: &crate::theme::Syntax, name: &str) -> Theme {
    let item = |scope: &str, color: u32, bold: bool| ThemeItem {
        scope: ScopeSelectors::from_str(scope).expect("gültiger Bereich"),
        style: StyleModifier {
            foreground: Some(c(color)),
            background: None,
            font_style: Some(if bold { FontStyle::BOLD } else { FontStyle::empty() }),
        },
    };
    let (fg, comment, string, number, keyword, typ, pre, op, func, bad, attr) =
        (s.fg, s.comment, s.string, s.number, s.keyword, s.typ, s.pre, s.op, s.func, s.bad, s.attr);
    Theme {
        name: Some(name.to_string()),
        author: None,
        settings: ThemeSettings {
            foreground: Some(c(fg)),
            background: Some(c(s.bg)),
            ..Default::default()
        },
        scopes: vec![
            item("punctuation, keyword.operator", op, true),
            item("variable, variable.parameter", fg, false),
            item("constant.numeric, constant.language, constant.character", number, false),
            item("string, string punctuation", string, false),
            item("constant.character.escape", number, false),
            item("keyword, storage.modifier", keyword, true),
            item("keyword.operator.word", keyword, true),
            item("storage, storage.type", typ, false),
            item("entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, support.type, support.class", typ, false),
            item("entity.name.function, support.function, meta.function-call variable.function", func, true),
            item("entity.name.tag", keyword, false),
            item("entity.other.attribute-name", attr, false),
            item("meta.preprocessor, keyword.control.import, keyword.other.preprocessor", pre, false),
            item("markup.heading, entity.name.section", keyword, true),
            item("markup.bold", fg, true),
            item("markup.raw, markup.inline.raw", pre, false),
            item("markup.inserted", s.inserted, false),
            item("markup.deleted", bad, false),
            item("comment, comment punctuation", comment, false),
            item("invalid", bad, false),
        ],
    }
}

/// Sprache anhand von Dateiname und erster Zeile bestimmen. None = normaler Text.
pub fn detect(name: &str, text: &str) -> Option<String> {
    let e = engine();
    let path = std::path::Path::new(name);
    let file = path.file_name().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
    let ext = path.extension().and_then(|x| x.to_str()).map(str::to_ascii_lowercase);
    // Endungen, bei denen die mitgelieferte Sammlung danebenliegt oder nichts kennt.
    let fixed: Option<Option<&str>> = match ext.as_deref() {
        Some("h") => Some(Some("C++")),
        Some("ksh") => Some(Some(BASH)),
        Some("jsx") => Some(Some("JavaScript")),
        // Die meisten .conf-Dateien sind keine nginx-Einstellungen.
        Some("conf") if !file.contains("nginx") => Some(None),
        _ => None,
    };
    let by_file = match fixed {
        Some(lang) => lang.map(str::to_string),
        None => {
            let by_ext = ext.as_deref().and_then(|x| e.by_extension(x));
            let orig = path.file_name().and_then(|x| x.to_str()).unwrap_or("");
            let by_name = e.by_extension(orig).or_else(|| e.by_extension(&file));
            match by_name.or(by_ext) {
                // Ausdrücklich Text (.txt): die Endung geht vor.
                Some(found) if found.name == "Plain Text" => return None,
                found => found.map(|f| f.name.clone()),
            }
        }
    };
    by_file.or_else(|| detect_by_content(text).map(str::to_string))
}

const BASH: &str = "Bourne Again Shell (bash)";

/// Sprache am Dateianfang erkennen, wenn die Endung nichts sagt. Es zählt nur die erste
/// nicht leere Zeile: bei `#!` der Name des Interpreters, sonst der Beginn eines Markup-Dokuments.
pub fn detect_by_content(text: &str) -> Option<&'static str> {
    let head = &text[..text.char_indices().nth(400).map_or(text.len(), |(i, _)| i)];
    let line = head.trim_start_matches(['\u{feff}', ' ', '\t', '\n', '\r']).lines().next()?.trim_end();
    if let Some(rest) = line.strip_prefix("#!") {
        // "#!/usr/bin/env -S python3 -u" -> "python3"; "#!/bin/bash" -> "bash"
        let mut words = rest.split_whitespace();
        let mut prog = words.next()?.rsplit('/').next()?;
        if prog == "env" {
            prog = words.find(|w| !w.starts_with('-') && !w.contains('='))?;
        }
        let name = prog.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
        return Some(match name {
            "sh" | "bash" | "dash" | "ksh" | "zsh" | "ash" => BASH,
            "python" | "pypy" => "Python",
            "perl" => "Perl",
            "php" => "PHP",
            "ruby" => "Ruby",
            "node" | "nodejs" | "deno" | "bun" => "JavaScript",
            "pwsh" | "powershell" => "PowerShell",
            "lua" | "luajit" => "Lua",
            _ => return None,
        });
    }
    let low = line.to_ascii_lowercase();
    if low.starts_with("<?xml") {
        Some("XML")
    } else if low.starts_with("<?php") {
        Some("PHP")
    } else if low.starts_with("<!doctype html") || low.starts_with("<html") {
        Some("HTML")
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Span {
    pub chars: u32,
    pub color: Color32,
    pub bold: bool,
}

pub struct DocHl {
    pub language: Option<String>,
    /// Von Hand gewählt: dann erkennt Quillpad die Sprache nicht mehr selbst.
    pub manual: bool,
    /// Stand des Dokuments bei der letzten selbsttätigen Erkennung.
    pub checked_rev: u64,
    theme: ThemeId,
    checkpoints: Vec<(ParseState, HighlightState)>,
    cur: Option<(usize, ParseState, HighlightState)>,
    cache: HashMap<usize, Arc<Vec<Span>>>,
}

impl DocHl {
    pub fn new(language: Option<String>) -> DocHl {
        DocHl {
            language,
            manual: false,
            checked_rev: 0,
            theme: ThemeId::Light,
            checkpoints: Vec::new(),
            cur: None,
            cache: HashMap::new(),
        }
    }

    pub fn set_language(&mut self, language: Option<String>) {
        self.language = language;
        self.reset();
    }

    pub fn set_theme(&mut self, theme: ThemeId) {
        if self.theme != theme {
            self.theme = theme;
            self.reset();
        }
    }

    fn reset(&mut self) {
        self.checkpoints.clear();
        self.cur = None;
        self.cache.clear();
    }

    pub fn invalidate_from(&mut self, line: usize) {
        if self.language.is_none() {
            return;
        }
        self.checkpoints.truncate(line / CHECKPOINT + 1);
        if self.cur.as_ref().is_some_and(|c| c.0 > line) {
            self.cur = None;
        }
        self.cache.retain(|l, _| *l < line);
    }

    pub fn spans(&self, line: usize) -> Option<Arc<Vec<Span>>> {
        self.cache.get(&line).cloned()
    }

    /// Arbeitet auf den sichtbaren Bereich hin. false = noch nicht fertig, später weiter.
    pub fn ensure(&mut self, text: &Rope, first: usize, last: usize, budget: Duration) -> bool {
        let Some(lang) = self.language.clone() else {
            return true;
        };
        let e = engine();
        let Some((set, syntax)) = e.find(&lang) else {
            return true;
        };
        let last = last.min(text.len_lines());
        let Some(need) = (first..last).find(|l| !self.cache.contains_key(l)) else {
            return true;
        };
        let idx = ThemeId::ALL.iter().position(|t| *t == self.theme).unwrap_or(0);
        let theme = &e.themes[idx];
        let highlighter = Highlighter::new(theme);
        if self.checkpoints.is_empty() {
            self.checkpoints.push((
                ParseState::new(syntax),
                HighlightState::new(&highlighter, ScopeStack::new()),
            ));
        }
        if self.cur.as_ref().map_or(true, |c| c.0 > need) {
            let idx = (need / CHECKPOINT).min(self.checkpoints.len() - 1);
            let (p, h) = self.checkpoints[idx].clone();
            self.cur = Some((idx * CHECKPOINT, p, h));
        }
        let (mut line, mut ps, mut hs) = self.cur.take().expect("gesetzt");
        let t0 = Instant::now();
        if self.cache.len() > 20_000 {
            self.cache.clear();
        }
        let mut done = true;
        while line < last {
            if line == self.checkpoints.len() * CHECKPOINT {
                self.checkpoints.push((ps.clone(), hs.clone()));
            }
            let (spans, _) = highlight_line(text.line(line), &mut ps, &mut hs, &highlighter, set);
            if line >= first {
                self.cache.insert(line, Arc::new(spans));
            }
            line += 1;
            if t0.elapsed() > budget && line < last {
                done = false;
                break;
            }
        }
        self.cur = Some((line, ps, hs));
        done
    }
}

/// Eine Zeile parsen und einfärben. Der Zustand wandert zur nächsten Zeile weiter.
/// Liefert die Abschnitte und ob die Zeile zu lang war (dann ohne Abschnitte).
fn highlight_line(
    rope_line: ropey::RopeSlice,
    ps: &mut ParseState,
    hs: &mut HighlightState,
    highlighter: &Highlighter,
    set: &SyntaxSet,
) -> (Vec<Span>, bool) {
    let long = rope_line.len_chars() > MAX_LINE;
    let mut s = if long { String::from("\n") } else { rope_line.to_string() };
    if !s.ends_with('\n') {
        s.push('\n');
    }
    let ops = ps.parse_line(&s, set).unwrap_or_default();
    let mut spans: Vec<Span> = Vec::new();
    for (style, piece) in HighlightIterator::new(hs, &ops, &s, highlighter) {
        let n = piece.chars().count() as u32;
        if n == 0 {
            continue;
        }
        let color = Color32::from_rgb(style.foreground.r, style.foreground.g, style.foreground.b);
        let bold = style.font_style.contains(FontStyle::BOLD);
        match spans.last_mut() {
            Some(l) if l.color == color && l.bold == bold => l.chars += n,
            _ => spans.push(Span { chars: n, color, bold }),
        }
    }
    (if long { Vec::new() } else { spans }, long)
}

/// Abschnitte für den Druck: feste Papierfarben, unabhängig vom Bildschirmdesign.
/// Geparst wird ab Zeile 0, damit mehrzeilige Konstrukte stimmen. Zeilen ohne Sprache
/// oder über MAX_LINE bestehen aus einem einzigen Abschnitt in Textfarbe.
#[cfg(test)]
pub fn paper_spans(text: &Rope, language: Option<&str>, mono: bool, lines: std::ops::Range<usize>) -> Vec<Vec<Span>> {
    paper_spans_until(text, language, mono, lines, &|| false).expect("ohne Abbruch")
}

/// Wie `paper_spans`, fragt aber alle 256 Zeilen `stop`; true bricht ab (None).
pub fn paper_spans_until(
    text: &Rope,
    language: Option<&str>,
    mono: bool,
    lines: std::ops::Range<usize>,
    stop: &dyn Fn() -> bool,
) -> Option<Vec<Vec<Span>>> {
    let e = engine();
    let pc = crate::theme::paper_syntax(mono);
    let fg = Color32::from_rgb((pc.fg >> 16) as u8, (pc.fg >> 8) as u8, pc.fg as u8);
    let total = text.len_lines();
    // Zeichen ohne Zeilenende
    let plain_len = |l: usize| {
        let line = text.line(l);
        let mut n = line.len_chars();
        if n > 0 && line.char(n - 1) == '\n' {
            n -= 1;
            if n > 0 && line.char(n - 1) == '\r' {
                n -= 1;
            }
        }
        n as u32
    };
    let plain = |l: usize| {
        let n = plain_len(l);
        if n == 0 { Vec::new() } else { vec![Span { chars: n, color: fg, bold: false }] }
    };
    let lines = lines.start.min(total)..lines.end.min(total);
    let Some((set, syntax)) = language.and_then(|l| e.find(l)) else {
        return Some(lines.map(plain).collect());
    };
    let theme = &e.paper[usize::from(mono)];
    let highlighter = Highlighter::new(theme);
    let mut ps = ParseState::new(syntax);
    let mut hs = HighlightState::new(&highlighter, ScopeStack::new());
    let mut out = Vec::with_capacity(lines.len());
    for line in 0..lines.end {
        if line % 256 == 0 && stop() {
            return None;
        }
        let (mut spans, long) = highlight_line(text.line(line), &mut ps, &mut hs, &highlighter, set);
        if line < lines.start {
            continue;
        }
        if long {
            out.push(plain(line));
            continue;
        }
        // Zeilenende (auch das angehängte der letzten Zeile) gehört nicht zum Druck: hinten abschneiden.
        let mut excess = spans.iter().map(|s| s.chars).sum::<u32>().saturating_sub(plain_len(line));
        while excess > 0 {
            let Some(l) = spans.last_mut() else { break };
            let cut = excess.min(l.chars);
            l.chars -= cut;
            excess -= cut;
            if l.chars == 0 {
                spans.pop();
            }
        }
        out.push(spans);
    }
    Some(out)
}

/// Kommentarzeichen je Sprache: (Zeile, Block)
pub fn comment_tokens(lang: Option<&str>) -> (Option<&'static str>, Option<(&'static str, &'static str)>) {
    let l = lang.unwrap_or("").to_lowercase();
    match l.as_str() {
        "c" | "c++" | "c#" | "java" | "javascript" | "typescript" | "typescriptreact"
        | "rust" | "go" | "swift" | "kotlin" | "scala" | "php" | "groovy" | "dart" | "objective-c"
        | "objective-c++" | "d" | "actionscript" | "json" | "less" | "sass" | "scss" | "zig" => {
            (Some("//"), Some(("/*", "*/")))
        }
        "css" => (None, Some(("/*", "*/"))),
        "python" | "ruby" | "perl" | "bourne again shell (bash)" | "shell-unix-generic" | "yaml" | "toml"
        | "makefile" | "dockerfile" | "r" | "nim" | "powershell" | "elixir" | "fish" | "tcl" | "cmake"
        | "ini" | "git config" | "nix" => (Some("#"), None),
        "html" | "xml" | "markdown" | "svg" | "vue component" => (None, Some(("<!--", "-->"))),
        "sql" | "lua" | "haskell" | "ada" | "vhdl" | "elm" => (Some("--"), None),
        "batch file" | "dos batch" => (Some("REM"), None),
        "lisp" | "clojure" | "scheme" | "x86_64 assembly" => (Some(";"), None),
        "latex" | "tex" | "erlang" | "matlab" => (Some("%"), None),
        "pascal" => (Some("//"), Some(("{", "}"))),
        "visual basic" | "vb.net" => (Some("'"), None),
        _ => (None, None),
    }
}

/// Muster für die Gliederung je Sprache.
pub fn function_patterns(lang: Option<&str>) -> Vec<&'static str> {
    match lang.unwrap_or("").to_lowercase().as_str() {
        "rust" => vec![
            r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+(\w+)",
            r"^\s*(?:pub\s+)?(?:struct|enum|trait|impl)\s+([\w<>, :]+)",
        ],
        "python" => vec![r"^\s*(?:async\s+)?def\s+(\w+)", r"^\s*class\s+(\w+)"],
        "javascript" | "typescript" | "typescriptreact" => vec![
            r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s*\*?\s*(\w+)",
            r"^\s*(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s*)?(?:\([^)]*\)|\w+)\s*=>",
            r"^\s*(?:export\s+)?(?:abstract\s+)?class\s+(\w+)",
        ],
        "go" => vec![r"^\s*func\s+(?:\([^)]*\)\s*)?(\w+)", r"^\s*type\s+(\w+)"],
        "php" => vec![r"^\s*(?:\w+\s+)*function\s+(\w+)", r"^\s*(?:abstract\s+|final\s+)?class\s+(\w+)"],
        "ruby" => vec![r"^\s*def\s+([\w.?!]+)", r"^\s*(?:class|module)\s+([\w:]+)"],
        "lua" => vec![r"^\s*(?:local\s+)?function\s+([\w.:]+)"],
        "bourne again shell (bash)" | "shell-unix-generic" => {
            vec![r"^\s*(?:function\s+)?(\w+)\s*\(\)", r"^\s*function\s+(\w+)"]
        }
        "powershell" => vec![r"(?i)^\s*function\s+([\w-]+)"],
        "batch file" | "dos batch" => vec![r"^\s*:(\w+)"],
        "sql" => vec![
            r"(?i)^\s*create\s+(?:or\s+replace\s+)?(?:table|view|function|procedure|index)\s+(?:if\s+not\s+exists\s+)?([\w.`\[\]]+)",
        ],
        "markdown" => vec![r"^#{1,6}\s+(.+)$"],
        "css" => vec![r"^\s*([.#]?[\w-][^{;]*?)\s*\{"],
        "c" | "c++" | "objective-c" | "java" | "c#" => vec![
            r"^\s*(?:[\w:<>,*&\[\]]+\s+)+[*&]?(\w+)\s*\([^;{]*\)\s*(?:const\s*)?(?:\{|$)",
            r"^\s*(?:\w+\s+)*(?:class|struct|enum|interface|namespace)\s+(\w+)",
        ],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_languages() {
        assert_eq!(detect("a.rs", "").as_deref(), Some("Rust"));
        assert_eq!(detect("a.py", "").as_deref(), Some("Python"));
        assert!(detect("a.unbekannt", "").is_none());
        assert!(detect("skript", "#!/bin/bash").is_some());
    }

    #[test]
    fn file_names_and_endings_pick_the_right_language() {
        let d = |n: &str, t: &str| detect(n, t);
        for n in ["a.sh", "A.SH", "a.bash", "a.zsh", "a.ksh", ".bashrc", ".zshrc", ".profile", "PKGBUILD"] {
            assert_eq!(d(n, "").as_deref(), Some(BASH), "{n}");
        }
        assert_eq!(d("kopf.h", "").as_deref(), Some("C++"), ".h ist C/C++, nicht Objective-C");
        assert_eq!(d("App.jsx", "").as_deref(), Some("JavaScript"));
        assert_eq!(d("Makefile", "").as_deref(), Some("Makefile"));
        assert_eq!(d("nginx.conf", "").as_deref(), Some("nginx"));
        assert_eq!(d("sites/nginx-site.conf", "").as_deref(), Some("nginx"));
        assert_eq!(d("fstab.conf", "# Einstellungen\na = 1"), None, ".conf ist meist nicht nginx");
        assert_eq!(d("dienst.conf", "#!/bin/sh\n").as_deref(), Some(BASH));
        assert_eq!(d("notiz.txt", "Hallo"), None);
        assert_eq!(d("notiz.txt", "<?xml version=\"1.0\"?>"), None, "Text bleibt Text");
        for name in ["C++", "JavaScript", BASH] {
            assert!(engine().find(name).is_some(), "{name} fehlt in der Sammlung");
        }
    }

    #[test]
    fn many_languages_available() {
        let n = engine().names.len();
        assert!(n > 100, "nur {n} Sprachen");
        for must in ["TOML", "TypeScript", "PowerShell", "SQL"] {
            assert!(engine().names.iter().any(|x| x == must), "{must} fehlt");
        }
    }

    #[test]
    fn content_rules_use_interpreter_and_markup_start() {
        assert_eq!(detect_by_content("#!/bin/sh\necho"), Some("Bourne Again Shell (bash)"));
        assert_eq!(detect_by_content("\n\n  #!/usr/bin/env python3\n"), Some("Python"));
        assert_eq!(detect_by_content("#!/usr/bin/env -S python3.12 -u\n"), Some("Python"));
        assert_eq!(detect_by_content("#!/usr/bin/node\n"), Some("JavaScript"));
        assert_eq!(detect_by_content("#!/usr/bin/fish\n"), None, "unbekannter Interpreter");
        assert_eq!(detect_by_content("#!/usr/bin/shellcheck\n"), None, "nur der ganze Name zählt");
        assert_eq!(detect_by_content("<?xml version=\"1.0\"?>"), Some("XML"));
        assert_eq!(detect_by_content("<?php echo 1;"), Some("PHP"));
        assert_eq!(detect_by_content("<!DOCTYPE html>"), Some("HTML"));
        assert_eq!(detect_by_content("<?"), None);
        assert_eq!(detect_by_content("hallo welt"), None);
        assert_eq!(detect_by_content("x\n#!/bin/sh"), None, "nur die erste Zeile zählt");
        assert_eq!(detect("skript.txt", "#!/bin/sh\n"), None, "die Endung geht vor");
    }

    fn light_comment() -> Color32 {
        let c = crate::theme::syntax(ThemeId::Light).comment;
        Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
    }

    #[test]
    fn own_languages_work() {
        assert_eq!(detect("a.ps1", "").as_deref(), Some("PowerShell"));
        assert_eq!(detect("a.vbs", "").as_deref(), Some("Visual Basic"));
        let text = Rope::from_str("# x\nGet-Item $a\n");
        let mut h = DocHl::new(Some("PowerShell".into()));
        assert!(h.ensure(&text, 0, 2, Duration::from_secs(5)));
        assert_eq!(h.spans(0).unwrap()[0].color, light_comment());
    }

    #[test]
    fn colors_comment_and_keyword_differently() {
        let text = Rope::from_str("// x\nfn main() {}\n");
        let mut h = DocHl::new(Some("Rust".into()));
        assert!(h.ensure(&text, 0, 2, Duration::from_secs(5)));
        let comment = h.spans(0).unwrap();
        let code = h.spans(1).unwrap();
        assert_eq!(comment[0].color, light_comment());
        assert_ne!(code[0].color, comment[0].color);
    }

    #[test]
    fn edit_invalidates_following_lines() {
        let text = Rope::from_str("a\nb\n");
        let mut h = DocHl::new(Some("Rust".into()));
        h.ensure(&text, 0, 2, Duration::from_secs(5));
        assert!(h.spans(1).is_some());
        h.invalidate_from(1);
        assert!(h.spans(0).is_some());
        assert!(h.spans(1).is_none());
    }

    #[test]
    fn jump_far_in_big_file_parses_up_to_there() {
        let s: String = (0..5000).map(|i| format!("// {i}\n")).collect();
        let text = Rope::from_str(&s);
        let mut h = DocHl::new(Some("Rust".into()));
        while !h.ensure(&text, 4000, 4010, Duration::from_millis(5)) {}
        assert!(h.spans(4005).is_some());
    }

    fn rgb(h: u32) -> Color32 {
        Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8)
    }

    #[test]
    fn paper_colors_are_readable_on_white() {
        use crate::theme::{contrast, paper_syntax};
        for mono in [false, true] {
            let s = paper_syntax(mono);
            for (n, c) in [
                ("fg", s.fg), ("comment", s.comment), ("string", s.string), ("number", s.number),
                ("keyword", s.keyword), ("typ", s.typ), ("pre", s.pre), ("op", s.op), ("func", s.func),
                ("bad", s.bad), ("attr", s.attr), ("inserted", s.inserted),
            ] {
                assert!(contrast(c, 0xFFFFFF) >= 4.5, "{n} {c:06X} mono={mono}");
            }
        }
        assert_eq!(paper_syntax(false).fg, 0x000000);
        assert_eq!(paper_syntax(true).comment, 0x666666);
    }

    #[test]
    fn paper_spans_ignore_the_screen_theme() {
        use crate::theme::paper_syntax;
        // In Rust ist "#" kein Kommentar, daher "//".
        let text = Rope::from_str("// Kommentar
fn main() {}
");
        let before = paper_spans(&text, Some("Rust"), false, 0..2);
        let mut hl = DocHl::new(Some("Rust".into()));
        hl.set_theme(ThemeId::Monokai);
        hl.ensure(&text, 0, 2, Duration::from_secs(5));
        let after = paper_spans(&text, Some("Rust"), false, 0..2);
        let key = |v: &Vec<Span>| v.iter().map(|s| (s.chars, s.color, s.bold)).collect::<Vec<_>>();
        assert_eq!(key(&before[0]), key(&after[0]));
        assert!(!before[0].is_empty());
        assert!(before[0].iter().all(|s| s.color == rgb(paper_syntax(false).comment)));
        let mono = paper_spans(&text, Some("Rust"), true, 0..2);
        assert!(mono[0].iter().all(|s| s.color == rgb(0x666666)));
        assert!(mono[1].iter().all(|s| s.color == rgb(0x000000)));
        for i in 0..2usize {
            for v in [&before, &mono] {
                assert_eq!(v[i].iter().map(|s| s.chars).sum::<u32>(), 12);
            }
        }
        // Ohne Sprache: ein Abschnitt in Textfarbe
        let none = paper_spans(&text, None, false, 1..2);
        assert_eq!(none.len(), 1);
        assert_eq!(none[0].len(), 1);
        assert_eq!(none[0][0].chars, 12);
    }

    #[test]
    fn paper_spans_last_line_without_newline() {
        let text = Rope::from_str("fn a() {}
let x = 1;");
        for lang in [Some("Rust"), None] {
            let v = paper_spans(&text, lang, false, 0..2);
            assert_eq!(v.len(), 2);
            assert_eq!(v[0].iter().map(|s| s.chars).sum::<u32>(), 9);
            assert_eq!(v[1].iter().map(|s| s.chars).sum::<u32>(), 10, "{lang:?}");
        }
    }
}
