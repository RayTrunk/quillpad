//! Linux: Drucker über CUPS (`lpstat`, `lpoptions`), Ausgabe als PDF an `lp`.
use super::cups;
use super::font::PrintFont;
use super::{default_paper, paper_named, pdf::write_pdf, Geom, Page, Paper, Printer};
use std::io::ErrorKind;
use std::os::unix::fs::OpenOptionsExt;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

/// Führt ein CUPS-Werkzeug aus; fehlt es, kommt "no-cups".
fn run(cmd: &str, args: &[&str]) -> Result<Output, String> {
    Command::new(cmd).args(args).output().map_err(|e| match e.kind() {
        ErrorKind::NotFound => "no-cups".to_string(),
        _ => format!("{cmd}: {e}"),
    })
}

pub fn printers() -> Result<Vec<Printer>, String> {
    let out = run("lpstat", &["-e"])?;
    cups::lp_result(out.status.success(), &String::from_utf8_lossy(&out.stderr))?;
    let names = cups::parse_lpstat_e(&String::from_utf8_lossy(&out.stdout));
    // Ohne Standarddrucker meldet `lpstat -d` evtl. einen Fehler; das ist kein Grund zum Abbruch.
    let default = run("lpstat", &["-d"])
        .ok()
        .and_then(|o| cups::parse_lpstat_d(&String::from_utf8_lossy(&o.stdout)));
    Ok(names
        .into_iter()
        .map(|name| Printer { is_default: default.as_deref() == Some(name.as_str()), name })
        .collect())
}

/// Format aus der Wahl, sonst Druckervorgabe, sonst A4 bzw. Letter. Rand 0, es gelten die eingestellten Ränder.
pub fn paper_for(printer: &str, choice: Option<&str>) -> Paper {
    if let Some(p) = choice.and_then(paper_named) {
        return p;
    }
    run("lpoptions", &["-p", printer, "-l"])
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| cups::parse_page_size(&String::from_utf8_lossy(&o.stdout)))
        .and_then(|name| paper_named(&name))
        .unwrap_or_else(default_paper)
}

#[allow(clippy::too_many_arguments)]
pub fn print_pages(
    printer: &str,
    pages: &[Page],
    g: &Geom,
    font: &PrintFont,
    title: &str,
    copies: u32,
    duplex: bool,
    paper: &Paper,
) -> Result<(), String> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let pdf = write_pdf(pages, g, font, title)?;
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let file = std::env::temp_dir().join(format!("quillpad-{}-{n}.pdf", std::process::id()));
    // create_new: eine schon vorhandene (womöglich fremde) Datei wird nie überschrieben;
    // 0o600: nur der eigene Benutzer kann den Inhalt lesen.
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&file)
        .and_then(|mut f| std::io::Write::write_all(&mut f, &pdf))
        .map_err(|e| format!("{}: {e}", file.display()))?;
    let args = cups::lp_args(printer, copies, duplex, &paper.name, title, &file);
    let out = Command::new("lp").args(&args).output();
    // Die Datei wird in jedem Fall entfernt; `lp` hat sie beim Erfolg schon an CUPS übergeben.
    let _ = std::fs::remove_file(&file);
    match out {
        Ok(o) => cups::lp_result(o.status.success(), &String::from_utf8_lossy(&o.stderr)),
        Err(e) if e.kind() == ErrorKind::NotFound => Err("no-cups".to_string()),
        Err(e) => Err(format!("lp: {e}")),
    }
}
