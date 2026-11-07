// Auch im Entwicklungsbau als Fensterprogramm: kein Konsolenfenster vor dem Editor.
#![cfg_attr(not(test), windows_subsystem = "windows")]
// Quillpad – © 2026 R. Trunk – GNU GPL v3 oder neuer (siehe LICENSE)

mod app;
mod commands;
mod doc;
mod editor;
mod fileio;
mod highlight;
mod icons;
mod instance;
mod search;
mod settings;
#[cfg(test)]
mod tests;
mod textops;
mod theme;
mod ui_chrome;
mod ui_find;
mod ui_panels;
mod update;

use std::path::PathBuf;

/// Startoptionen. --screenshot und --state dienen der automatischen Prüfung.
#[derive(Default)]
struct Args {
    screenshot: Option<PathBuf>,
    state: Option<String>,
    files: Vec<PathBuf>,
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let mut it = std::env::args_os().skip(1);
    while let Some(x) = it.next() {
        match x.to_str() {
            Some("--screenshot") => a.screenshot = it.next().map(PathBuf::from),
            Some("--state") => a.state = it.next().and_then(|s| s.into_string().ok()),
            _ => a.files.push(PathBuf::from(x)),
        }
    }
    a
}

/// Programmsymbol (Notizblatt mit Feder), dasselbe wie in der exe und im Installer.
fn window_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/quillpad.png")).unwrap_or_default()
}

fn main() -> eframe::Result {
    let args = parse_args();
    let screenshot_mode = args.screenshot.is_some();
    let storage = if screenshot_mode { None } else { settings::Storage::default_location() };
    if let Some(st) = &storage {
        if instance::try_forward(st, &args.files) {
            return Ok(());
        }
    }
    let loaded = storage.as_ref().map(|s| s.load_settings()).unwrap_or_default();
    let size = if screenshot_mode { None } else { loaded.window }.unwrap_or([1000.0, 700.0]);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_min_inner_size([420.0, 260.0])
            .with_title("Quillpad")
            // Unter Linux ordnet der Desktop das Fenster damit quillpad.desktop zu.
            .with_app_id("quillpad")
            .with_icon(std::sync::Arc::new(window_icon()))
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "Quillpad",
        options,
        Box::new(move |cc| {
            let inbox = storage.as_ref().and_then(|st| instance::listen(st, cc.egui_ctx.clone()));
            let mut a = app::App::new(cc.egui_ctx.clone(), storage, loaded, args.files);
            a.inbox = inbox;
            a.shot = args.screenshot.map(app::Shot::new);
            if let Some(state) = args.state {
                a.apply_test_state(&state);
            }
            Ok(Box::new(a))
        }),
    )
}
