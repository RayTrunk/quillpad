//! Einstellungen und Sitzung als JSON im Benutzerordner.
use crate::editor::EdOpts;
use crate::theme::ThemeId;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub ed: EdOpts,
    pub font: String,
    pub font_size: f32,
    pub theme: ThemeId,
    /// Nur zum Einlesen älterer Einstellungen, aus denen das Design übernommen wird.
    #[serde(skip_serializing)]
    pub dark: bool,
    pub remember_session: bool,
    pub recent: Vec<String>,
    pub default_eol: String,
    pub default_enc: String,
    pub window: Option<[f32; 2]>,
    pub find_history: Vec<String>,
    pub workspace: Option<String>,
    pub large_file_enabled: bool,
    pub large_file_mb: u64,
    pub suppress_2gb_warning: bool,
    pub auto_reload: bool,
    /// Beim Start höchstens einmal am Tag bei GitHub nach einer neueren Version fragen.
    pub update_check: bool,
    /// Zeitpunkt der letzten Suche als Unix-Sekunden.
    pub last_update_check: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            ed: EdOpts::default(),
            font: "Courier New".into(),
            font_size: 16.0,
            theme: ThemeId::Light,
            dark: false,
            remember_session: true,
            recent: Vec::new(),
            default_eol: if cfg!(windows) { "crlf" } else { "lf" }.into(),
            default_enc: "utf8".into(),
            window: None,
            find_history: Vec::new(),
            workspace: None,
            large_file_enabled: true,
            large_file_mb: 256,
            suppress_2gb_warning: false,
            auto_reload: false,
            update_check: true,
            last_update_check: 0,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SessionTab {
    pub doc_id: u64,
    pub path: Option<String>,
    pub title: String,
    pub backup: Option<String>,
    pub enc: String,
    pub eol: String,
    pub language: Option<String>,
    pub anchor: usize,
    pub head: usize,
    pub top_row: f64,
    pub bookmarks: Vec<usize>,
    pub read_only: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Session {
    pub panes: [Vec<SessionTab>; 2],
    pub active: [usize; 2],
    pub active_pane: usize,
}

/// Wo das Programm seine Daten ablegt. In Tests und beim Bildschirmfoto: nirgends.
#[derive(Clone, Debug)]
pub struct Storage {
    pub dir: PathBuf,
}

impl Storage {
    pub fn default_location() -> Option<Storage> {
        dirs::config_dir().map(|d| Storage { dir: d.join("Quillpad") })
    }

    pub fn backup_dir(&self) -> PathBuf {
        self.dir.join("backup")
    }

    fn read<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn write<T: Serialize>(path: &Path, value: &T) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(b) = serde_json::to_vec_pretty(value) {
            let _ = crate::fileio::write_atomic(path, &b);
        }
    }

    pub fn load_settings(&self) -> Settings {
        let mut s: Settings = Self::read(&self.dir.join("settings.json"));
        if std::mem::take(&mut s.dark) && s.theme == ThemeId::Light {
            s.theme = ThemeId::Dark;
        }
        s
    }
    pub fn save_settings(&self, s: &Settings) {
        Self::write(&self.dir.join("settings.json"), s);
    }
    pub fn load_session(&self) -> Session {
        Self::read(&self.dir.join("session.json"))
    }
    pub fn save_session(&self, s: &Session) {
        Self::write(&self.dir.join("session.json"), s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_survive_a_roundtrip_and_tolerate_missing_fields() {
        let dir = std::env::temp_dir().join(format!("quillpad-set-{}", std::process::id()));
        let st = Storage { dir: dir.clone() };
        let mut s = Settings::default();
        s.font_size = 20.0;
        st.save_settings(&s);
        let back = st.load_settings();
        assert_eq!(back.font_size, 20.0);
        std::fs::write(dir.join("settings.json"), b"{\"dark\":true}").unwrap();
        let partial = st.load_settings();
        assert_eq!(partial.theme, ThemeId::Dark, "altes dark wird übernommen");
        assert_eq!(partial.font, "Courier New");
        std::fs::write(dir.join("settings.json"), b"kaputt").unwrap();
        assert_eq!(st.load_settings().theme, ThemeId::Light);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
