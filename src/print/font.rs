//! Druckschrift: Hauptschrift mit Fettschnitt und Ersatzschriften für Symbole und CJK.
//! Liefert Zeichenbreiten für den Seitenaufbau und die Schriftdaten für PDF und GDI.
use super::Measure;
use ab_glyph::{Font, FontArc, FontVec};
use std::sync::Arc;

/// Eine Schriftdatei; bei `.ttc` gilt Index 0.
#[derive(Clone)]
pub struct Face {
    pub data: Arc<Vec<u8>>,
    pub index: u32,
}

pub struct PrintFont {
    pub regular: Face,
    pub bold: Option<Face>,
    pub fallbacks: Vec<Face>,
    // Einmal geparst, damit `advance` und `face` pro Zeichen billig bleiben.
    p_regular: FontArc,
    p_bold: Option<FontArc>,
    p_fallbacks: Vec<FontArc>,
}

/// Liest eine Schriftdatei aus dem Index (Schlüssel: kleingeschriebener Dateiname).
fn load_face(file: &str) -> Option<(Face, FontArc)> {
    let path = crate::ui_chrome::font_index().get(&file.to_lowercase())?;
    let bytes = std::fs::read(path).ok()?;
    let parsed = FontVec::try_from_vec_and_index(bytes.clone(), 0).ok()?;
    Some((Face { data: Arc::new(bytes), index: 0 }, FontArc::new(parsed)))
}

/// Dateilisten (normal, fett) zu einem Schriftnamen: dieselben wie im Editor
/// (`FONT_CHOICES`), dazu zwei Namen, die dort fehlen, sonst eine Vermutung nach dem Namen.
fn name_files(name: &str) -> (Vec<String>, Vec<String>) {
    let own = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    if let Some(c) = crate::ui_chrome::FONT_CHOICES.iter().find(|c| c.0 == name) {
        return (own(c.1), own(c.2));
    }
    match name {
        "DejaVu Sans Mono" => (own(&["DejaVuSansMono.ttf"]), own(&["DejaVuSansMono-Bold.ttf"])),
        "Liberation Mono" => (own(&["LiberationMono-Regular.ttf"]), own(&["LiberationMono-Bold.ttf"])),
        _ => {
            let stem: String = name.to_lowercase().split_whitespace().collect();
            (vec![format!("{stem}.ttf")], vec![format!("{stem}bold.ttf")])
        }
    }
}

/// Dateiname der Standard-Festbreitschrift laut fontconfig; Fehler werden ignoriert.
#[cfg(all(unix, not(target_os = "macos")))]
fn fc_match(pattern: &str) -> Option<String> {
    let out = std::process::Command::new("fc-match").args(["-f", "%{file}", pattern]).output().ok()?;
    let path = String::from_utf8(out.stdout).ok()?;
    std::path::Path::new(path.trim()).file_name().map(|n| n.to_string_lossy().into_owned())
}
#[cfg(not(all(unix, not(target_os = "macos"))))]
fn fc_match(_pattern: &str) -> Option<String> {
    None
}

impl PrintFont {
    /// Sucht die Hauptschrift: gewählte Schrift, dann Systemvorgabe, dann DejaVu Sans Mono.
    pub fn load(editor_font: &str) -> Option<PrintFont> {
        // Gruppen aus (Normaldateien, Fettdateien); die erste mit lesbarer Normalschrift gewinnt.
        let mut groups: Vec<(Vec<String>, Vec<String>)> = vec![name_files(editor_font)];
        if cfg!(windows) {
            groups.push((vec!["consola.ttf".into()], vec!["consolab.ttf".into()]));
        } else if let Some(r) = fc_match("monospace") {
            groups.push((vec![r], fc_match("monospace:bold").into_iter().collect()));
        }
        groups.push((vec!["DejaVuSansMono.ttf".into()], vec!["DejaVuSansMono-Bold.ttf".into()]));

        let mut found = None;
        for (regs, bolds) in groups {
            if let Some(main) = regs.iter().find_map(|r| load_face(r)) {
                found = Some((main, bolds.iter().find_map(|b| load_face(b))));
                break;
            }
        }
        let ((regular, p_regular), bold) = found?;
        let (bold, p_bold) = match bold {
            Some((f, p)) => (Some(f), Some(p)),
            None => (None, None),
        };
        let mut fallbacks = Vec::new();
        let mut p_fallbacks = Vec::new();
        for list in [crate::ui_chrome::SYM_FONTS, crate::ui_chrome::CJK_FONTS] {
            if let Some((f, p)) = list.iter().find_map(|n| load_face(n)) {
                fallbacks.push(f);
                p_fallbacks.push(p);
            }
        }
        Some(PrintFont { regular, bold, fallbacks, p_regular, p_bold, p_fallbacks })
    }

    /// 0 liefert Fett oder Normal, n die Ersatzschrift n-1.
    pub fn face_data(&self, face: u8, bold: bool) -> &Face {
        match face {
            0 if bold => self.bold.as_ref().unwrap_or(&self.regular),
            0 => &self.regular,
            n => &self.fallbacks[n as usize - 1],
        }
    }

    fn parsed(&self, face: u8, bold: bool) -> &FontArc {
        match face {
            0 if bold => self.p_bold.as_ref().unwrap_or(&self.p_regular),
            0 => &self.p_regular,
            n => &self.p_fallbacks[n as usize - 1],
        }
    }

    /// Glyph-ID des Zeichens in der Schrift (face, bold); 0 = fehlt (.notdef).
    pub(crate) fn glyph_id(&self, face: u8, bold: bool, c: char) -> u16 {
        self.parsed(face, bold).glyph_id(c).0
    }

    fn unit_advance(font: &FontArc, c: char) -> f32 {
        font.h_advance_unscaled(font.glyph_id(c)) / font.units_per_em().unwrap_or(1000.0)
    }
}

impl Measure for PrintFont {
    fn advance(&self, c: char, bold: bool) -> f32 {
        let face = self.face(c);
        let font = self.parsed(face, bold);
        if face == 0 && font.glyph_id(c).0 == 0 {
            // Keine Schrift kennt das Zeichen: Breite des Leerzeichens der Hauptschrift.
            return Self::unit_advance(font, ' ');
        }
        Self::unit_advance(font, c)
    }

    fn face(&self, c: char) -> u8 {
        if self.p_regular.glyph_id(c).0 != 0 {
            return 0;
        }
        match self.p_fallbacks.iter().position(|f| f.glyph_id(c).0 != 0) {
            Some(i) => i as u8 + 1,
            None => 0,
        }
    }

    fn ascent(&self) -> f32 {
        self.p_regular.ascent_unscaled() / self.p_regular.units_per_em().unwrap_or(1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_font_files_match_editor() {
        for c in crate::ui_chrome::FONT_CHOICES.iter() {
            let (r, b) = name_files(c.0);
            assert_eq!(r, c.1.iter().map(|s| s.to_string()).collect::<Vec<_>>(), "{}", c.0);
            assert_eq!(b, c.2.iter().map(|s| s.to_string()).collect::<Vec<_>>(), "{}", c.0);
        }
        assert_eq!(name_files("Foo Bar").0, vec!["foobar.ttf".to_string()]);
    }

    #[test]
    fn print_font_loads_and_measures() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PrintFont>();
        let Some(f) = PrintFont::load("Courier New") else {
            eprintln!("keine Schrift gefunden, Test übersprungen");
            return;
        };
        assert!(f.advance('a', false) > 0.0);
        assert_eq!(f.advance('a', false), f.advance('W', false));
        assert_eq!(f.face('a'), 0);
        assert!(f.advance('\u{1F600}', false) > 0.0);
        assert!(f.ascent() > 0.0);
        assert_eq!(f.face_data(0, true).index, 0);
    }
}
