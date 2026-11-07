// Auch im Entwicklungsbau als Fensterprogramm: kein Konsolenfenster vor dem Editor.
#![cfg_attr(not(test), windows_subsystem = "windows")]

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

/// Ein eigenes, einfaches Programmsymbol: Notizblatt mit Feder.
fn window_icon() -> egui::IconData {
    let n = 32usize;
    let mut rgba = vec![0u8; n * n * 4];
    let mut put = |x: i32, y: i32, c: [u8; 4]| {
        if x >= 0 && y >= 0 && (x as usize) < n && (y as usize) < n {
            let i = (y as usize * n + x as usize) * 4;
            rgba[i..i + 4].copy_from_slice(&c);
        }
    };
    for y in 3..30 {
        for x in 6..25 {
            put(x, y, if y < 8 { [70, 130, 200, 255] } else { [250, 250, 245, 255] });
        }
    }
    for y in (11..28).step_by(4) {
        for x in 9..22 {
            put(x, y, [170, 170, 190, 255]);
        }
    }
    for i in 0..15 {
        for w in 0..3 {
            put(26 - i + w, 6 + i, [210, 120, 40, 255]);
        }
    }
    egui::IconData { rgba, width: n as u32, height: n as u32 }
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
    let size = if screenshot_mode { None } else { loaded.window }.unwrap_or([945.0, 617.0]);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_min_inner_size([420.0, 260.0])
            .with_title("Quillpad")
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
