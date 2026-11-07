//! Nur ein Programmfenster: Ein zweiter Start reicht seine Dateien an das laufende Fenster
//! weiter und beendet sich. Die Verbindung bleibt auf diesem Rechner (127.0.0.1).
use crate::settings::Storage;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

fn port_file(st: &Storage) -> PathBuf {
    st.dir.join("instance.txt")
}

/// true = ein laufendes Fenster hat die Dateien übernommen.
pub fn try_forward(st: &Storage, files: &[PathBuf]) -> bool {
    let Ok(text) = std::fs::read_to_string(port_file(st)) else { return false };
    let mut parts = text.split_whitespace();
    let (Some(port), Some(token)) = (parts.next(), parts.next()) else { return false };
    let Ok(port) = port.parse::<u16>() else { return false };
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut s) = TcpStream::connect_timeout(&addr, Duration::from_millis(400)) else { return false };
    let _ = s.set_write_timeout(Some(Duration::from_millis(800)));
    let _ = s.set_read_timeout(Some(Duration::from_millis(800)));
    let mut msg = format!("{token}\n");
    for f in files {
        let full = std::fs::canonicalize(f).unwrap_or_else(|_| f.clone());
        msg.push_str(&crate::app::strip_verbatim(full).display().to_string());
        msg.push('\n');
    }
    msg.push('\n');
    if s.write_all(msg.as_bytes()).is_err() {
        return false;
    }
    // Das laufende Fenster bestätigt; ohne Antwort startet dieses Programm selbst.
    let mut ack = String::new();
    BufReader::new(s).read_line(&mut ack).is_ok() && ack.trim() == "ok"
}

/// Nimmt Dateien von späteren Starts an. Eine leere Liste bedeutet: Fenster nach vorn holen.
pub fn listen(st: &Storage, ctx: egui::Context) -> Option<Receiver<Vec<PathBuf>>> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).ok()?;
    let port = listener.local_addr().ok()?.port();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let token = format!("{:x}{:x}", nanos, std::process::id());
    std::fs::create_dir_all(&st.dir).ok()?;
    std::fs::write(port_file(st), format!("{port} {token}")).ok()?;
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let _ = conn.set_read_timeout(Some(Duration::from_secs(2)));
            let mut reader = BufReader::new(conn);
            let mut first = String::new();
            if reader.read_line(&mut first).is_err() || first.trim() != token {
                continue;
            }
            let mut files = Vec::new();
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(n) if n > 0 && !line.trim().is_empty() => files.push(PathBuf::from(line.trim_end())),
                    _ => break,
                }
            }
            if tx.send(files).is_err() {
                break;
            }
            let _ = reader.get_mut().write_all(b"ok\n");
            ctx.request_repaint();
        }
    });
    Some(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_start_hands_files_to_the_first() {
        let dir = std::env::temp_dir().join(format!("quillpad-inst-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let st = Storage { dir: dir.clone() };
        assert!(!try_forward(&st, &[]), "noch läuft nichts");
        let rx = listen(&st, egui::Context::default()).unwrap();
        assert!(try_forward(&st, &[PathBuf::from("gibt/es/nicht.txt")]));
        let got = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(got.len(), 1);
        assert!(got[0].ends_with("nicht.txt"));
        // Falsches Kennwort wird nicht angenommen.
        let text = std::fs::read_to_string(port_file(&st)).unwrap();
        let port = text.split_whitespace().next().unwrap();
        std::fs::write(port_file(&st), format!("{port} falsch")).unwrap();
        assert!(!try_forward(&st, &[]));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
