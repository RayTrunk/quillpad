//! Übersetzungen der Oberfläche: Englisch (Standard und Rückfall), Deutsch, Französisch.
//!
//! Die Texte stehen in `locales/<sprache>/<bereich>.toml`, gegliedert in Tabellen
//! (`[menu]` mit `file = "File"` ergibt den Schlüssel `menu.file`). Im Code holt
//! `t!("menu.file")` den Text in der eingestellten Sprache; Platzhalter in geschweiften
//! Klammern werden benannt ersetzt: `t!("status.line_col", line = 3, col = 5)`.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    En,
    De,
    Fr,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::En, Lang::De, Lang::Fr];

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::De => "de",
            Lang::Fr => "fr",
        }
    }

    /// Name der Sprache in ihr selbst, für die Auswahlliste.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::De => "Deutsch",
            Lang::Fr => "Français",
        }
    }

    /// "de-DE", "fr_CH", "en-US" … → passende Sprache, sonst None.
    pub fn from_locale(tag: &str) -> Option<Lang> {
        let primary = tag.split(['-', '_', '.']).next()?.to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| l.code() == primary)
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Sprache des Betriebssystems, wenn Quillpad sie kann; sonst Englisch.
pub fn system_lang() -> Lang {
    sys_locale::get_locales().find_map(|l| Lang::from_locale(&l)).unwrap_or(Lang::En)
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(l: Lang) {
    CURRENT.store(l as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::De,
        2 => Lang::Fr,
        _ => Lang::En,
    }
}

/// Alle Bereichsdateien je Sprache. Neue Bereiche hier eintragen.
macro_rules! areas {
    ($lang:literal) => {
        [
            include_str!(concat!("../locales/", $lang, "/common.toml")),
            include_str!(concat!("../locales/", $lang, "/chrome.toml")),
            include_str!(concat!("../locales/", $lang, "/find.toml")),
            include_str!(concat!("../locales/", $lang, "/panels.toml")),
            include_str!(concat!("../locales/", $lang, "/app.toml")),
        ]
    };
}

fn sources(l: Lang) -> [&'static str; 5] {
    match l {
        Lang::En => areas!("en"),
        Lang::De => areas!("de"),
        Lang::Fr => areas!("fr"),
    }
}

type Catalog = HashMap<String, String>;

fn flatten(prefix: &str, table: &toml::Table, out: &mut Catalog) {
    for (k, v) in table {
        let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
        match v {
            toml::Value::String(s) => {
                out.insert(key, s.clone());
            }
            toml::Value::Table(t) => flatten(&key, t, out),
            // Zahlen oder Wahrheitswerte gehören nicht in die Kataloge; sie würden nur verwirren.
            other => {
                out.insert(key, format!("{other:?}"));
            }
        }
    }
}

/// Liest eine Sprache. Fehlerhafte Dateien fallen in Tests auf; im Programm bleibt der Bereich leer.
pub fn parse(l: Lang) -> Result<Catalog, String> {
    let mut out = Catalog::new();
    for (i, text) in sources(l).iter().enumerate() {
        let table: toml::Table = text.parse().map_err(|e| format!("{} Bereich {i}: {e}", l.code()))?;
        flatten("", &table, &mut out);
    }
    Ok(out)
}

fn catalogs() -> &'static [Catalog; 3] {
    static C: OnceLock<[Catalog; 3]> = OnceLock::new();
    C.get_or_init(|| Lang::ALL.map(|l| parse(l).unwrap_or_default()))
}

/// Text in einer bestimmten Sprache; fehlt er, der englische, sonst der Schlüssel selbst.
pub fn tr_in(l: Lang, key: &str) -> String {
    let c = catalogs();
    c[l.index()]
        .get(key)
        .or_else(|| c[Lang::En.index()].get(key))
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

pub fn tr(key: &str) -> String {
    tr_in(lang(), key)
}

/// Ersetzt `{name}` durch die Werte; unbekannte Platzhalter bleiben stehen.
pub fn fill(mut text: String, args: &[(&str, String)]) -> String {
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

pub fn tr_args(key: &str, args: &[(&str, String)]) -> String {
    fill(tr(key), args)
}

/// Tastenkürzel in der Schreibweise der Sprache: im Code immer englisch ("Ctrl+Shift+S"),
/// angezeigt z. B. als "Strg+Umschalt+S" oder "Ctrl+Maj+S".
pub fn keys(combo: &str) -> String {
    keys_in(lang(), combo)
}

pub fn keys_in(l: Lang, combo: &str) -> String {
    if combo.is_empty() {
        return String::new();
    }
    combo
        .split('+')
        .map(|k| {
            let named = match (l, k) {
                (Lang::De, "Ctrl") => "Strg",
                (Lang::De, "Shift") => "Umschalt",
                (Lang::De, "Up") => "Hoch",
                (Lang::De, "Down") => "Runter",
                (Lang::De, "Left") => "Links",
                (Lang::De, "Right") => "Rechts",
                (Lang::De, "Del") => "Entf",
                (Lang::De, "Ins") => "Einfg",
                (Lang::De, "Space") => "Leertaste",
                (Lang::De, "Enter") => "Eingabe",
                (Lang::De, "PgUp") => "Bild auf",
                (Lang::De, "PgDn") => "Bild ab",
                (Lang::De, "Home") => "Pos1",
                (Lang::De, "End") => "Ende",
                (Lang::Fr, "Shift") => "Maj",
                (Lang::Fr, "Up") => "Haut",
                (Lang::Fr, "Down") => "Bas",
                (Lang::Fr, "Left") => "Gauche",
                (Lang::Fr, "Right") => "Droite",
                (Lang::Fr, "Del") => "Suppr",
                (Lang::Fr, "Ins") => "Inser",
                (Lang::Fr, "Space") => "Espace",
                (Lang::Fr, "Enter") => "Entrée",
                (Lang::Fr, "Esc") => "Échap",
                (Lang::Fr, "PgUp") => "Pg préc",
                (Lang::Fr, "PgDn") => "Pg suiv",
                (Lang::Fr, "Home") => "Début",
                (Lang::Fr, "End") => "Fin",
                (_, other) => other,
            };
            named.to_string()
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// `t!("schlüssel")` oder `t!("schlüssel", name = wert, …)`; liefert einen `String`.
#[macro_export]
macro_rules! t {
    ($key:literal) => {
        $crate::i18n::tr($key)
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::tr_args($key, &[$((stringify!($name), ($value).to_string())),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn placeholders(s: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut rest = s;
        while let Some(a) = rest.find('{') {
            let Some(b) = rest[a..].find('}') else { break };
            let name = &rest[a + 1..a + b];
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                out.insert(name.to_string());
            }
            rest = &rest[a + b + 1..];
        }
        out
    }

    #[test]
    fn all_languages_parse_and_have_the_same_keys_and_placeholders() {
        let en = parse(Lang::En).expect("en lesbar");
        assert!(!en.is_empty());
        for l in [Lang::De, Lang::Fr] {
            let other = parse(l).unwrap_or_else(|e| panic!("{e}"));
            let missing: Vec<_> = en.keys().filter(|k| !other.contains_key(*k)).collect();
            let extra: Vec<_> = other.keys().filter(|k| !en.contains_key(*k)).collect();
            assert!(missing.is_empty(), "{}: fehlt {missing:?}", l.code());
            assert!(extra.is_empty(), "{}: überzählig {extra:?}", l.code());
            for (k, v) in &en {
                assert_eq!(placeholders(v), placeholders(&other[k]), "{}: Platzhalter in {k}", l.code());
            }
        }
    }

    /// Jeder im Quelltext verwendete Schlüssel muss im englischen Katalog stehen.
    #[test]
    fn every_key_used_in_the_code_exists() {
        let en = parse(Lang::En).unwrap();
        let mut missing = BTreeSet::new();
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            // i18n.rs zeigt in Kommentaren nur Beispielschlüssel.
            if path.extension().and_then(|e| e.to_str()) != Some("rs") || name == "i18n.rs" {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            for (i, _) in src.match_indices("t!(\"") {
                // Nur freistehendes t!, nicht das Ende von format!( o. Ä.
                let before = src[..i].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let rest = &src[i + 4..];
                let Some(end) = rest.find('"') else { continue };
                let key = &rest[..end];
                if key.contains(' ') || key.is_empty() || key == "schlüssel" {
                    continue;
                }
                if !en.contains_key(key) {
                    missing.insert(format!("{}: {key}", path.file_name().unwrap().to_string_lossy()));
                }
            }
        }
        assert!(missing.is_empty(), "Schlüssel ohne Text: {missing:?}");
    }

    #[test]
    fn locales_and_keys_are_mapped() {
        assert_eq!(Lang::from_locale("de-DE"), Some(Lang::De));
        assert_eq!(Lang::from_locale("fr_CH.UTF-8"), Some(Lang::Fr));
        assert_eq!(Lang::from_locale("en-US"), Some(Lang::En));
        assert_eq!(Lang::from_locale("ja-JP"), None);
        assert_eq!(keys_in(Lang::De, "Ctrl+Shift+Up"), "Strg+Umschalt+Hoch");
        assert_eq!(keys_in(Lang::Fr, "Ctrl+Shift+S"), "Ctrl+Maj+S");
        assert_eq!(keys_in(Lang::En, "Ctrl+Shift+S"), "Ctrl+Shift+S");
        assert_eq!(keys_in(Lang::De, ""), "");
        assert_eq!(fill("{a} und {b}".into(), &[("a", "1".into()), ("b", "2".into())]), "1 und 2");
    }
}
