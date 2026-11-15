//! CUPS-Hilfen ohne Systemaufrufe: Ausgaben von `lpstat`/`lpoptions` lesen und die
//! Argumente für `lp` bauen. Plattformunabhängig, damit die Tests überall laufen.
use std::path::Path;

/// `lpstat -e`: ein Druckername je Zeile.
pub fn parse_lpstat_e(out: &str) -> Vec<String> {
    out.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

/// `lpstat -d`: Text nach dem letzten ": " (englisch und deutsch); sonst kein Standard.
pub fn parse_lpstat_d(out: &str) -> Option<String> {
    let line = out.lines().map(str::trim).find(|l| !l.is_empty())?;
    let (_, name) = line.rsplit_once(": ")?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// `lpoptions -l`: in der Zeile `PageSize/…:` der mit `*` markierte Eintrag.
pub fn parse_page_size(lpoptions_l: &str) -> Option<String> {
    let line = lpoptions_l.lines().find(|l| l.trim_start().starts_with("PageSize/"))?;
    let (_, values) = line.split_once(':')?;
    values.split_whitespace().find_map(|v| v.strip_prefix('*')).filter(|v| !v.is_empty()).map(String::from)
}

/// Argumente für `lp`; Duplex wendet an der langen Kante.
pub fn lp_args(printer: &str, copies: u32, duplex: bool, media: &str, title: &str, file: &Path) -> Vec<String> {
    let mut a: Vec<String> = vec!["-d".into(), printer.into(), "-n".into(), copies.max(1).to_string()];
    if duplex {
        a.extend(["-o".into(), "sides=two-sided-long-edge".into()]);
    }
    a.extend(["-o".into(), format!("media={media}"), "-t".into(), title.into()]);
    a.push(file.to_string_lossy().into_owned());
    a
}

/// Ergebnis von `lp`: bei Fehler die Meldung aus stderr.
pub fn lp_result(status_ok: bool, stderr: &str) -> Result<(), String> {
    if status_ok {
        return Ok(());
    }
    let msg = stderr.trim();
    Err(if msg.is_empty() { "lp: Druckauftrag abgelehnt".to_string() } else { msg.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cups_lists_printers_and_default() {
        assert_eq!(parse_lpstat_e("HP_LaserJet\nPDF\n"), vec!["HP_LaserJet".to_string(), "PDF".to_string()]);
        assert_eq!(parse_lpstat_d("system default destination: PDF"), Some("PDF".to_string()));
        assert_eq!(parse_lpstat_d("Standardziel des Systems: PDF\n"), Some("PDF".to_string()));
        assert_eq!(parse_lpstat_d("no system default destination"), None);
    }

    #[test]
    fn cups_reads_default_page_size() {
        assert_eq!(parse_page_size("PageSize/Media Size: Letter *A4 A5 Legal"), Some("A4".to_string()));
        let multi = "Resolution/Resolution: *600dpi\nPageSize/Papiergröße: *Letter A4\n";
        assert_eq!(parse_page_size(multi), Some("Letter".to_string()));
        assert_eq!(parse_page_size("Duplex/2-Sided: *None"), None);
    }

    #[test]
    fn cups_builds_lp_arguments() {
        let p = Path::new("/tmp/x.pdf");
        let ps = p.to_string_lossy().into_owned();
        let want: Vec<String> = ["-d", "HP", "-n", "2", "-o", "sides=two-sided-long-edge", "-o", "media=A4", "-t", "backup.sh", &ps]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(lp_args("HP", 2, true, "A4", "backup.sh", p), want);
        let simplex: Vec<String> = ["-d", "HP", "-n", "2", "-o", "media=A4", "-t", "backup.sh", &ps]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(lp_args("HP", 2, false, "A4", "backup.sh", p), simplex);
    }

    #[test]
    fn cups_reports_lp_errors() {
        let msg = "lp: The printer or class does not exist.";
        assert_eq!(lp_result(false, msg), Err(msg.to_string()));
        assert_eq!(lp_result(true, ""), Ok(()));
    }
}
