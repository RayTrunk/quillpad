//! Update-Hinweis: fragt bei GitHub das neueste Release ab und vergleicht es mit der eigenen
//! Version. Heruntergeladen wird nichts; bei einer neueren Version öffnet sich auf Wunsch die
//! Release-Seite im Browser.
use serde::Deserialize;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

/// Repository auf GitHub, dessen Releases geprüft werden.
pub const REPO: &str = "RayTrunk/quillpad";
pub const REPO_URL: &str = "https://github.com/RayTrunk/quillpad";
/// Automatisch wird höchstens einmal in diesem Abstand gesucht.
pub const AUTO_INTERVAL_SECS: i64 = 24 * 60 * 60;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub notes: String,
}

pub enum Outcome {
    Newer(Release),
    UpToDate,
    Failed(String),
}

/// "v1.2.3", "1.2" oder "1.2.3-beta" → (1, 2, 3). Zusätze hinter - oder + zählen nicht.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let core = s.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let mut next = |required: bool| match parts.next() {
        Some(p) => p.parse::<u64>().ok(),
        None if !required => Some(0),
        None => None,
    };
    let v = (next(true)?, next(false)?, next(false)?);
    parts.next().is_none().then_some(v)
}

pub fn is_newer(remote: &str, local: &str) -> bool {
    matches!((parse_version(remote), parse_version(local)), (Some(r), Some(l)) if r > l)
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
}

fn fetch_latest() -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).build();
    let response = agent
        .get(&url)
        .set("User-Agent", "Quillpad-Update-Check")
        .set("Accept", "application/vnd.github+json")
        .call();
    let r: ApiRelease = match response {
        Ok(r) => r.into_json().map_err(|e| format!("Die Antwort von GitHub ist nicht lesbar: {e}"))?,
        Err(ureq::Error::Status(404, _)) => return Err("Es ist noch kein Release veröffentlicht.".into()),
        Err(ureq::Error::Status(code, _)) => return Err(format!("GitHub meldet Fehler {code}.")),
        Err(e) => return Err(format!("Keine Verbindung zu GitHub: {e}")),
    };
    Ok(Release {
        version: r.tag_name.trim_start_matches(['v', 'V']).to_string(),
        url: r.html_url,
        notes: r.body.unwrap_or_default(),
    })
}

/// Fragt im Hintergrund nach; das Fenster wird neu gezeichnet, sobald die Antwort da ist.
pub fn check_in_background(ctx: egui::Context) -> Receiver<Outcome> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let outcome = match fetch_latest() {
            Ok(r) if is_newer(&r.version, VERSION) => Outcome::Newer(r),
            Ok(_) => Outcome::UpToDate,
            Err(e) => Outcome::Failed(e),
        };
        let _ = tx.send(outcome);
        ctx.request_repaint();
    });
    rx
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_read_and_compared_by_number() {
        assert_eq!(parse_version("v0.9.0"), Some((0, 9, 0)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("1.0.0-beta.1"), Some((1, 0, 0)));
        assert_eq!(parse_version("eins"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert!(is_newer("v0.10.0", "0.9.0"), "10 ist größer als 9, nicht kleiner");
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("v0.9.0", "0.9.0"));
        assert!(!is_newer("0.8.5", "0.9.0"));
        assert!(!is_newer("kaputt", "0.9.0"));
    }

    /// Fragt wirklich bei GitHub nach. Nur auf Wunsch: cargo test -- --ignored live_release
    #[test]
    #[ignore]
    fn live_release_can_be_read() {
        let r = fetch_latest().expect("Release lesbar");
        println!("neuestes Release: {} ({})", r.version, r.url);
        assert!(parse_version(&r.version).is_some(), "Versionsnummer lesbar: {}", r.version);
        assert!(r.url.starts_with(REPO_URL), "{}", r.url);
    }
}
