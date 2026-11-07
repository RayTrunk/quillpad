//! Dateien lesen und schreiben: Kodierungen, Zeilenenden, sicheres Speichern.
use encoding_rs::Encoding;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Eol {
    Crlf,
    Lf,
    Cr,
}

impl Eol {
    pub fn label(self) -> String {
        match self {
            Eol::Crlf => crate::t!("eol.crlf"),
            Eol::Lf => crate::t!("eol.lf"),
            Eol::Cr => crate::t!("eol.cr"),
        }
    }
    /// Kurzform für die Statuszeile.
    pub fn short(self) -> &'static str {
        match self {
            Eol::Crlf => "CRLF",
            Eol::Lf => "LF",
            Eol::Cr => "CR",
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Eol::Crlf => "\r\n",
            Eol::Lf => "\n",
            Eol::Cr => "\r",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Eol::Crlf => "crlf",
            Eol::Lf => "lf",
            Eol::Cr => "cr",
        }
    }
    pub fn from_name(s: &str) -> Eol {
        match s {
            "lf" => Eol::Lf,
            "cr" => Eol::Cr,
            _ => Eol::Crlf,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Enc {
    Ansi,
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Legacy(&'static Encoding),
}

/// Zeichensätze im Menü „Codierung“: (Gruppe, Anzeigename, Kodierung)
pub fn legacy_list() -> Vec<(&'static str, &'static str, &'static Encoding)> {
    use encoding_rs::*;
    let [west, central, cyrillic, greek, turkish, hebrew, arabic, japanese, chinese, korean] = *legacy_groups();
    vec![
        (west, "ISO 8859-15", ISO_8859_15),
        (central, "Windows-1250", WINDOWS_1250),
        (central, "ISO 8859-2", ISO_8859_2),
        (cyrillic, "Windows-1251", WINDOWS_1251),
        (cyrillic, "KOI8-R", KOI8_R),
        (greek, "Windows-1253", WINDOWS_1253),
        (turkish, "Windows-1254", WINDOWS_1254),
        (hebrew, "Windows-1255", WINDOWS_1255),
        (arabic, "Windows-1256", WINDOWS_1256),
        (japanese, "Shift-JIS", SHIFT_JIS),
        (chinese, "GBK", GBK),
        (chinese, "Big5", BIG5),
        (korean, "EUC-KR", EUC_KR),
    ]
}

/// Gruppennamen der Zeichensätze in der eingestellten Sprache. Das Menü braucht
/// `&'static str`; deshalb wird je Sprache einmal festgehalten (wenige Bytes, wächst nicht).
fn legacy_groups() -> &'static [&'static str; 10] {
    use std::sync::OnceLock;
    static CACHE: [OnceLock<[&'static str; 10]>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    CACHE[crate::i18n::lang() as usize].get_or_init(|| {
        [
            crate::t!("app.enc_group.western"),
            crate::t!("app.enc_group.central"),
            crate::t!("app.enc_group.cyrillic"),
            crate::t!("app.enc_group.greek"),
            crate::t!("app.enc_group.turkish"),
            crate::t!("app.enc_group.hebrew"),
            crate::t!("app.enc_group.arabic"),
            crate::t!("app.enc_group.japanese"),
            crate::t!("app.enc_group.chinese"),
            crate::t!("app.enc_group.korean"),
        ]
        .map(|s| &*Box::leak(s.into_boxed_str()))
    })
}

impl Enc {
    pub fn label(self) -> String {
        match self {
            Enc::Ansi => "ANSI".into(),
            Enc::Utf8 => "UTF-8".into(),
            Enc::Utf8Bom => crate::t!("enc.utf8_bom"),
            Enc::Utf16Le => "UTF-16 LE".into(),
            Enc::Utf16Be => "UTF-16 BE".into(),
            Enc::Legacy(e) => e.name().to_string(),
        }
    }
    pub fn name(self) -> String {
        match self {
            Enc::Ansi => "ansi".into(),
            Enc::Utf8 => "utf8".into(),
            Enc::Utf8Bom => "utf8bom".into(),
            Enc::Utf16Le => "utf16le".into(),
            Enc::Utf16Be => "utf16be".into(),
            Enc::Legacy(e) => e.name().to_string(),
        }
    }
    pub fn from_name(s: &str) -> Enc {
        match s {
            "ansi" => Enc::Ansi,
            "utf8bom" => Enc::Utf8Bom,
            "utf16le" => Enc::Utf16Le,
            "utf16be" => Enc::Utf16Be,
            "utf8" => Enc::Utf8,
            other => Encoding::for_label(other.as_bytes())
                .map(Enc::Legacy)
                .unwrap_or(Enc::Utf8),
        }
    }
}

/// Die ANSI-Kodierung des Systems. Ist das System selbst auf UTF-8 gestellt, zählt die
/// Vorgabe seines Gebietsschemas.
#[cfg(windows)]
pub fn ansi_encoding() -> &'static Encoding {
    extern "system" {
        fn GetACP() -> u32;
        fn GetLocaleInfoEx(name: *const u16, kind: u32, data: *mut u16, len: i32) -> i32;
    }
    // SAFETY: Beide Aufrufe lesen nur Systemwerte; der Puffer ist groß genug für die zwei
    // Einheiten, die LOCALE_RETURN_NUMBER liefert.
    let mut cp = unsafe { GetACP() };
    if cp == 65001 {
        let name: Vec<u16> = "!x-sys-default-locale\0".encode_utf16().collect();
        let mut buf = [0u16; 2];
        let n = unsafe { GetLocaleInfoEx(name.as_ptr(), 0x2000_1004, buf.as_mut_ptr(), 2) };
        cp = if n > 0 { buf[0] as u32 | (buf[1] as u32) << 16 } else { 1252 };
    }
    use encoding_rs::*;
    match cp {
        874 => WINDOWS_874,
        932 => SHIFT_JIS,
        936 => GBK,
        949 => EUC_KR,
        950 => BIG5,
        1250 => WINDOWS_1250,
        1251 => WINDOWS_1251,
        1253 => WINDOWS_1253,
        1254 => WINDOWS_1254,
        1255 => WINDOWS_1255,
        1256 => WINDOWS_1256,
        1257 => WINDOWS_1257,
        1258 => WINDOWS_1258,
        _ => WINDOWS_1252,
    }
}

#[cfg(not(windows))]
pub fn ansi_encoding() -> &'static Encoding {
    encoding_rs::WINDOWS_1252
}

/// Wandelt alle Zeilenenden nach LF; intern arbeitet der Editor nur mit LF.
pub fn to_lf(s: String) -> String {
    if !s.contains('\r') {
        return s;
    }
    s.replace("\r\n", "\n").replace('\r', "\n")
}

fn detect_eol(s: &str, default: Eol) -> Eol {
    let b = s.as_bytes();
    for i in 0..b.len() {
        match b[i] {
            b'\n' => return Eol::Lf,
            b'\r' => {
                return if b.get(i + 1) == Some(&b'\n') {
                    Eol::Crlf
                } else {
                    Eol::Cr
                }
            }
            _ => {}
        }
    }
    default
}

/// Erkennt die Kodierung am Dateianfang; UTF-8 falls gültig, sonst ANSI.
pub fn detect_enc(bytes: &[u8]) -> Enc {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        Enc::Utf8Bom
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        Enc::Utf16Le
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        Enc::Utf16Be
    } else if std::str::from_utf8(bytes).is_ok() {
        Enc::Utf8
    } else {
        Enc::Ansi
    }
}

pub fn decode_with(bytes: &[u8], enc: Enc) -> String {
    match enc {
        Enc::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
        Enc::Utf8Bom => {
            let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
            String::from_utf8_lossy(b).into_owned()
        }
        Enc::Utf16Le => {
            let b = bytes.strip_prefix(&[0xFF, 0xFE]).unwrap_or(bytes);
            encoding_rs::UTF_16LE
                .decode_without_bom_handling(b)
                .0
                .into_owned()
        }
        Enc::Utf16Be => {
            let b = bytes.strip_prefix(&[0xFE, 0xFF]).unwrap_or(bytes);
            encoding_rs::UTF_16BE
                .decode_without_bom_handling(b)
                .0
                .into_owned()
        }
        Enc::Ansi => ansi_encoding()
            .decode_without_bom_handling(bytes)
            .0
            .into_owned(),
        Enc::Legacy(e) => e.decode_without_bom_handling(bytes).0.into_owned(),
    }
}

pub struct Loaded {
    pub text: String,
    pub enc: Enc,
    pub eol: Eol,
}

pub fn decode(bytes: &[u8], default_eol: Eol) -> Loaded {
    let enc = detect_enc(bytes);
    let raw = decode_with(bytes, enc);
    let eol = detect_eol(&raw, default_eol);
    Loaded {
        text: to_lf(raw),
        enc,
        eol,
    }
}

/// Liefert die Bytes und ob Zeichen in dieser Kodierung nicht darstellbar waren.
pub fn encode(text_lf: &str, enc: Enc, eol: Eol) -> (Vec<u8>, bool) {
    let owned;
    let text: &str = if eol == Eol::Lf {
        text_lf
    } else {
        owned = text_lf.replace('\n', eol.as_str());
        &owned
    };
    match enc {
        Enc::Utf8 => (text.as_bytes().to_vec(), false),
        Enc::Utf8Bom => {
            let mut v = vec![0xEF, 0xBB, 0xBF];
            v.extend_from_slice(text.as_bytes());
            (v, false)
        }
        Enc::Utf16Le => {
            let mut v = vec![0xFF, 0xFE];
            for u in text.encode_utf16() {
                v.extend_from_slice(&u.to_le_bytes());
            }
            (v, false)
        }
        Enc::Utf16Be => {
            let mut v = vec![0xFE, 0xFF];
            for u in text.encode_utf16() {
                v.extend_from_slice(&u.to_be_bytes());
            }
            (v, false)
        }
        Enc::Ansi => {
            let (b, _, lossy) = ansi_encoding().encode(text);
            (b.into_owned(), lossy)
        }
        Enc::Legacy(e) => {
            let (b, _, lossy) = e.encode(text);
            (b.into_owned(), lossy)
        }
    }
}

/// Schreibt erst in eine Nachbardatei und ersetzt dann das Ziel, damit ein Fehler
/// beim Schreiben die vorhandene Datei nicht zerstört.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "datei".into());
    let tmp = target.with_file_name(format!(".{name}.quillpad-tmp"));
    if std::fs::write(&tmp, bytes).is_ok() {
        if let Ok(meta) = std::fs::metadata(&target) {
            let _ = std::fs::set_permissions(&tmp, meta.permissions());
        }
        if std::fs::rename(&tmp, &target).is_ok() {
            return Ok(());
        }
        let _ = std::fs::remove_file(&tmp);
    }
    std::fs::write(&target, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_encodings() {
        for enc in [Enc::Utf8, Enc::Utf8Bom, Enc::Utf16Le, Enc::Utf16Be] {
            let (b, lossy) = encode("Größe\nzwei", enc, Eol::Crlf);
            assert!(!lossy);
            let l = decode(&b, Eol::Lf);
            assert_eq!(l.text, "Größe\nzwei");
            assert_eq!(l.enc, enc);
            assert_eq!(l.eol, Eol::Crlf);
        }
    }

    #[test]
    fn ansi_is_detected_when_not_utf8() {
        let l = decode(&[b'G', 0xF6, b'\n'], Eol::Crlf);
        assert_eq!(l.enc, Enc::Ansi);
        assert_eq!(l.text, "Gö\n");
        assert_eq!(l.eol, Eol::Lf);
    }

    #[test]
    fn unmappable_is_reported() {
        assert!(encode("日本", Enc::Ansi, Eol::Lf).1);
    }

    #[test]
    fn atomic_write_replaces_content() {
        let dir = std::env::temp_dir().join(format!("quillpad-io-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.txt");
        write_atomic(&f, b"eins").unwrap();
        write_atomic(&f, b"zwei").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"zwei");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
