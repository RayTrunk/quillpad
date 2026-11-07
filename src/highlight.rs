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
            themes: ThemeId::ALL.into_iter().map(make_theme).collect(),
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

fn make_theme(t: ThemeId) -> Theme {
    let item = |scope: &str, color: u32, bold: bool| ThemeItem {
        scope: ScopeSelectors::from_str(scope).expect("gültiger Bereich"),
        style: StyleModifier {
            foreground: Some(c(color)),
            background: None,
            font_style: Some(if bold { FontStyle::BOLD } else { FontStyle::empty() }),
        },
    };
    let s = crate::theme::syntax(t);
    let (fg, comment, string, number, keyword, typ, pre, op, func, bad, attr) =
        (s.fg, s.comment, s.string, s.number, s.keyword, s.typ, s.pre, s.op, s.func, s.bad, s.attr);
    Theme {
        name: Some(t.label().to_string()),
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
    let by_ext = path
        .extension()
        .and_then(|x| x.to_str())
        .and_then(|x| e.by_extension(x));
    let by_name = path
        .file_name()
        .and_then(|x| x.to_str())
        .and_then(|x| e.by_extension(x));
    match by_name.or(by_ext) {
        Some(found) if found.name != "Plain Text" => Some(found.name.clone()),
        Some(_) => None,
        None => detect_by_content(text).map(str::to_string),
    }
}

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
            "sh" | "bash" | "dash" | "ksh" | "zsh" | "ash" => "Bourne Again Shell (bash)",
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
    theme: ThemeId,
    checkpoints: Vec<(ParseState, HighlightState)>,
    cur: Option<(usize, ParseState, HighlightState)>,
    cache: HashMap<usize, Arc<Vec<Span>>>,
}

impl DocHl {
    pub fn new(language: Option<String>) -> DocHl {
        DocHl {
            language,
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
            let rope_line = text.line(line);
            let long = rope_line.len_chars() > MAX_LINE;
            let mut s = if long {
                String::from("\n")
            } else {
                rope_line.to_string()
            };
            if !s.ends_with('\n') {
                s.push('\n');
            }
            let ops = ps.parse_line(&s, set).unwrap_or_default();
            let mut spans: Vec<Span> = Vec::new();
            for (style, piece) in HighlightIterator::new(&mut hs, &ops, &s, &highlighter) {
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
            if line >= first {
                self.cache.insert(line, Arc::new(if long { Vec::new() } else { spans }));
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
}
