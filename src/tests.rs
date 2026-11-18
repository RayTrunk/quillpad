//! Durchläufe durch das ganze Programm ohne Fenster: echte Eingaben, echte Bilder des Zustands.
use crate::app::{Action, App};
use crate::commands::{Cmd, Motion};
use crate::doc::SelSet;
use crate::fileio::Enc;
use crate::textops::CaseKind;
use crate::ui_find::FindTab;
use egui::{pos2, vec2, Context, Event, Key, Modifiers, PointerButton, RawInput, Rect};

struct Rig {
    ctx: Context,
    app: App,
    time: f64,
    /// Zuletzt in die Zwischenablage gelegter Text.
    copied: Option<String>,
}

impl Rig {
    fn new() -> Rig {
        // Die Tests prüfen deutsche Texte.
        crate::i18n::set_lang(crate::i18n::Lang::De);
        let ctx = Context::default();
        let app = App::new(ctx.clone(), None, Default::default(), Vec::new());
        let mut r = Rig { ctx, app, time: 0.0, copied: None };
        r.frames(3);
        r
    }

    fn frame_with(&mut self, events: Vec<Event>) {
        self.frame_mods(events, Modifiers::NONE);
    }

    /// Ein Bild mit gehaltenen Umschalttasten.
    fn frame_mods(&mut self, events: Vec<Event>, modifiers: Modifiers) {
        self.time += 1.0 / 60.0;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 650.0))),
            time: Some(self.time),
            events,
            modifiers,
            ..Default::default()
        };
        let app = &mut self.app;
        let out = self.ctx.run(input, |ctx| app.frame(ctx));
        for c in out.platform_output.commands {
            if let egui::OutputCommand::CopyText(t) = c {
                self.copied = Some(t);
            }
        }
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.frame_with(Vec::new());
        }
    }

    fn typed(&mut self, s: &str) {
        self.frame_with(vec![Event::Text(s.to_string())]);
        self.frames(1);
    }

    fn key(&mut self, key: Key, modifiers: Modifiers) {
        self.frame_with(vec![Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }]);
        self.frames(1);
    }

    fn act(&mut self, a: Action) {
        let ctx = self.ctx.clone();
        self.app.run(&ctx, a);
        self.frames(2);
    }

    fn text(&self) -> String {
        self.app.cur_doc().full_text()
    }

    fn set_text(&mut self, t: &str) {
        self.act(Action::Cmd(Cmd::SelectAll));
        self.act(Action::Cmd(Cmd::Paste(t.to_string())));
    }

    fn click_text_at(&mut self, p: egui::Pos2) {
        let mods = Modifiers::NONE;
        self.frame_with(vec![Event::PointerMoved(p)]);
        self.frame_with(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: mods }]);
        self.frame_with(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: mods }]);
        self.frames(1);
    }

    fn head(&self) -> usize {
        self.app.cur_view().sels.main().head
    }
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("quillpad-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn starts_with_one_empty_new_document() {
    let r = Rig::new();
    assert_eq!(r.app.panes[0].tabs.len(), 1);
    assert_eq!(r.app.cur_doc().title, "Unbenannt 1");
    assert_eq!(r.text(), "");
    assert!(!r.app.cur_doc().is_modified());
}

#[test]
fn typing_goes_into_the_focused_editor() {
    let mut r = Rig::new();
    r.typed("hallo");
    assert_eq!(r.text(), "hallo");
    assert_eq!(r.head(), 5);
    assert!(r.app.cur_doc().is_modified());
    r.key(Key::Backspace, Modifiers::NONE);
    assert_eq!(r.text(), "hall");
    r.key(Key::Enter, Modifiers::NONE);
    r.typed("x");
    assert_eq!(r.text(), "hall\nx");
}

#[test]
fn ctrl_z_and_ctrl_y_work_through_shortcuts() {
    let mut r = Rig::new();
    r.typed("abc");
    r.key(Key::Z, Modifiers::CTRL);
    assert_eq!(r.text(), "");
    r.key(Key::Y, Modifiers::CTRL);
    assert_eq!(r.text(), "abc");
}

#[test]
fn arrow_keys_and_selection() {
    let mut r = Rig::new();
    r.set_text("eins\nzwei\ndrei");
    r.key(Key::Home, Modifiers::CTRL);
    assert_eq!(r.head(), 0);
    r.key(Key::ArrowDown, Modifiers::NONE);
    r.key(Key::End, Modifiers::NONE);
    assert_eq!(r.head(), 9);
    r.key(Key::ArrowLeft, Modifiers::SHIFT);
    r.key(Key::ArrowLeft, Modifiers::SHIFT);
    let s = r.app.cur_view().sels.main();
    assert_eq!((s.start(), s.end()), (7, 9));
    r.typed("X");
    assert_eq!(r.text(), "eins\nzwX\ndrei");
}

#[test]
fn vertical_movement_keeps_the_column() {
    let mut r = Rig::new();
    r.set_text("langezeile\nk\nlangezeile");
    r.key(Key::Home, Modifiers::CTRL);
    for _ in 0..6 {
        r.key(Key::ArrowRight, Modifiers::NONE);
    }
    r.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(r.head(), 12, "kurze Zeile: Ende");
    r.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(r.head(), 19, "wieder Spalte 6");
}

#[test]
fn line_shortcuts_duplicate_delete_move() {
    let mut r = Rig::new();
    r.set_text("a\nb");
    r.key(Key::Home, Modifiers::CTRL);
    r.key(Key::D, Modifiers::CTRL);
    assert_eq!(r.text(), "a\na\nb");
    r.key(Key::L, Modifiers::CTRL);
    assert_eq!(r.text(), "a\nb");
    r.key(Key::ArrowDown, Modifiers::CTRL | Modifiers::SHIFT);
    assert_eq!(r.text(), "b\na");
    assert_eq!(r.head(), 2, "Cursor wandert mit der Zeile");
    r.key(Key::ArrowUp, Modifiers::CTRL | Modifiers::SHIFT);
    assert_eq!(r.text(), "a\nb");
}

#[test]
fn clipboard_events_and_multi_caret_paste() {
    let mut r = Rig::new();
    r.set_text("eins zwei");
    r.key(Key::Home, Modifiers::CTRL);
    r.act(Action::Cmd(Cmd::Move(Motion::WordRight, true)));
    r.frame_with(vec![Event::Cut]);
    r.frames(1);
    assert_eq!(r.text(), "zwei");
    r.frame_with(vec![Event::Paste("neu ".into())]);
    r.frames(1);
    assert_eq!(r.text(), "neu zwei");
}

#[test]
fn box_selection_types_on_every_line() {
    let mut r = Rig::new();
    r.set_text("aaa\nbbb\nccc");
    r.key(Key::Home, Modifiers::CTRL);
    r.act(Action::Cmd(Cmd::Move(Motion::Right, false)));
    r.key(Key::ArrowDown, Modifiers::ALT | Modifiers::SHIFT);
    r.key(Key::ArrowDown, Modifiers::ALT | Modifiers::SHIFT);
    assert_eq!(r.app.cur_view().sels.sels.len(), 3);
    r.typed("X");
    assert_eq!(r.text(), "aXaa\nbXbb\ncXcc");
    r.key(Key::Z, Modifiers::CTRL);
    assert_eq!(r.text(), "aaa\nbbb\nccc");
}

#[test]
fn mouse_click_places_the_caret_and_drag_selects() {
    let mut r = Rig::new();
    r.set_text("0123456789\nzweite");
    let m_ch = r.app.metrics.as_ref().unwrap().ch;
    let line_h = r.app.metrics.as_ref().unwrap().line_h;
    // Die Lage des Schreibfelds hängt von der Schrift des Systems ab, darum vom Fenster erfragen.
    let id = egui::Id::new(("editor", r.app.cur_view().uid));
    let text = r.ctx.read_response(id).expect("Schreibfeld gezeichnet").rect;
    let x0 = text.left() + 2.0;
    let p = pos2(x0 + m_ch * 4.2, text.top() + line_h * 0.5);
    r.click_text_at(p);
    assert_eq!(r.head(), 4);
    let q = pos2(x0 + m_ch * 8.2, p.y);
    r.frame_with(vec![Event::PointerMoved(p)]);
    r.frame_with(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }]);
    r.frame_with(vec![Event::PointerMoved(q)]);
    r.frame_with(vec![Event::PointerButton { pos: q, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
    r.frames(1);
    let s = r.app.cur_view().sels.main();
    assert_eq!((s.start(), s.end()), (4, 8));
}

#[test]
fn tabs_new_switch_close() {
    let mut r = Rig::new();
    r.key(Key::N, Modifiers::CTRL);
    assert_eq!(r.app.panes[0].tabs.len(), 2);
    assert_eq!(r.app.cur_doc().title, "Unbenannt 2");
    r.typed("x");
    r.key(Key::Tab, Modifiers::CTRL);
    assert_eq!(r.app.cur_doc().title, "Unbenannt 1");
    r.key(Key::W, Modifiers::CTRL);
    assert_eq!(r.app.panes[0].tabs.len(), 1);
    assert_eq!(r.app.cur_doc().title, "Unbenannt 2");
    // Geändert: Schließen fragt nach und passiert noch nicht.
    r.key(Key::W, Modifiers::CTRL);
    assert!(matches!(r.app.dialog, Some(crate::app::Dialog::ConfirmClose { .. })));
    assert_eq!(r.text(), "x");
    let id = r.app.cur_id();
    r.app.confirm_close_answer(id, Some(false));
    r.frames(2);
    assert_eq!(r.app.panes[0].tabs.len(), 1, "letzter Tab wird durch einen leeren ersetzt");
    assert_eq!(r.text(), "");
}

#[test]
fn closing_with_cancel_keeps_everything() {
    let mut r = Rig::new();
    r.typed("wichtig");
    r.key(Key::N, Modifiers::CTRL);
    r.typed("auch");
    r.act(Action::CloseAll);
    let id = match r.app.dialog {
        Some(crate::app::Dialog::ConfirmClose { doc }) => doc,
        _ => panic!("Nachfrage erwartet"),
    };
    r.app.confirm_close_answer(id, None);
    r.frames(2);
    assert_eq!(r.app.docs.len(), 2);
    assert!(r.app.closing.is_empty());
}

#[test]
fn open_edit_save_keeps_encoding_and_line_endings() {
    let dir = temp_dir("save");
    let f = dir.join("deutsch.py");
    std::fs::write(&f, b"# Gr\xF6\xDFe\r\nprint(1)\r\n").unwrap();
    let mut r = Rig::new();
    assert!(r.app.open_path(&f).is_some());
    r.frames(2);
    assert_eq!(r.app.panes[0].tabs.len(), 1, "leeres Unbenannt 1 wird ersetzt");
    assert_eq!(r.app.cur_doc().enc, Enc::Ansi);
    assert_eq!(r.text(), "# Größe\nprint(1)\n");
    assert_eq!(r.app.cur_doc().hl.language.as_deref(), Some("Python"));
    r.key(Key::End, Modifiers::CTRL);
    r.typed("x");
    assert!(r.app.cur_doc().is_modified());
    r.key(Key::S, Modifiers::CTRL);
    r.frames(1);
    assert!(!r.app.cur_doc().is_modified());
    assert_eq!(std::fs::read(&f).unwrap(), b"# Gr\xF6\xDFe\r\nprint(1)\r\nx");
    // Dieselbe Datei nochmal öffnen: kein zweiter Tab
    assert!(r.app.open_path(&f).is_some());
    assert_eq!(r.app.panes[0].tabs.len(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn unmappable_characters_never_get_lost_silently() {
    let dir = temp_dir("lossy");
    let f = dir.join("a.txt");
    std::fs::write(&f, b"G\xF6\n").unwrap();
    let mut r = Rig::new();
    r.app.open_path(&f);
    r.frames(2);
    r.typed("日本");
    r.key(Key::S, Modifiers::CTRL);
    assert!(matches!(r.app.dialog, Some(crate::app::Dialog::LossySave { .. })));
    assert_eq!(std::fs::read(&f).unwrap(), b"G\xF6\n", "Datei bleibt unberührt");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn open_missing_file_shows_a_message_instead_of_failing() {
    let mut r = Rig::new();
    assert!(r.app.open_path(std::path::Path::new("Z:\\gibt\\es\\nicht.txt")).is_none());
    assert!(matches!(r.app.dialog, Some(crate::app::Dialog::Message { .. })));
    assert_eq!(r.app.panes[0].tabs.len(), 1);
}

#[test]
fn find_replace_count_mark() {
    let mut r = Rig::new();
    r.set_text("Katze Hund katze\nkatzen");
    r.key(Key::Home, Modifiers::CTRL);
    r.app.find.what = "katze".into();
    assert_eq!(r.app.count_matches(), 3);
    r.app.find.case = true;
    assert_eq!(r.app.count_matches(), 2);
    r.app.find.whole = true;
    assert_eq!(r.app.count_matches(), 1);
    r.app.find.whole = false;
    r.key(Key::F3, Modifiers::NONE);
    let s = r.app.cur_view().sels.main();
    assert_eq!((s.start(), s.end()), (11, 16));
    r.key(Key::F3, Modifiers::NONE);
    let s = r.app.cur_view().sels.main();
    assert_eq!((s.start(), s.end()), (17, 22));
    r.key(Key::F3, Modifiers::NONE);
    assert_eq!(r.app.cur_view().sels.main().start(), 11, "Am Ende von vorn");
    r.key(Key::F3, Modifiers::SHIFT);
    assert_eq!(r.app.cur_view().sels.main().start(), 17, "rückwärts auch");
    assert_eq!(r.app.mark_all(), 2);
    assert_eq!(r.app.cur_doc().marks, vec![(11, 16), (17, 22)]);
    r.app.find.with = "Maus".into();
    r.app.replace_all_current();
    r.frames(1);
    assert_eq!(r.text(), "Katze Hund Maus\nMausn");
    r.key(Key::Z, Modifiers::CTRL);
    assert_eq!(r.text(), "Katze Hund katze\nkatzen", "Alle ersetzen ist ein einziger Schritt");
}

#[test]
fn replace_one_replaces_only_a_real_hit_then_moves_on() {
    let mut r = Rig::new();
    r.set_text("aa aa");
    r.key(Key::Home, Modifiers::CTRL);
    r.app.find.what = "aa".into();
    r.app.find.with = "b".into();
    r.app.replace_one();
    assert_eq!(r.text(), "aa aa", "erst finden");
    r.app.replace_one();
    assert_eq!(r.text(), "b aa");
    let s = r.app.cur_view().sels.main();
    assert_eq!((s.start(), s.end()), (2, 4));
}

#[test]
fn unicode_search_positions_are_correct() {
    let mut r = Rig::new();
    r.set_text("äöü 日本 ß");
    r.key(Key::Home, Modifiers::CTRL);
    r.app.find.what = "日本".into();
    assert!(r.app.find_step(false));
    let s = r.app.cur_view().sels.main();
    assert_eq!((s.start(), s.end()), (4, 6));
    r.app.find_all_docs(true);
    r.frames(1);
    assert!(r.app.results.open);
    let hit = r.app.results.rows.iter().find_map(|x| match x {
        crate::ui_panels::ResRow::Hit { hit, .. } => Some(hit.clone()),
        _ => None,
    });
    let hit = hit.unwrap();
    assert_eq!((hit.line, hit.col, hit.len), (0, 4, 2));
}

#[test]
fn bad_regex_shows_message_and_does_not_panic() {
    let mut r = Rig::new();
    r.set_text("x");
    r.app.find.what = "(".into();
    r.app.find.mode = crate::search::Mode::Regex;
    assert!(!r.app.find_step(false));
    assert!(r.app.find.message_err);
}

#[test]
fn find_in_files_runs_in_background_and_jumps() {
    let dir = temp_dir("files");
    std::fs::write(dir.join("a.txt"), "erste\nNadel hier\n").unwrap();
    std::fs::write(dir.join("b.rs"), "Nadel\n").unwrap();
    std::fs::write(dir.join("c.bin"), [0u8, 1, 2, b'N', b'a', b'd', b'e', b'l']).unwrap();
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub").join("d.txt"), "Nadel\n").unwrap();
    let mut r = Rig::new();
    r.app.find.what = "Nadel".into();
    r.app.find.dir = dir.display().to_string();
    r.app.find.filters = "*.txt".into();
    r.app.find.case = true;
    r.app.start_file_job(None);
    for _ in 0..500 {
        r.frames(1);
        if r.app.find_job.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(r.app.find_job.is_none());
    let files = r.app.results.rows.iter().filter(|x| matches!(x, crate::ui_panels::ResRow::File { .. })).count();
    let hits: Vec<_> = r.app.results.rows.iter().filter_map(|x| match x {
        crate::ui_panels::ResRow::Hit { path, hit, .. } => Some((path.clone(), hit.clone())),
        _ => None,
    }).collect();
    assert_eq!(files, 2, "nur .txt, auch im Unterordner");
    assert_eq!(hits.len(), 2);
    let (p, h) = hits.iter().find(|(p, _)| p.as_ref().unwrap().ends_with("a.txt")).unwrap().clone();
    assert_eq!(h.line, 1);
    r.app.goto_hit(None, p, h.line, h.col, h.len);
    r.frames(2);
    assert_eq!(r.app.cur_doc().name(), "a.txt");
    let s = r.app.cur_view().sels.main();
    assert_eq!(r.app.cur_doc().slice(s.start(), s.end()), "Nadel");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn replace_in_files_changes_files_and_keeps_encoding() {
    let dir = temp_dir("repl");
    std::fs::write(dir.join("a.txt"), b"Gr\xF6\xDFe alt\r\n").unwrap();
    std::fs::write(dir.join("b.txt"), "ohne\n").unwrap();
    let mut r = Rig::new();
    r.app.find.what = "alt".into();
    r.app.find.with = "neu".into();
    r.app.find.dir = dir.display().to_string();
    r.app.find.filters = "*.txt".into();
    r.app.start_file_job(Some("neu".into()));
    for _ in 0..500 {
        r.frames(1);
        if r.app.find_job.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(std::fs::read(dir.join("a.txt")).unwrap(), b"Gr\xF6\xDFe neu\r\n");
    assert_eq!(std::fs::read(dir.join("b.txt")).unwrap(), b"ohne\n");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn bookmarks_navigate_and_delete_lines() {
    let mut r = Rig::new();
    r.set_text("a\nb\nc\nd");
    r.key(Key::Home, Modifiers::CTRL);
    r.key(Key::ArrowDown, Modifiers::NONE);
    r.key(Key::F2, Modifiers::CTRL);
    r.key(Key::ArrowDown, Modifiers::NONE);
    r.key(Key::ArrowDown, Modifiers::NONE);
    r.key(Key::F2, Modifiers::CTRL);
    assert_eq!(r.app.cur_doc().bookmarks.len(), 2);
    r.key(Key::F2, Modifiers::NONE);
    assert_eq!(r.head(), 2, "nächstes Lesezeichen, von vorn");
    r.key(Key::F2, Modifiers::SHIFT);
    assert_eq!(r.head(), 6);
    r.act(Action::BookmarksDeleteLines);
    assert_eq!(r.text(), "a\nc");
    r.key(Key::Z, Modifiers::CTRL);
    assert_eq!(r.text(), "a\nb\nc\nd");
}

#[test]
fn folding_hides_rows_and_caret_unfolds() {
    let mut r = Rig::new();
    r.set_text("kopf {\n    x\n    y\n}\nende");
    r.act(Action::FoldAll);
    assert!(r.app.cur_doc().is_hidden(1));
    let v = r.app.cur_view();
    assert_eq!(v.map.total_rows(r.app.cur_doc()), 2);
    r.act(Action::Cmd(Cmd::Move(Motion::DocStart, false)));
    r.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(r.app.cur_doc().text.char_to_line(r.head()), 4, "springt über den eingeklappten Block");
    r.app.goto_line(2);
    r.frames(2);
    assert!(!r.app.cur_doc().is_hidden(2), "Springen hinein klappt auf");
}

#[test]
fn word_wrap_breaks_long_lines_into_rows() {
    let mut r = Rig::new();
    r.act(Action::ToggleWrap);
    let long = "wort ".repeat(200);
    r.set_text(&long);
    r.frames(2);
    let v = r.app.cur_view();
    let rows = v.map.total_rows(r.app.cur_doc());
    assert!(rows > 5, "umbrochen in {rows} Reihen");
    // Beim Tippen bleibt die Zählung stimmig.
    r.key(Key::End, Modifiers::CTRL);
    r.typed("\u{e4}");
    r.key(Key::Enter, Modifiers::NONE);
    r.typed("neu");
    r.frames(2);
    let v = r.app.cur_view();
    assert_eq!(v.map.total_rows(r.app.cur_doc()), rows + 1);
    r.act(Action::ToggleWrap);
    assert_eq!(r.app.cur_view().map.total_rows(r.app.cur_doc()), 2);
}

#[test]
fn split_view_shares_text_and_keeps_its_own_caret() {
    let mut r = Rig::new();
    r.set_text("hallo");
    r.key(Key::End, Modifiers::CTRL);
    r.act(Action::CloneToOther);
    assert!(r.app.is_split());
    assert_eq!(r.app.active_pane, 1);
    r.act(Action::Cmd(Cmd::Move(Motion::DocStart, false)));
    r.typed("X");
    assert_eq!(r.text(), "Xhallo");
    r.frames(2);
    let left_head = r.app.panes[0].tabs[0].sels.main().head;
    assert_eq!(left_head, 6, "Cursor links rutscht mit");
    // Schließen rechts lässt das Dokument links bestehen, ohne Nachfrage.
    r.key(Key::W, Modifiers::CTRL);
    assert!(r.app.dialog.is_none());
    assert!(!r.app.is_split());
    assert_eq!(r.text(), "Xhallo");
}

#[test]
fn case_sort_comment_and_trim_commands() {
    let mut r = Rig::new();
    r.set_text("banane  \napfel");
    r.act(Action::Cmd(Cmd::SortLines { descending: false, ignore_case: false }));
    assert_eq!(r.text(), "apfel\nbanane  ");
    r.act(Action::Cmd(Cmd::TrimTrailing));
    assert_eq!(r.text(), "apfel\nbanane");
    r.act(Action::Cmd(Cmd::SelectAll));
    r.act(Action::Cmd(Cmd::Case(CaseKind::Upper)));
    assert_eq!(r.text(), "APFEL\nBANANE");
    r.act(Action::SetLanguage(Some("Python".into())));
    r.act(Action::Cmd(Cmd::Comment(None)));
    assert_eq!(r.text(), "# APFEL\n# BANANE");
    r.act(Action::Cmd(Cmd::Comment(None)));
    assert_eq!(r.text(), "APFEL\nBANANE");
}

#[test]
fn autocomplete_offers_words_from_the_document() {
    let mut r = Rig::new();
    r.set_text("Haustür Hausboot H");
    r.key(Key::End, Modifiers::CTRL);
    r.key(Key::Space, Modifiers::CTRL);
    let items = r.app.cur_view().complete.as_ref().map(|c| c.items.clone()).unwrap_or_default();
    assert_eq!(items, vec!["Hausboot", "Haustür"]);
    r.key(Key::ArrowDown, Modifiers::NONE);
    r.key(Key::Enter, Modifiers::NONE);
    assert_eq!(r.text(), "Haustür Hausboot Haustür");
}

#[test]
fn encoding_menu_reinterprets_and_converts() {
    let dir = temp_dir("enc");
    let f = dir.join("a.txt");
    std::fs::write(&f, "Grüße".as_bytes()).unwrap();
    let mut r = Rig::new();
    r.app.open_path(&f);
    r.frames(2);
    assert_eq!(r.app.cur_doc().enc, Enc::Utf8);
    r.act(Action::Encode(Enc::Ansi));
    assert_eq!(r.text(), "GrÃ¼ÃŸe");
    r.act(Action::Encode(Enc::Utf8));
    assert_eq!(r.text(), "Grüße");
    r.act(Action::Convert(Enc::Utf16Le));
    assert!(r.app.cur_doc().is_modified());
    r.key(Key::S, Modifiers::CTRL);
    r.frames(1);
    assert_eq!(&std::fs::read(&f).unwrap()[..2], &[0xFF, 0xFE]);
    r.act(Action::SetEol(crate::fileio::Eol::Cr));
    assert!(r.app.cur_doc().is_modified());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn column_editor_inserts_numbers() {
    let mut r = Rig::new();
    r.set_text("ab\nc\ndef");
    r.key(Key::Home, Modifiers::CTRL);
    r.act(Action::Cmd(Cmd::Move(Motion::Right, false)));
    r.act(Action::Cmd(Cmd::Move(Motion::Right, false)));
    r.app.column_insert(|i, n| crate::ui_panels::column_number(1, 2, 1, i, n, 0, 0));
    r.frames(2);
    assert_eq!(r.text(), "ab1\nc 3\nde5f");
    r.key(Key::Z, Modifiers::CTRL);
    assert_eq!(r.text(), "ab\nc\ndef");
}

#[test]
fn big_document_stays_quick_to_draw_and_navigate() {
    let mut r = Rig::new();
    let big: String = (0..200_000).map(|i| format!("Zeile Nummer {i}\n")).collect();
    r.act(Action::Cmd(Cmd::Paste(big)));
    r.frames(2);
    let t0 = std::time::Instant::now();
    r.key(Key::End, Modifiers::CTRL);
    r.typed("x");
    r.key(Key::Home, Modifiers::CTRL);
    r.frames(10);
    assert!(t0.elapsed().as_secs_f32() < 5.0, "dauerte {:?}", t0.elapsed());
    assert_eq!(r.app.cur_doc().lines(), 200_001);
}

#[test]
fn session_restores_tabs_and_unsaved_text() {
    let dir = temp_dir("session");
    let storage = crate::settings::Storage { dir: dir.join("data") };
    let saved = dir.join("datei.txt");
    std::fs::write(&saved, "von Platte\n").unwrap();
    {
        let ctx = Context::default();
        let app = App::new(ctx.clone(), Some(storage.clone()), Default::default(), Vec::new());
        let mut r = Rig { ctx, app, time: 0.0, copied: None };
        r.frames(2);
        r.typed("nicht gespeichert");
        r.app.open_path(&saved);
        r.frames(2);
        r.key(Key::End, Modifiers::CTRL);
        r.typed("++");
        r.app.save_session();
    }
    let ctx = Context::default();
    let app = App::new(ctx.clone(), Some(storage.clone()), Default::default(), Vec::new());
    let mut r = Rig { ctx, app, time: 0.0, copied: None };
    r.frames(2);
    assert_eq!(r.app.panes[0].tabs.len(), 2);
    let texts: Vec<String> = r.app.panes[0].tabs.iter().map(|v| r.app.docs[&v.doc].full_text()).collect();
    assert_eq!(texts[0], "nicht gespeichert");
    assert_eq!(texts[1], "von Platte\n++");
    assert!(r.app.docs.values().all(|d| d.is_modified()), "Wiederhergestelltes zählt als ungespeichert");
    assert_eq!(std::fs::read(&saved).unwrap(), b"von Platte\n", "Datei selbst bleibt unberührt");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn external_change_prompts_and_reload_works() {
    let dir = temp_dir("watch");
    let f = dir.join("a.txt");
    std::fs::write(&f, "alt").unwrap();
    let mut r = Rig::new();
    r.app.open_path(&f);
    r.frames(2);
    r.app.reload_doc(r.app.cur_id());
    assert_eq!(r.text(), "alt");
    std::fs::write(&f, "ganz neu").unwrap();
    r.app.reload_doc(r.app.cur_id());
    r.frames(2);
    assert_eq!(r.text(), "ganz neu");
    assert!(!r.app.cur_doc().is_modified());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn reload_of_a_changed_document_asks_first() {
    let dir = temp_dir("reload");
    let f = dir.join("a.txt");
    std::fs::write(&f, "alt").unwrap();
    let mut r = Rig::new();
    r.app.open_path(&f);
    r.frames(2);
    r.typed("x");
    r.act(Action::Reload);
    assert!(matches!(r.app.dialog, Some(crate::app::Dialog::ReloadAsk { .. })));
    assert!(r.text().contains('x'), "noch nichts verworfen");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Ohne Druckschrift auf dem System lassen sich die Drucktests nicht ausführen.
fn print_font_missing(r: &Rig) -> bool {
    let missing = crate::print::font::PrintFont::load(&r.app.settings.font).is_none();
    if missing {
        eprintln!("keine Druckschrift gefunden, Test übersprungen");
    }
    missing
}

fn print_ui(r: &mut Rig) -> &mut crate::ui_print::PrintUi {
    match &mut r.app.dialog {
        Some(crate::app::Dialog::Print(ui)) => ui,
        _ => panic!("Druckfenster ist nicht offen"),
    }
}

/// Frames, bis die Seiten zum aktuellen Stand fertig sind (höchstens 200).
fn wait_print_pages(r: &mut Rig) {
    for _ in 0..200 {
        r.frames(1);
        if print_ui(r).ready() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("Seiten wurden nicht fertig");
}

fn print_texts(r: &mut Rig) -> Vec<String> {
    let ui = print_ui(r);
    ui.shown().iter().flat_map(|p| &p.runs).map(|x| x.text.clone()).collect()
}

#[test]
fn print_dialog_opens_and_builds_pages() {
    let mut r = Rig::new();
    if print_font_missing(&r) {
        return;
    }
    r.app.settings.theme = crate::theme::ThemeId::Monokai;
    r.app.style_dirty = true;
    r.set_text("#!/bin/bash\necho hi\n");
    r.act(Action::Print);
    wait_print_pages(&mut r);
    let ui = print_ui(&mut r);
    let pages = ui.pages.as_ref().expect("Seiten");
    assert!(!pages.is_empty());
    let run = pages[0].runs.iter().find(|x| x.text.contains("echo")).expect("Run mit echo");
    let s = crate::theme::paper_syntax(false);
    let paper = [s.fg, s.comment, s.string, s.number, s.keyword, s.typ, s.pre, s.op, s.func, s.bad, s.attr, s.inserted];
    let rgb = |h: u32| [(h >> 16) as u8, (h >> 8) as u8, h as u8];
    assert_ne!(run.color, [0, 0, 0], "Bash ist erkannt und eingefärbt");
    assert!(paper.iter().any(|&c| rgb(c) == run.color), "Farbe {:?} stammt nicht vom Papier", run.color);
}

#[test]
fn print_selection_keeps_line_numbers() {
    let mut r = Rig::new();
    if print_font_missing(&r) {
        return;
    }
    r.set_text("eins\nzwei\ndrei");
    r.app.cur_view_mut().sels = SelSet::range(5, 10);
    assert!(r.app.can_print_selection());
    r.act(Action::Print);
    print_ui(&mut r).sel = crate::ui_print::PageSel::Selection;
    wait_print_pages(&mut r);
    let ui = print_ui(&mut r);
    let runs: Vec<&crate::print::Run> = ui.shown().iter().flat_map(|p| &p.runs).collect();
    let nums: Vec<&str> = runs
        .iter()
        .filter(|x| x.color == crate::print::GRAY && x.text.chars().all(|c| c.is_ascii_digit()))
        .map(|x| x.text.as_str())
        .collect();
    assert_eq!(nums, ["2"]);
    assert!(runs.iter().any(|x| x.text == "zwei"));
    assert!(!runs.iter().any(|x| x.text.contains("eins") || x.text.contains("drei")));
}

#[test]
fn print_selection_disabled_for_multiple_cursors() {
    let mut r = Rig::new();
    r.set_text("abc\ndef");
    r.app.cur_view_mut().sels = SelSet::range(0, 2);
    assert!(r.app.can_print_selection());
    r.app.cur_view_mut().sels = SelSet { sels: vec![crate::doc::Sel::new(0, 1), crate::doc::Sel::new(4, 5)], primary: 0 };
    assert!(!r.app.can_print_selection());
    r.app.cur_view_mut().sels = SelSet::single(1);
    assert!(!r.app.can_print_selection(), "leere Auswahl");
}

#[test]
fn print_page_range_keeps_total() {
    let mut r = Rig::new();
    if print_font_missing(&r) {
        return;
    }
    let text: String = (1..=200).map(|i| format!("Zeile {i}\n")).collect();
    r.set_text(&text);
    r.act(Action::Print);
    wait_print_pages(&mut r);
    let total = print_ui(&mut r).pages.as_ref().unwrap().len();
    assert!(total >= 2, "200 Zeilen brauchen mehrere Seiten");
    print_ui(&mut r).sel = crate::ui_print::PageSel::Range(2, 2);
    r.frames(2);
    let ui = print_ui(&mut r);
    assert_eq!(ui.shown().len(), 1);
    let footer = crate::t!("print.footer").replace("{page}", "2").replace("{total}", &total.to_string());
    assert!(ui.shown()[0].runs.iter().any(|x| x.text == footer), "Fuß {footer:?}");
}

#[test]
fn print_rebuilds_after_edit() {
    let mut r = Rig::new();
    if print_font_missing(&r) {
        return;
    }
    r.set_text("alpha");
    r.act(Action::Print);
    wait_print_pages(&mut r);
    assert!(print_texts(&mut r).iter().any(|t| t.contains("alpha")));
    let rev = r.app.cur_doc().rev;
    r.set_text("beta");
    assert_ne!(r.app.cur_doc().rev, rev);
    wait_print_pages(&mut r);
    let texts = print_texts(&mut r);
    assert!(texts.iter().any(|t| t.contains("beta")));
    assert!(!texts.iter().any(|t| t.contains("alpha")));
}

#[test]
fn print_preview_finishes_after_option_back_and_forth() {
    let mut r = Rig::new();
    if print_font_missing(&r) {
        return;
    }
    // Genug Zeilen, damit der Seitenaufbau noch läuft, wenn umgeschaltet wird.
    let text: String = (1..=30_000).map(|i| format!("Zeile {i} mit etwas Text\n")).collect();
    r.set_text(&text);
    r.act(Action::Print);
    wait_print_pages(&mut r);
    // Neue Schriftgröße startet einen Seitenaufbau; Farbe aus und gleich wieder an, bevor er fertig ist.
    let size = print_ui(&mut r).opts.font_size_pt + 2.0;
    print_ui(&mut r).opts.font_size_pt = size;
    r.frames(1);
    let ui = print_ui(&mut r);
    ui.opts.color = !ui.opts.color;
    r.frames(1);
    let ui = print_ui(&mut r);
    ui.opts.color = !ui.opts.color;
    r.frames(1);
    wait_print_pages(&mut r);
    let pages = print_ui(&mut r).pages.clone().expect("Seiten");
    assert!(pages[0].runs.iter().all(|x| x.size == size), "Seiten zur neuen Schriftgröße");
}

#[test]
fn print_layout_option_reuses_colored_lines() {
    let mut r = Rig::new();
    if print_font_missing(&r) {
        return;
    }
    r.set_text("fn main() {}\nlet x = 1;\n");
    r.act(Action::Print);
    wait_print_pages(&mut r);
    let before = print_ui(&mut r).src_lines().expect("Zeilen");
    let pages = print_ui(&mut r).pages.clone().expect("Seiten");
    let ui = print_ui(&mut r);
    ui.opts.line_numbers = !ui.opts.line_numbers;
    r.frames(1);
    wait_print_pages(&mut r);
    let ui = print_ui(&mut r);
    // Nur neu umbrochen, nicht neu eingefärbt.
    assert!(std::sync::Arc::ptr_eq(&before, &ui.src_lines().expect("Zeilen")));
    assert!(!std::sync::Arc::ptr_eq(&pages, ui.pages.as_ref().expect("Seiten")));
}

#[test]
fn empty_regex_match_moves_forward_instead_of_getting_stuck() {
    let mut r = Rig::new();
    r.set_text("a\nb\nc");
    r.key(Key::Home, Modifiers::CTRL);
    r.app.find.what = "^".into();
    r.app.find.mode = crate::search::Mode::Regex;
    let mut seen = Vec::new();
    for _ in 0..4 {
        assert!(r.app.find_step(false));
        r.frames(1);
        seen.push(r.head());
    }
    assert_eq!(seen, vec![2, 4, 0, 2], "läuft durch alle Zeilen und von vorn");
}

#[test]
fn huge_warning_starts_at_two_gib() {
    use crate::app::huge_warning_needed as h;
    assert!(!h(0));
    assert!(!h(1 << 30));
    assert!(!h((2 << 30) - 1));
    assert!(h(2 << 30));
    assert!(h(3 << 30));
}

#[test]
fn large_file_rules_turn_off_colors_wrap_and_helpers() {
    let dir = temp_dir("large");
    let f = dir.join("gross.py");
    std::fs::write(&f, "x = 1\nHaus haus\n").unwrap();
    let mut r = Rig::new();
    r.app.settings.large_file_mb = 1;
    assert!(r.app.is_large(1 << 20));
    assert!(!r.app.is_large((1 << 20) - 1));
    std::fs::write(&f, vec![b'a'; 1 << 20]).unwrap();
    r.act(Action::ToggleWrap);
    assert!(r.app.settings.ed.wrap);
    r.app.open_path(&f);
    r.frames(2);
    let d = r.app.cur_doc();
    assert!(d.large);
    assert!(d.hl.language.is_none(), "große Dateien bleiben ohne Farben");
    assert!(!r.app.settings.ed.wrap, "Zeilenumbruch wird abgeschaltet");
    r.key(Key::Space, Modifiers::CTRL);
    assert!(r.app.cur_view().complete.is_none(), "keine Vervollständigung");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn histories_keep_ten_entries() {
    let dir = temp_dir("hist");
    let mut r = Rig::new();
    for i in 0..12 {
        let f = dir.join(format!("{i}.txt"));
        std::fs::write(&f, "x").unwrap();
        r.app.open_path(&f);
    }
    assert_eq!(r.app.settings.recent.len(), crate::app::RECENT_MAX);
    assert_eq!(crate::app::RECENT_MAX, 10);
    r.set_text("x");
    for i in 0..12 {
        r.app.find.what = format!("w{i}");
        r.app.count_matches();
    }
    assert_eq!(r.app.find.history.len(), 10);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn zero_tab_size_falls_back_to_four() {
    let ctx = Context::default();
    let mut s = crate::settings::Settings::default();
    s.ed.tab_size = 0;
    let app = App::new(ctx, None, s, Vec::new());
    assert_eq!(app.settings.ed.tab_size, 4);
}

#[test]
fn column_numbers_follow_format_repeat_and_padding() {
    use crate::ui_panels::column_number as c;
    let list = |rep, lead, fmt, n: usize| (0..n).map(|i| c(8, 1, rep, i, n, lead, fmt)).collect::<Vec<_>>();
    assert_eq!(list(1, 1, 0, 3), ["08", "09", "10"]);
    assert_eq!(list(1, 2, 0, 3), [" 8", " 9", "10"]);
    assert_eq!(list(2, 0, 0, 4), ["8", "8", "9", "9"]);
    assert_eq!(c(255, 1, 1, 0, 1, 0, 1), "ff");
    assert_eq!(c(255, 1, 1, 0, 1, 0, 2), "FF");
    assert_eq!(c(8, 1, 1, 0, 1, 0, 3), "10");
    assert_eq!(c(5, 1, 1, 0, 1, 0, 4), "101");
}

#[test]
fn completion_appears_while_typing_except_for_numbers() {
    let mut r = Rig::new();
    r.set_text("Haustür 12345 ");
    r.key(Key::End, Modifiers::CTRL);
    r.typed("H");
    assert_eq!(r.app.cur_view().complete.as_ref().map(|c| c.items.clone()), Some(vec!["Haustür".to_string()]));
    r.key(Key::Enter, Modifiers::NONE);
    assert_eq!(r.text(), "Haustür 12345 Haustür");
    r.typed(" ");
    r.typed("1");
    assert!(r.app.cur_view().complete.is_none(), "reine Zahlen werden nicht vorgeschlagen");
    r.app.settings.ed.auto_complete = false;
    r.typed(" H");
    assert!(r.app.cur_view().complete.is_none());
}

#[test]
fn find_field_fills_from_the_word_at_the_caret() {
    let mut r = Rig::new();
    r.set_text("eins zwei");
    r.key(Key::Home, Modifiers::CTRL);
    r.act(Action::ShowFind(FindTab::Find));
    assert_eq!(r.app.find.what, "eins");
    assert!(!r.app.find.in_selection);
    r.set_text(&"a".repeat(500));
    r.act(Action::Cmd(Cmd::SelectAll));
    r.act(Action::ShowFind(FindTab::Replace));
    assert_eq!(r.app.find.what.len(), 500, "bis 500 Zeichen füllt die Markierung das Suchfeld");
    assert!(!r.app.find.in_selection, "eine Zeile ist kein Bereich");
    r.set_text("eins\nzwei\ndrei");
    r.act(Action::Cmd(Cmd::SelectAll));
    r.act(Action::ShowFind(FindTab::Replace));
    assert!(r.app.find.in_selection, "über mehrere Zeilen wird „Nur in der Markierung“ angehakt");
}

#[test]
fn replace_all_reports_read_only_and_non_ansi() {
    let dir = temp_dir("ansi");
    let f = dir.join("a.txt");
    std::fs::write(&f, b"G\xF6 alt").unwrap();
    let mut r = Rig::new();
    r.app.open_path(&f);
    r.frames(2);
    assert_eq!(r.app.cur_doc().enc, Enc::Ansi);
    r.app.find.what = "alt".into();
    r.app.find.with = "日本".into();
    r.app.replace_all_current();
    assert!(r.app.find.message.contains("ANSI"), "{}", r.app.find.message);
    assert!(r.text().contains("alt"));
    r.app.find.with = "neu".into();
    r.act(Action::ToggleReadOnly);
    r.app.replace_all_current();
    assert!(r.app.find.message.contains("schreibgeschützt"), "{}", r.app.find.message);
    r.act(Action::ToggleReadOnly);
    r.app.replace_all_current();
    assert!(r.text().contains("neu"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn file_search_sees_unsaved_text_and_honours_exclusions() {
    let dir = temp_dir("fif");
    std::fs::create_dir_all(dir.join("tests")).unwrap();
    std::fs::write(dir.join("offen.txt"), "nichts\n").unwrap();
    std::fs::write(dir.join("zu.log"), "Nadel\n").unwrap();
    std::fs::write(dir.join("tests").join("t.txt"), "Nadel\n").unwrap();
    std::fs::write(dir.join("roh.bin"), [0u8, b'N', b'a', b'd', b'e', b'l']).unwrap();
    let mut r = Rig::new();
    r.app.open_path(&dir.join("offen.txt"));
    r.frames(2);
    r.typed("Nadel");
    r.app.find.what = "Nadel".into();
    r.app.find.dir = dir.display().to_string();
    r.app.find.filters = "*.* !*.log !/tests/".into();
    r.app.find.open = true;
    r.app.start_file_job(None);
    for _ in 0..500 {
        r.frames(1);
        if r.app.find_job.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let names: Vec<String> = r.app.results.rows.iter().filter_map(|x| match x {
        crate::ui_panels::ResRow::File { name, .. } => Some(name.clone()),
        _ => None,
    }).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.iter().any(|n| n.ends_with("offen.txt")), "ungespeicherter Text zählt");
    assert!(names.iter().any(|n| n.ends_with("roh.bin")), "beim Suchen werden Binärdateien nicht ausgelassen");
    assert!(r.app.find.open, "die Seitenleiste bleibt für weitere Suchen offen");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn external_changes_ask_or_reload_depending_on_the_setting() {
    let dir = temp_dir("keep");
    let f = dir.join("a.txt");
    std::fs::write(&f, "alt").unwrap();
    let mut r = Rig::new();
    r.app.open_path(&f);
    r.frames(2);
    // Standard: Nachfrage.
    std::fs::write(&f, "neu").unwrap();
    r.app.cur_doc_mut().disk_mtime = Some(std::time::UNIX_EPOCH);
    r.app.check_disk_changes();
    assert!(matches!(r.app.dialog, Some(crate::app::Dialog::ReloadAsk { .. })));
    assert_eq!(r.text(), "alt");
    r.app.dialog = None;
    // Ohne Nachfrage: lädt neu, solange nichts verloren gehen kann.
    r.app.settings.auto_reload = true;
    r.app.cur_doc_mut().disk_mtime = Some(std::time::UNIX_EPOCH);
    r.app.check_disk_changes();
    assert!(r.app.dialog.is_none());
    assert_eq!(r.text(), "neu");
    // Mit ungespeicherten Änderungen wird trotzdem gefragt.
    r.frames(2);
    r.typed("x");
    r.app.cur_doc_mut().disk_mtime = Some(std::time::UNIX_EPOCH);
    r.app.check_disk_changes();
    assert!(matches!(r.app.dialog, Some(crate::app::Dialog::ReloadAsk { .. })));
    // Gelöschte Datei: Rückfrage, Text bleibt erhalten.
    r.app.dialog = None;
    std::fs::remove_file(&f).unwrap();
    r.app.check_disk_changes();
    assert!(matches!(r.app.dialog, Some(crate::app::Dialog::DeletedAsk { .. })));
    assert!(r.text().contains('x'));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn read_only_blocks_edits() {
    let mut r = Rig::new();
    r.set_text("x");
    r.act(Action::ToggleReadOnly);
    r.typed("y");
    r.key(Key::Backspace, Modifiers::NONE);
    assert_eq!(r.text(), "x");
    r.act(Action::ToggleReadOnly);
    r.typed("y");
    assert_ne!(r.text(), "x");
}

#[test]
fn all_dialogs_and_panels_draw_without_failing() {
    let mut r = Rig::new();
    r.set_text("fn main() {\n    let a = 1;\n}\n");
    r.act(Action::SetLanguage(Some("Rust".into())));
    for s in ["find", "replace", "files", "map", "funcs", "split", "prefs", "dark", "symbols", "wrap"] {
        r.app.apply_test_state(s);
        r.frames(3);
    }
    r.app.dialog = None;
    for a in [Action::About, Action::Shortcuts, Action::Summary, Action::GoToDialog, Action::ColumnEditor, Action::Hash(crate::app::HashKind::Sha256), Action::ShowFind(FindTab::Mark)] {
        r.act(a);
        r.frames(2);
        r.app.dialog = None;
    }
    assert_eq!(r.app.func_cache.as_ref().map(|c| c.2.len()), Some(1));
    let _ = SelSet::single(0);
}

#[test]
fn every_theme_switches_and_draws() {
    use crate::theme::ThemeId;
    let mut r = Rig::new();
    r.set_text("fn main() {\n    let a = 1; // Kommentar\n}\n");
    r.act(Action::SetLanguage(Some("Rust".into())));
    r.act(Action::CloneToOther);
    for t in ThemeId::ALL {
        r.act(Action::SetTheme(t));
        r.frames(3);
        assert_eq!(r.app.settings.theme, t);
        assert_eq!(r.app.palette().bg, crate::theme::palette(t).bg);
        assert_eq!(r.app.palette().scroll_bevel, t.retro());
    }
    r.act(Action::SetTheme(ThemeId::Light));
    r.frames(2);
    assert_eq!(r.text(), "fn main() {\n    let a = 1; // Kommentar\n}\n", "Designwechsel lässt den Text in Ruhe");
}

#[test]
fn find_bar_counts_live_and_shows_the_position() {
    let mut r = Rig::new();
    r.set_text("eins zwei Eins drei eins");
    r.act(Action::ShowFind(FindTab::Find));
    r.app.find.what = "eins".into();
    r.app.find.case = false;
    r.app.find.mode = crate::search::Mode::Normal;
    r.frames(2);
    assert_eq!(r.app.live_count().map(|x| x.map(|(_, n)| n)), Some(Ok(3)), "ohne Groß/Klein drei Treffer");
    assert!(r.app.find_step(false));
    let first = match r.app.live_count() {
        Some(Ok((Some(i), 3))) => i,
        other => panic!("Position erwartet: {other:?}"),
    };
    assert!(r.app.find_step(false));
    assert_eq!(r.app.live_count(), Some(Ok((Some(first % 3 + 1), 3))), "springt zum nächsten Treffer");
    r.app.find.case = true;
    assert_eq!(r.app.live_count().map(|x| x.map(|(_, n)| n)), Some(Ok(2)), "mit Groß/Klein nur zwei");
    r.app.find.mode = crate::search::Mode::Regex;
    r.app.find.what = "(".into();
    assert!(matches!(r.app.live_count(), Some(Err(_))), "ungültiger Ausdruck wird gemeldet");
    r.app.find.what.clear();
    assert_eq!(r.app.live_count(), None);
    r.frames(2);
}

/// Die berechneten Spaltenpositionen (Mausklick, Markierung, Cursor) müssen dort liegen, wo
/// egui die Zeichen tatsächlich zeichnet, auch bei Bildschirmskalierung und langen Zeilen.
#[test]
fn column_positions_match_the_drawn_glyphs() {
    let line = "VAR=$(ls -la | wc -l)  # eine recht lange Zeile mit vielen Zeichen zum Testen der Markierung WWWW iiii ${XDG_CONFIG_HOME:-$HOME/.config}";
    for ppp in [1.0_f32, 1.25, 1.5, 1.75] {
        let mut r = Rig::new();
        r.ctx.set_pixels_per_point(ppp);
        r.frames(3);
        r.set_text(line);
        r.frames(2);
        let font = r.app.metrics.as_ref().unwrap().font.clone();
        let galley = r.ctx.fonts(|f| f.layout_no_wrap(line.to_string(), font, egui::Color32::WHITE));
        let glyphs = &galley.rows[0].glyphs;
        let doc = r.app.docs.get(&r.app.cur_view().doc).unwrap();
        let o = r.app.settings.ed.clone();
        let m = r.app.metrics.as_mut().unwrap();
        assert_eq!(m.ppp, ppp, "Maße folgen der Skalierung");
        for (k, g) in glyphs.iter().enumerate() {
            let x = crate::editor::line_pos_x(doc, &o, m, k);
            assert!((x - g.pos.x).abs() < 0.01, "ppp {ppp}, Spalte {k}: berechnet {x}, gezeichnet {}", g.pos.x);
        }
    }
}

#[test]
fn new_documents_get_their_language_from_the_first_line() {
    let mut r = Rig::new();
    assert_eq!(r.app.cur_doc().hl.language, None);
    r.typed("#!/bin/bash");
    r.key(Key::Enter, Modifiers::NONE);
    r.typed("echo hallo");
    assert_eq!(r.app.cur_doc().hl.language.as_deref(), Some("Bourne Again Shell (bash)"), "beim Tippen erkannt");
    r.set_text("<?xml version=\"1.0\"?>\n<a/>");
    assert_eq!(r.app.cur_doc().hl.language.as_deref(), Some("XML"), "beim Einfügen erkannt");
    r.set_text("nur Text");
    assert_eq!(r.app.cur_doc().hl.language, None, "ohne Hinweis wieder normaler Text");
    r.act(Action::SetLanguage(Some("Python".into())));
    r.set_text("#!/bin/sh\necho");
    assert_eq!(r.app.cur_doc().hl.language.as_deref(), Some("Python"), "von Hand gewählt bleibt gewählt");
}

#[test]
fn csv_file_gets_dialect_on_open() {
    let dir = temp_dir("csvopen");
    let a = dir.join("x.csv");
    let b = dir.join("x.txt");
    std::fs::write(&a, b"a;b\n1;2\n").unwrap();
    std::fs::write(&b, b"a;b\n1;2\n").unwrap();
    let mut r = Rig::new();
    r.app.open_path(&a);
    r.frames(2);
    let c = r.app.cur_doc().csv.as_ref().expect("Tabellen-Dokument");
    assert_eq!(c.dialect, crate::csv::Dialect { delim: ';', header: true });
    r.app.open_path(&b);
    r.frames(2);
    assert!(r.app.cur_doc().csv.is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn table_edit_is_one_undo_step() {
    let dir = temp_dir("csvundo");
    let f = dir.join("x.csv");
    std::fs::write(&f, b"a,b\n1,2\n3,4\n").unwrap();
    let mut r = Rig::new();
    r.app.open_path(&f);
    r.frames(2);
    let before = r.text();
    let d = r.app.cur_doc_mut();
    let dialect = d.csv.as_ref().unwrap().dialect;
    let rev = d.rev;
    let (p, chars) = d.csv.as_mut().unwrap().parsed(&d.text, rev);
    let mut ch = crate::csv::set_cell(&chars, &p, dialect, false, 1, 0, "X");
    ch.extend(crate::csv::set_cell(&chars, &p, dialect, false, 2, 1, "Y"));
    assert!(crate::csv::apply_changes(d, ch, &SelSet::single(0), 0));
    assert_eq!(d.full_text(), "a,b\nX,2\n3,Y\n");
    assert!(d.is_modified());
    assert!(d.undo().is_some());
    assert_eq!(d.full_text(), before);
    assert!(!d.is_modified());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn three_cx_cell_edit_saves_bytewise() {
    let dir = temp_dir("csv3cx");
    let f = dir.join("kontakte.csv");
    let orig: &[u8] = b"\xEF\xBB\xBF\"FirstName\",\"Mobile\"\r\n\"Anna\",\"+49 1\"\r\n\"Max\",\"+43 2\"\r\n";
    std::fs::write(&f, orig).unwrap();
    let mut r = Rig::new();
    r.app.open_path(&f);
    r.frames(2);
    let d = r.app.cur_doc_mut();
    let dialect = d.csv.as_ref().unwrap().dialect;
    let rev = d.rev;
    let (p, chars) = d.csv.as_mut().unwrap().parsed(&d.text, rev);
    let ch = crate::csv::set_cell(&chars, &p, dialect, true, 2, 1, "+43 9");
    assert!(crate::csv::apply_changes(d, ch, &SelSet::single(0), 0));
    r.act(Action::Save);
    let expect = String::from_utf8(orig.to_vec()).unwrap().replace("+43 2", "+43 9");
    assert_eq!(std::fs::read(&f).unwrap(), expect.as_bytes());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn noop_change_does_not_mark_modified() {
    let mut r = Rig::new();
    let d = r.app.cur_doc_mut();
    assert!(!crate::csv::apply_changes(d, vec![], &SelSet::single(0), 0));
    assert!(!d.is_modified());
}

#[test]
fn csv_cell_positions() {
    let p = crate::csv::parse("a,\"b c\"\nd,e\n", ',');
    assert_eq!(crate::csv::cell_at(&p, 0), (0, 0));
    assert_eq!(crate::csv::cell_at(&p, 4), (0, 1));
    assert_eq!(crate::csv::cell_at(&p, 9), (1, 0));
    assert_eq!(crate::csv::cell_at(&p, 999), (1, 1));
    assert_eq!(crate::csv::cell_pos(&p, 0, 1), 3);
    assert_eq!(crate::csv::cell_pos(&p, 1, 0), 8);
}

/// Öffnet `text` als CSV-Datei in einem eigenen Ordner.
fn open_csv(r: &mut Rig, tag: &str, name: &str, text: &str) -> std::path::PathBuf {
    let dir = temp_dir(tag);
    let f = dir.join(name);
    std::fs::write(&f, text).unwrap();
    r.app.open_path(&f);
    r.frames(3);
    dir
}

/// Aktive Zelle der Tabelle im aktiven Bereich.
fn table_cell(r: &mut Rig) -> (usize, usize) {
    let pane = r.app.active_pane;
    table_cell_in(r, pane)
}

fn table_cell_in(r: &mut Rig, pane: usize) -> (usize, usize) {
    let p = &mut r.app.panes[pane];
    let v = &mut p.tabs[p.active];
    let d = r.app.docs.get_mut(&v.doc).unwrap();
    crate::ui_table::active_cell(d, v)
}

#[test]
fn csv_opens_in_table_and_typing_edits_cell() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tbltype", "t.csv", "a,b\n1,2\n");
    assert!(r.app.cur_view().table.is_some(), "CSV startet in der Tabelle");
    r.key(Key::ArrowDown, Modifiers::NONE);
    r.typed("9");
    assert_eq!(r.text(), "a,b\n1,2\n", "erst Enter übernimmt");
    r.key(Key::Enter, Modifiers::NONE);
    assert_eq!(r.text(), "a,b\n9,2\n");
    assert_eq!(r.app.cur_doc().undo.len(), 1, "ein Rückgängig-Schritt");
    r.key(Key::Z, Modifiers::CTRL);
    assert_eq!(r.text(), "a,b\n1,2\n", "Strg+Z wirkt in der Tabelle");
    assert_eq!(table_cell(&mut r), (1, 0), "die geänderte Zelle ist wieder aktiv");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn f2_edits_cell_not_bookmark() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblf2", "t.csv", "a,b\n1,2\n3,4\n");
    r.app.cur_doc_mut().bookmarks.insert(2);
    r.key(Key::ArrowRight, Modifiers::NONE);
    let before = table_cell(&mut r);
    assert_eq!(before, (1, 1));
    r.key(Key::F2, Modifiers::NONE);
    assert_eq!(table_cell(&mut r), before, "F2 springt nicht zum Lesezeichen");
    r.typed("X");
    r.key(Key::Enter, Modifiers::NONE);
    assert_eq!(r.text(), "a,b\n1,2X\n3,4\n", "F2 bearbeitet den vorhandenen Inhalt");
    assert_eq!(table_cell(&mut r), (2, 1), "Enter geht nach unten");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn ctrl_l_in_table_keeps_lines() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblctrll", "t.csv", "a,b\n1,2\n3,4\n");
    r.key(Key::L, Modifiers::CTRL);
    r.key(Key::D, Modifiers::CTRL);
    assert_eq!(r.text(), "a,b\n1,2\n3,4\n", "Zeilenbefehle der Textansicht wirken nicht");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn split_view_follows_table_edit_and_undo() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblsplit", "t.csv", "a,b\n1,2\n3,4\n");
    r.act(Action::CloneToOther);
    assert!(r.app.panes[1].tabs[0].table.is_none(), "rechts Text");
    assert!(r.app.panes[0].tabs[0].table.is_some(), "links Tabelle");
    r.app.active_pane = 0;
    r.app.panes[0].tabs[0].want_focus = true;
    r.frames(2);
    r.typed("9");
    r.key(Key::Enter, Modifiers::NONE);
    assert_eq!(r.text(), "a,b\n9,2\n3,4\n");
    assert_eq!(table_cell_in(&mut r, 0), (2, 0));
    let right = r.app.panes[1].tabs[0].doc;
    assert_eq!(r.app.docs[&right].full_text(), "a,b\n9,2\n3,4\n", "rechts derselbe Text");
    r.app.active_pane = 1;
    r.app.panes[1].tabs[0].want_focus = true;
    r.frames(2);
    r.key(Key::Z, Modifiers::CTRL);
    assert_eq!(r.text(), "a,b\n1,2\n3,4\n", "Rückgängig in der Textansicht");
    r.frames(2);
    assert_eq!(table_cell_in(&mut r, 0), (1, 0), "Tabelle zeigt die geänderte Zelle");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn toggle_table_switches_mode() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tbltoggle", "t.txt", "a;b\n1;2\n");
    assert!(r.app.cur_view().table.is_none());
    assert!(r.app.cur_doc().csv.is_none());
    r.act(Action::ToggleTable);
    assert!(r.app.cur_view().table.is_some(), "über das Menü für jedes Dokument");
    assert_eq!(r.app.cur_doc().csv.as_ref().unwrap().dialect.delim, ';');
    r.key(Key::ArrowRight, Modifiers::NONE);
    assert_eq!(table_cell(&mut r), (1, 1));
    r.act(Action::ToggleTable);
    assert!(r.app.cur_view().table.is_none(), "zurück zum Text");
    r.typed("x");
    assert_eq!(r.text(), "a;b\n1;x2\n", "Textansicht hat den Fokus und die Stelle");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn large_doc_has_no_table() {
    let mut r = Rig::new();
    r.app.settings.large_file_mb = 1;
    let mut big = String::from("a,b\n");
    while big.len() < (1 << 20) {
        big.push_str("1,2\n");
    }
    let dir = open_csv(&mut r, "tbllarge", "big.csv", &big);
    assert!(r.app.cur_doc().large);
    assert!(r.app.cur_view().table.is_none(), "große Datei: keine Tabelle");
    r.act(Action::ToggleTable);
    assert!(r.app.cur_view().table.is_none(), "auch nicht über das Menü");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn find_in_table_selects_cell() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblfind", "t.csv", "a,b\n1,2\n");
    r.key(Key::F, Modifiers::CTRL);
    assert!(r.app.find.open);
    r.app.find.what = "2".into();
    r.app.find.mode = crate::search::Mode::Normal;
    r.key(Key::F3, Modifiers::NONE);
    assert_eq!(table_cell(&mut r), (1, 1));
    assert_eq!(r.text(), "a,b\n1,2\n");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn table_edit_keys_tab_escape_and_alt_enter() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblkeys", "t.csv", "a,b,c\n1,2,3\n");
    r.typed("x");
    r.key(Key::Tab, Modifiers::NONE);
    assert_eq!(r.text(), "a,b,c\nx,2,3\n");
    assert_eq!(table_cell(&mut r), (1, 1), "Tab geht nach rechts");
    r.typed("weg");
    r.key(Key::Escape, Modifiers::NONE);
    assert_eq!(r.text(), "a,b,c\nx,2,3\n", "Esc verwirft");
    assert_eq!(table_cell(&mut r), (1, 1));
    r.key(Key::F2, Modifiers::NONE);
    r.key(Key::Enter, Modifiers::ALT);
    r.typed("y");
    r.key(Key::Enter, Modifiers::NONE);
    assert_eq!(r.text(), "a,b,c\nx,\"2\ny\",3\n", "Alt+Enter: Zeilenumbruch im Feld, mit Anführungszeichen");
    r.key(Key::Delete, Modifiers::NONE);
    assert_eq!(r.text(), "a,b,c\nx,\"\",3\n", "Entf leert die Zelle, Stil des Feldes bleibt");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn clearing_a_large_selection_is_one_quick_step() {
    let mut r = Rig::new();
    let mut t = String::from("a,b,c\n");
    for i in 0..20_000 {
        t.push_str(&format!("{i},x{i},\"y\"\n"));
    }
    let dir = open_csv(&mut r, "tblclear", "t.csv", &t);
    let start = std::time::Instant::now();
    r.key(Key::A, Modifiers::CTRL);
    r.key(Key::Delete, Modifiers::NONE);
    let took = start.elapsed();
    let expect = format!("a,b,c\n{}", ",,\"\"\n".repeat(20_000));
    assert!(r.text() == expect, "alle Zellen leer, Anführungszeichen bleiben");
    assert_eq!(r.app.cur_doc().undo.len(), 1, "ein Rückgängig-Schritt");
    assert!(took.as_secs() < 20, "Leeren dauerte {took:?}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn undo_and_delete_in_one_frame_use_fresh_fields() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblundodel", "t.csv", "a,b\n1,2\n3,4\n");
    r.typed("xyz");
    r.key(Key::Enter, Modifiers::NONE);
    assert_eq!(r.text(), "a,b\nxyz,2\n3,4\n");
    let k = |key, modifiers| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers };
    r.frame_with(vec![k(Key::Z, Modifiers::CTRL), k(Key::Delete, Modifiers::NONE)]);
    r.frames(1);
    assert_eq!(r.text(), "a,b\n,2\n3,4\n", "Entf trifft die Zelle im zurückgenommenen Text");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn open_cell_edit_is_kept_on_save_and_find() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblsave", "t.csv", "a,b\n1,2\n3,4\n");
    r.typed("9");
    r.key(Key::S, Modifiers::CTRL);
    assert_eq!(std::fs::read_to_string(dir.join("t.csv")).unwrap(), "a,b\n9,2\n3,4\n", "Strg+S übernimmt die offene Bearbeitung");
    r.typed("7");
    r.app.find.what = "4".into();
    r.app.find.mode = crate::search::Mode::Normal;
    assert!(r.app.find_step(false));
    r.frames(2);
    assert_eq!(r.text(), "a,b\n7,2\n3,4\n", "Wert landet in der bearbeiteten Zelle");
    assert_eq!(table_cell(&mut r), (2, 1), "aktive Zelle bleibt beim Treffer");
    r.typed("8");
    r.act(Action::NextTab);
    assert_eq!(r.app.docs.values().find(|d| d.path.is_some()).unwrap().full_text(), "a,b\n7,2\n3,8\n", "Tabwechsel übernimmt");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn ctrl_f_during_cell_edit_moves_typing_to_the_find_field() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblctrlf", "t.csv", "a,b\n1,2\n");
    r.typed("9");
    r.key(Key::F, Modifiers::CTRL);
    r.frames(2);
    r.typed("xyz");
    assert_eq!(r.text(), "a,b\n9,2\n", "offene Bearbeitung übernommen, Suchtext nicht in der Zelle");
    assert!(r.app.find.what.ends_with("xyz"), "Suchfeld hat den Text: {:?}", r.app.find.what);
    assert!(r.app.cur_view().table.as_ref().unwrap().editing.is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

use crate::ui_table::TableCmd;

fn table_state(r: &mut Rig) -> &mut crate::ui_table::TableState {
    r.app.cur_view_mut().table.as_mut().expect("Tabellenansicht")
}

/// Klick auf die Überschrift von Spalte `col`.
fn click_head(r: &mut Rig, col: usize, mods: Modifiers) {
    let st = table_state(r);
    let x = st.head_rect.left() - st.scroll.x + st.widths[..col].iter().sum::<f32>() + st.widths[col] * 0.5;
    let p = pos2(x, st.head_rect.center().y);
    r.frame_mods(vec![Event::PointerMoved(p)], mods);
    r.frame_mods(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: mods }], mods);
    r.frame_mods(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: mods }], mods);
    r.frames(1);
}

#[test]
fn row_insert_delete_via_shortcuts() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblrows", "t.csv", "a,b\n1,2\n3,4\n");
    r.key(Key::ArrowRight, Modifiers::NONE);
    r.key(Key::Enter, Modifiers::CTRL);
    assert_eq!(r.text(), "a,b\n1,2\n,\n3,4\n", "Strg+Eingabe fügt unterhalb ein");
    assert_eq!(table_cell(&mut r), (2, 1), "neue Zeile ist aktiv, Spalte bleibt");
    assert_eq!(r.app.cur_doc().undo.len(), 1);
    r.key(Key::ArrowUp, Modifiers::NONE);
    r.key(Key::Delete, Modifiers::CTRL | Modifiers::SHIFT);
    assert_eq!(r.text(), "a,b\n,\n3,4\n", "Strg+Umschalt+Entf löscht die Zeile");
    assert_eq!(table_cell(&mut r), (1, 1));
    r.key(Key::ArrowDown, Modifiers::SHIFT);
    r.key(Key::Delete, Modifiers::CTRL | Modifiers::SHIFT);
    assert_eq!(r.text(), "a,b\n", "beide ausgewählten Zeilen, die Kopfzeile bleibt");
    r.key(Key::Delete, Modifiers::CTRL | Modifiers::SHIFT);
    assert_eq!(r.text(), "a,b\n", "Kopfzeile wird nie gelöscht");
    r.key(Key::Enter, Modifiers::CTRL);
    assert_eq!(r.text(), "a,b\n,\n", "nur Kopfzeile: erste Datenzeile entsteht");
    assert_eq!(r.app.cur_doc().undo.len(), 4, "jede Aktion ein Schritt");
    // Unter Windows kommt Strg+Umschalt+Entf als „Ausschneiden“ an, ohne Tastenereignis.
    if cfg!(windows) {
        crate::ui_chrome::DELETE_HELD.with(|h| h.set(true));
        r.frame_mods(vec![Event::Cut], Modifiers::CTRL | Modifiers::SHIFT);
        crate::ui_chrome::DELETE_HELD.with(|h| h.set(false));
        r.frames(1);
        assert_eq!(r.text(), "a,b\n", "auch als Ausschneiden-Ereignis");
        assert_eq!(r.app.cur_doc().undo.len(), 5);
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn ctrl_shift_x_in_table_does_not_delete_rows() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblcutx", "t.csv", "a,b\n1,2\n3,4\n");
    // Strg+Umschalt+X kommt ebenfalls als „Ausschneiden“ mit Strg+Umschalt an, aber ohne Entf.
    r.frame_mods(vec![Event::Cut], Modifiers::CTRL | Modifiers::SHIFT);
    r.frames(1);
    assert_eq!(r.text(), "a,b\n,2\n3,4\n", "schneidet die Zelle aus, löscht keine Zeile");
    assert_eq!(r.copied.as_deref(), Some("1"));
    // In der Zellbearbeitung gehört Ausschneiden dem Eingabefeld, auch mit gehaltenem Entf.
    r.key(Key::ArrowDown, Modifiers::NONE);
    r.typed("x");
    crate::ui_chrome::DELETE_HELD.with(|h| h.set(true));
    r.frame_mods(vec![Event::Cut], Modifiers::CTRL | Modifiers::SHIFT);
    crate::ui_chrome::DELETE_HELD.with(|h| h.set(false));
    r.frames(1);
    r.key(Key::Escape, Modifiers::NONE);
    assert_eq!(r.text(), "a,b\n,2\n3,4\n", "keine Zeile gelöscht");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn columns_include_the_header() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblcols", "t.csv", "a,b\n1,2\n");
    r.act(Action::Table(TableCmd::ColRight));
    assert_eq!(r.text(), "a,,b\n1,,2\n");
    assert_eq!(table_cell(&mut r), (1, 1));
    r.act(Action::Table(TableCmd::ColMoveRight));
    assert_eq!(r.text(), "a,b,\n1,2,\n");
    assert_eq!(table_cell(&mut r), (1, 2));
    r.act(Action::Table(TableCmd::DeleteCol));
    assert_eq!(r.text(), "a,b\n1,2\n");
    assert_eq!(table_cell(&mut r), (1, 1), "Nachbar links wird aktiv");
    r.act(Action::Table(TableCmd::ColLeft));
    assert_eq!(r.text(), "a,,b\n1,,2\n", "auch die Kopfzeile bekommt die Zelle");
    assert_eq!(r.app.cur_doc().undo.len(), 4);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn filter_and_sort_do_not_modify() {
    let mut r = Rig::new();
    let text = "n,m\nb,1\na,2\nc,3\n";
    let dir = open_csv(&mut r, "tblfilter", "t.csv", text);
    click_head(&mut r, 0, Modifiers::NONE);
    assert_eq!(table_state(&mut r).rows.sort, Some((0, crate::csv::SortDir::Asc)), "Klick sortiert");
    assert_eq!(table_state(&mut r).rows.rows, vec![2, 1, 3]);
    r.key(Key::L, Modifiers::CTRL | Modifiers::SHIFT);
    r.typed("3");
    assert_eq!(table_state(&mut r).rows.filter, "3", "Strg+Umschalt+L öffnet das Filterfeld");
    assert_eq!(table_state(&mut r).rows.rows, vec![3]);
    assert_eq!(r.text(), text);
    assert!(!r.app.cur_doc().is_modified(), "Filtern und Sortieren ändern nichts");
    click_head(&mut r, 1, Modifiers::CTRL);
    assert_eq!(table_state(&mut r).rows.sort, Some((0, crate::csv::SortDir::Asc)), "Strg+Klick sortiert nicht");
    assert_eq!(table_cell(&mut r).1, 1, "Strg+Klick wählt die Spalte");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn apply_order_is_one_undo_step() {
    let mut r = Rig::new();
    let text = "n\nb\na\nc";
    let dir = open_csv(&mut r, "tblorder", "t.csv", text);
    click_head(&mut r, 0, Modifiers::NONE);
    assert!(!r.app.table_cmd_state().unwrap().enabled(TableCmd::RowUp), "Verschieben gesperrt bei Sortierung");
    r.act(Action::Table(TableCmd::ApplyOrder));
    assert_eq!(r.text(), "n\na\nb\nc", "Datensätze in Ansichtsreihenfolge, ohne Zeilenende am Schluss");
    assert_eq!(r.app.cur_doc().undo.len(), 1);
    assert_eq!(table_state(&mut r).rows.sort, None, "danach keine Ansichtssortierung");
    r.key(Key::Z, Modifiers::CTRL);
    assert_eq!(r.text(), text);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn apply_order_disabled_while_filtered() {
    let mut r = Rig::new();
    let text = "n\nb\na\nc\n";
    let dir = open_csv(&mut r, "tblorderf", "t.csv", text);
    click_head(&mut r, 0, Modifiers::NONE);
    r.act(Action::Table(TableCmd::ShowFilter));
    r.typed("a");
    let s = r.app.table_cmd_state().unwrap();
    assert!(!s.enabled(TableCmd::ApplyOrder));
    assert!(s.enabled(TableCmd::ClearSort));
    r.act(Action::Table(TableCmd::ApplyOrder));
    assert_eq!(r.text(), text, "mit Filter keine Übernahme");
    assert_eq!(r.app.cur_doc().undo.len(), 0);
    r.act(Action::Table(TableCmd::ClearSort));
    assert_eq!(table_state(&mut r).rows.sort, None);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn row_insert_in_sorted_view_goes_next_to_the_record() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblsortins", "t.csv", "n\nb\na\nc\n");
    click_head(&mut r, 0, Modifiers::NONE);
    // Ansicht a, b, c; aktiv ist die erste Ansichtszeile (a, Datensatz 2).
    r.key(Key::ArrowUp, Modifiers::CTRL);
    assert_eq!(table_cell(&mut r), (2, 0));
    r.act(Action::Table(TableCmd::RowBelow));
    assert_eq!(r.text(), "n\nb\na\n\nc\n", "im Text direkt nach dem Datensatz");
    assert_eq!(table_state(&mut r).rows.rows, vec![2, 3, 1, 4], "in der Ansicht an der Einfügestelle");
    assert_eq!(table_cell(&mut r), (3, 0));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn copy_paste_block_roundtrip() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblclip", "t.csv", "a,b,c\n1,2,3\n4,\"5\t6\",7\n,,\n,,\n");
    r.key(Key::ArrowDown, Modifiers::SHIFT);
    r.key(Key::ArrowRight, Modifiers::SHIFT);
    r.frame_with(vec![Event::Copy]);
    r.frames(1);
    let clip = r.copied.clone().expect("Kopieren legt Text ab");
    assert_eq!(clip, "1\t2\n4\t\"5\t6\"");
    r.key(Key::ArrowDown, Modifiers::CTRL);
    r.key(Key::ArrowUp, Modifiers::NONE);
    r.frame_with(vec![Event::Paste(clip)]);
    r.frames(1);
    assert_eq!(r.text(), "a,b,c\n1,2,3\n4,\"5\t6\",7\n,1,2\n,4,5\t6\n", "Werte gleich; Tab ist im Komma-CSV kein Grund für Anführungszeichen");
    assert_eq!(r.app.cur_doc().undo.len(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn single_value_fills_selection() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblfill", "t.csv", "a,b,c\n1,2,3\n4,5,6\n7,8,9\n");
    r.key(Key::ArrowDown, Modifiers::SHIFT);
    r.key(Key::ArrowRight, Modifiers::SHIFT);
    r.frame_with(vec![Event::Paste("x".into())]);
    r.frames(1);
    assert_eq!(r.text(), "a,b,c\nx,x,3\nx,x,6\n7,8,9\n");
    assert_eq!(r.app.cur_doc().undo.len(), 1, "eine Gruppe");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn table_menu_only_with_table_and_tablefilter_state_draws() {
    let mut r = Rig::new();
    assert!(r.app.table_cmd_state().is_none(), "ohne Tabelle kein Menü");
    r.app.apply_test_state("tablefilter");
    r.frames(3);
    assert!(r.app.table_cmd_state().is_some());
    let st = table_state(&mut r);
    assert!(!st.rows.is_identity());
    assert!(st.rows.rows.len() < 7);
    assert!(!r.app.cur_doc().is_modified());
}

#[test]
fn csv_spans_carry_quote_state() {
    let cols = crate::theme::csv_columns(crate::theme::ThemeId::Light);
    let dim = crate::theme::csv_delim(crate::theme::ThemeId::Light);
    let l1: Vec<char> = "a,\"b\n".chars().collect();
    let (s1, col, q) = crate::highlight::csv_line_spans(&l1, 0, false, ',', cols, dim);
    assert_eq!((col, q), (1, true), "Zeile 1 endet im offenen Feld der zweiten Spalte");
    assert_eq!(s1.iter().map(|s| s.chars).sum::<u32>(), 5, "Abschnitte decken die Zeile ab");
    assert_eq!(s1[0].color, cols[0]);
    assert_eq!(s1[1].color, dim, "Trennzeichen gedämpft");
    assert_eq!(s1[2].color, cols[1]);
    let l2: Vec<char> = "c,d\",e\n".chars().collect();
    let (s2, col, q) = crate::highlight::csv_line_spans(&l2, col, q, ',', cols, dim);
    assert_eq!(s2[0].color, cols[1], "Zeile 2 setzt das Feld der zweiten Spalte fort");
    assert_eq!(s2[0].chars, 4, "Komma im Anführungszeichen ist kein Trennzeichen");
    assert_eq!(s2[1].color, dim);
    assert_eq!(s2[2].color, cols[2]);
    assert_eq!((col, q), (2, false));
    // Spaltenfarben wiederholen sich nach der Palette.
    let l3: Vec<char> = "0;1;2;3;4;5;6".chars().collect();
    let (s3, col, _) = crate::highlight::csv_line_spans(&l3, 0, false, ';', cols, dim);
    assert_eq!(col, 6);
    assert_eq!(s3.last().unwrap().color, cols[0]);
    // Das Dokument färbt über Zeilen hinweg und nach Wechsel des Trennzeichens neu.
    let text = ropey::Rope::from_str("a,\"b\nc,d\",e\nf;g\n");
    let mut hl = crate::highlight::DocHl::new(None);
    hl.set_csv(Some(','));
    assert!(hl.ensure(&text, 0, 3, std::time::Duration::from_secs(5)));
    assert_eq!(hl.spans(1).unwrap()[0].color, cols[1]);
    assert_eq!(hl.csv_col_at(&text, 7), Some(1), "„d“ steht in Spalte 2");
    assert_eq!(hl.csv_col_at(&text, 10), Some(2));
    assert_eq!(hl.spans(2).unwrap().len(), 1, "ohne Komma eine Spalte");
    hl.set_csv(Some(';'));
    assert!(hl.spans(2).is_none(), "neues Trennzeichen färbt neu");
    assert!(hl.ensure(&text, 0, 3, std::time::Duration::from_secs(5)));
    assert_eq!(hl.spans(2).unwrap().len(), 3);
}

fn csv_info(r: &mut Rig) -> crate::ui_table::CsvStatus {
    let p = &mut r.app.panes[r.app.active_pane];
    let v = &mut p.tabs[p.active];
    let d = r.app.docs.get_mut(&v.doc).unwrap();
    crate::ui_table::status_info(d, v).expect("CSV-Dokument")
}

/// Spaltenzahl, wie die Tabelle sie aus dem Dialekt des Dokuments zeigt.
fn csv_ncols(r: &mut Rig) -> usize {
    let d = r.app.cur_doc_mut();
    let rev = d.rev;
    let (p, _) = d.csv.as_mut().unwrap().parsed(&d.text, rev);
    p.records.iter().map(|x| x.fields.len()).max().unwrap_or(1)
}

#[test]
fn switching_delimiter_reparses() {
    let mut r = Rig::new();
    let src = "a;b;c\n1;2,5;3\n4;5;6\n";
    let dir = open_csv(&mut r, "tbldelim", "t.csv", src);
    let i = csv_info(&mut r);
    assert_eq!((csv_ncols(&mut r), i.rows, i.row), (3, 2, Some(1)));
    assert_eq!(i.name.as_deref(), Some("a"));
    let d = r.app.cur_doc().csv.as_ref().unwrap().dialect;
    assert_eq!(d.delim, ';');
    let ctx = r.ctx.clone();
    r.app.run(&ctx, Action::CsvDialect(crate::csv::Dialect { delim: ',', ..d }));
    assert_eq!(r.app.cur_doc().hl.csv_delim(), Some(','), "Textfarben folgen sofort, nicht erst im nächsten Bild");
    r.frames(2);
    assert_eq!(csv_ncols(&mut r), 2, "mit Komma andere Spaltenzahl");
    assert_eq!(r.text(), src, "Text unverändert");
    assert!(!r.app.cur_doc().is_modified());
    r.act(Action::CsvDialect(crate::csv::Dialect { delim: ',', header: false }));
    let i = csv_info(&mut r);
    assert_eq!(i.rows, 3, "ohne Kopfzeile zählt jede Zeile");
    assert_eq!(i.name, None);
    assert!(r.app.cur_doc().csv.as_ref().unwrap().manual);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn header_row_has_no_number() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblheadrow", "t.csv", "a,b\n1,2\n3,4\n");
    r.app.cur_view_mut().sels = SelSet::single(0);
    let i = csv_info(&mut r);
    assert_eq!((i.row, i.rows), (None, 2), "Kopfzeile ist keine Zeile 1");
    r.app.cur_view_mut().sels = SelSet::single(4);
    assert_eq!(csv_info(&mut r).row, Some(1));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn text_view_status_names_column_and_unclosed_quote() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tbltxtcol", "t.csv", "Name,Mobile\nAnna,\"+49\nBerta,1\n");
    r.act(Action::ToggleTable);
    assert!(r.app.cur_view().table.is_none());
    r.app.cur_view_mut().sels = SelSet::single(18);
    r.frames(2);
    let pos = r.app.cur_view().sels.main().head;
    assert_eq!(crate::ui_table::text_column(r.app.cur_doc_mut(), pos), Some((1, Some("Mobile".to_string()))));
    assert_eq!(csv_info(&mut r).unclosed, Some(2), "Zeile 2 im Text, ab 1 gezählt");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn table_toggle_keeps_language_colours_of_code() {
    let mut r = Rig::new();
    let dir = temp_dir("tblcode");
    let f = dir.join("a.rs");
    std::fs::write(&f, "// x\nfn main() {}\n").unwrap();
    r.app.open_path(&f);
    r.frames(2);
    r.act(Action::ToggleTable);
    assert!(r.app.cur_view().table.is_some());
    r.act(Action::ToggleTable);
    r.frames(2);
    let d = r.app.cur_doc();
    assert!(d.csv.is_some(), "Dialekt bleibt vom Umschalten");
    assert_eq!(d.hl.csv_delim(), None, "Rust bleibt Rust");
    assert_eq!(d.hl.language.as_deref(), Some("Rust"));
    let comment = crate::theme::syntax(crate::theme::ThemeId::Light).comment;
    let c = d.hl.spans(0).expect("gefärbt")[0].color;
    assert_eq!((c.r(), c.g(), c.b()), ((comment >> 16) as u8, (comment >> 8) as u8, comment as u8));

    // Normaler Text als Tabelle gezeigt: im Text Spaltenfarben.
    let g = dir.join("liste.txt");
    std::fs::write(&g, "a;b\n1;2\n").unwrap();
    r.app.open_path(&g);
    r.frames(2);
    r.act(Action::ToggleTable);
    r.act(Action::ToggleTable);
    r.frames(2);
    assert_eq!(r.app.cur_doc().hl.csv_delim(), Some(';'));
    std::fs::remove_dir_all(&dir).unwrap();
}

// --- Abschlussprüfung: Reihenfolge gleicher Startpositionen, Rückgängig mit weniger Zeilen ---

#[test]
fn paste_into_short_row_keeps_cell_order() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblpasteord", "t.csv", "h1,h2,h3,h4\n1,\n");
    r.key(Key::ArrowRight, Modifiers::NONE);
    assert_eq!(table_cell(&mut r), (1, 1));
    r.frame_with(vec![Event::Paste("a\tb\tc".into())]);
    r.frames(1);
    assert_eq!(r.text(), "h1,h2,h3,h4\n1,a,b,c\n", "Werte in Spaltenreihenfolge");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn fill_selection_into_short_row_keeps_cell_order() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblfillord", "t.csv", "h1,h2,h3,h4\n1,\n");
    r.key(Key::ArrowRight, Modifiers::NONE);
    r.key(Key::ArrowRight, Modifiers::SHIFT);
    r.key(Key::ArrowRight, Modifiers::SHIFT);
    r.frame_with(vec![Event::Paste("x".into())]);
    r.frames(1);
    assert_eq!(r.text(), "h1,h2,h3,h4\n1,x,x,x\n");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn paste_rows_at_end_without_newline_keeps_order() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblpasteend", "t.csv", "h1,h2\n1");
    r.key(Key::ArrowRight, Modifiers::NONE);
    r.frame_with(vec![Event::Paste("x\ny".into())]);
    r.frames(1);
    assert_eq!(r.text(), "h1,h2\n1,x\n,y", "erst die Zelle, dann die neue Zeile");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn undo_of_inserted_row_does_not_panic() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblundorow", "t.csv", "a,b\n1,2\n");
    r.key(Key::Enter, Modifiers::CTRL);
    assert_eq!(r.text(), "a,b\n1,2\n,\n");
    r.key(Key::Z, Modifiers::CTRL);
    assert_eq!(r.text(), "a,b\n1,2\n");
    assert_eq!(table_state(&mut r).rows.rows, vec![1]);
    r.key(Key::Y, Modifiers::CTRL);
    assert_eq!(r.text(), "a,b\n1,2\n,\n");
    assert_eq!(table_state(&mut r).rows.rows, vec![1, 2]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn undo_of_paste_with_new_rows_does_not_panic() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblundopaste", "t.csv", "a,b\n1,2\n");
    r.frame_with(vec![Event::Paste("x\ty\nq\tr\ns\tt".into())]);
    r.frames(1);
    assert_eq!(r.text(), "a,b\nx,y\nq,r\ns,t\n");
    r.key(Key::ArrowDown, Modifiers::CTRL);
    r.key(Key::Z, Modifiers::CTRL);
    assert_eq!(r.text(), "a,b\n1,2\n");
    assert_eq!(table_state(&mut r).rows.rows, vec![1]);
    assert_eq!(table_cell(&mut r).0, 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn menu_paste_copy_and_line_commands_in_table() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tblmenupaste", "t.csv", "a,b\n1,2\n3,4\n");
    r.act(Action::Cmd(Cmd::Paste("x\ty\nq\tr".into())));
    assert_eq!(r.text(), "a,b\nx,y\nq,r\n", "Einfügen aus dem Menü verteilt auf die Zellen");
    assert_eq!(r.app.cur_doc().undo.len(), 1);
    r.act(Action::Cmd(Cmd::SelectAll));
    r.act(Action::Cmd(Cmd::Copy));
    assert_eq!(r.copied.as_deref(), Some("x\ty\nq\tr"), "Kopieren aus dem Menü nimmt die Zellen");
    r.act(Action::Cmd(Cmd::DuplicateLines));
    r.act(Action::Cmd(Cmd::Case(CaseKind::Upper)));
    r.act(Action::Cmd(Cmd::Comment(None)));
    assert_eq!(r.text(), "a,b\nx,y\nq,r\n", "Zeilenbefehle der Textansicht wirken nicht in der Tabelle");
    r.act(Action::Cmd(Cmd::Cut));
    assert_eq!(r.text(), "a,b\n,\n,\n", "Ausschneiden leert die Zellen");
    r.act(Action::Cmd(Cmd::Undo));
    assert_eq!(r.text(), "a,b\nx,y\nq,r\n");
    r.act(Action::Cmd(Cmd::Undo));
    assert_eq!(r.text(), "a,b\n1,2\n3,4\n");
    assert_eq!(table_state(&mut r).rows.rows, vec![1, 2]);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Messung, kein Test: `cargo test --release -- --ignored --nocapture table_timing_100k`.
#[test]
#[ignore]
fn table_timing_100k() {
    use std::time::{Duration, Instant};
    let mut t = String::from("id,name,city,note\n1\n");
    for i in 2..100_000 {
        t.push_str(&format!("{i},\"Name {i}\",Stadt{},x{i}\n", i % 97));
    }
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tbltiming", "t.csv", &t);
    let median = |mut v: Vec<Duration>| {
        v.sort();
        v[v.len() / 2]
    };
    let mut cell = Vec::new();
    let mut missing = Vec::new();
    let mut row = Vec::new();
    let mut parse = Vec::new();
    let mut idle = Vec::new();
    for k in 0..7 {
        let s = Instant::now();
        r.frames(2);
        idle.push(s.elapsed());
        // Vorhandenes Feld (Datensatz 2, Spalte 0)
        r.app.cur_view_mut().sels = SelSet::single(cell_start(&mut r, 2, 0));
        r.frames(1);
        r.typed(&format!("{k}"));
        let s = Instant::now();
        r.key(Key::Enter, Modifiers::NONE);
        cell.push(s.elapsed());
        // Fehlendes Feld einer kurzen Zeile (Datensatz 1, Spalte 3)
        r.key(Key::ArrowUp, Modifiers::CTRL);
        r.key(Key::End, Modifiers::NONE);
        r.typed(&format!("{k}"));
        let s = Instant::now();
        r.key(Key::Escape, Modifiers::NONE);
        let esc = s.elapsed();
        r.typed(&format!("{k}"));
        let s = Instant::now();
        r.key(Key::Enter, Modifiers::NONE);
        missing.push(s.elapsed().saturating_sub(esc));
        r.key(Key::Z, Modifiers::CTRL);
        // Zeile einfügen
        let s = Instant::now();
        r.key(Key::Enter, Modifiers::CTRL);
        row.push(s.elapsed());
        r.key(Key::Z, Modifiers::CTRL);
        // Reines Neuparsen
        let d = r.app.cur_doc_mut();
        let c = d.csv.as_mut().unwrap();
        let mut fresh = crate::csv::DocCsv::new(c.dialect);
        let s = Instant::now();
        let _ = fresh.parsed(&d.text, d.rev);
        parse.push(s.elapsed());
    }
    println!("zwei leere Bilder:       {:?}", median(idle));
    println!("Zelle übernehmen:        {:?}", median(cell));
    println!("fehlendes Feld setzen:   {:?}", median(missing));
    println!("Zeile einfügen:          {:?}", median(row));
    println!("Parsen (DocCsv::parsed): {:?}", median(parse));
    std::fs::remove_dir_all(&dir).unwrap();
}

fn cell_start(r: &mut Rig, rec: usize, col: usize) -> usize {
    let d = r.app.cur_doc_mut();
    let rev = d.rev;
    let (p, _) = d.csv.as_mut().unwrap().parsed(&d.text, rev);
    crate::csv::cell_pos(&p, rec, col)
}

#[test]
fn text_views_do_not_parse_the_table() {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, "tbllazy", "t.csv", "a,b\n1,2\n3,4\n");
    r.act(Action::CloneToOther);
    assert!(r.app.panes[0].tabs[0].table.is_some(), "links Tabelle");
    // Links ein anderer Tab vorn: die Tabelle ist nicht zu sehen.
    r.app.active_pane = 0;
    r.act(Action::New);
    r.app.active_pane = 1;
    r.app.panes[1].tabs[0].want_focus = true;
    r.frames(2);
    r.typed("x");
    r.frames(3);
    let id = r.app.panes[1].tabs[0].doc;
    let d = &r.app.docs[&id];
    assert!(d.full_text().contains('x'), "getippt: {:?}", d.full_text());
    assert_ne!(d.csv.as_ref().unwrap().parsed_rev(), Some(d.rev), "Text getippt, aber keine sichtbare Tabelle: nicht geparst");
    // Die Tabelle wieder vorn: jetzt wird geparst.
    r.app.active_pane = 0;
    r.act(Action::PrevTab);
    let d = &r.app.docs[&id];
    assert_eq!(d.csv.as_ref().unwrap().parsed_rev(), Some(d.rev));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Öffnen, Bewegen, Tippen, Zeile einfügen, Rückgängig: nichts davon darf abbrechen.
fn table_roundtrip_without_panic(name: &str, text: &str) {
    let mut r = Rig::new();
    let dir = open_csv(&mut r, &format!("tblnp{name}"), "t.csv", text);
    assert!(r.app.cur_view().table.is_some(), "{name}: Tabelle");
    for (key, m) in [
        (Key::ArrowDown, Modifiers::NONE),
        (Key::ArrowRight, Modifiers::NONE),
        (Key::ArrowRight, Modifiers::NONE),
        (Key::End, Modifiers::NONE),
        (Key::ArrowDown, Modifiers::CTRL),
        (Key::Home, Modifiers::CTRL),
    ] {
        r.key(key, m);
    }
    r.key(Key::End, Modifiers::NONE);
    r.typed("q");
    r.key(Key::Enter, Modifiers::NONE);
    r.key(Key::Enter, Modifiers::CTRL);
    r.typed("w");
    r.key(Key::Tab, Modifiers::NONE);
    r.key(Key::Delete, Modifiers::NONE);
    let edited = r.text();
    let steps = r.app.cur_doc().undo.len();
    for _ in 0..steps {
        r.key(Key::Z, Modifiers::CTRL);
    }
    assert_eq!(r.text(), text, "{name}: alles zurückgenommen");
    for _ in 0..steps {
        r.key(Key::Y, Modifiers::CTRL);
    }
    assert_eq!(r.text(), edited, "{name}: alles wiederhergestellt");
    r.act(Action::Table(TableCmd::DeleteRows));
    r.key(Key::Z, Modifiers::CTRL);
    r.frames(2);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn table_survives_ragged_empty_unclosed_and_header_only_files() {
    table_roundtrip_without_panic("ragged", "a,b,c\n1\n2,3\n\n4,5,6,7\n");
    table_roundtrip_without_panic("empty", "");
    table_roundtrip_without_panic("unclosed", "a,b\n1,\"offen\n2,3\n");
    table_roundtrip_without_panic("headeronly", "Name,Phone\n");
    table_roundtrip_without_panic("headeronlynonl", "Name,Phone");
}

#[test]
fn reload_keeps_a_table_opened_by_hand() {
    // Eine .txt von Hand als Tabelle: nach dem Neuladen bleibt der Dialekt, sonst
    // stünde die Tabelle ohne CSV-Daten da und die Tabellenbefehle liefen ins Leere.
    let dir = temp_dir("reloadtbl");
    let f = dir.join("liste.txt");
    std::fs::write(&f, "a;b\n1;2\n").unwrap();
    let mut r = Rig::new();
    r.app.open_path(&f);
    r.frames(2);
    assert!(r.app.cur_doc().csv.is_none());
    r.act(Action::ToggleTable);
    let d = r.app.cur_doc().csv.as_ref().unwrap().dialect;
    r.act(Action::CsvDialect(crate::csv::Dialect { header: !d.header, ..d }));
    std::fs::write(&f, "a;b\n1;2\n3;4\n").unwrap();
    let id = r.app.cur_id();
    r.app.reload_doc(id);
    r.frames(2);
    let c = r.app.cur_doc().csv.as_ref().expect("Dialekt nach dem Neuladen noch da");
    assert_eq!(c.dialect, crate::csv::Dialect { header: !d.header, ..d }, "von Hand gewählter Dialekt bleibt");
    assert!(r.app.cur_view().table.is_some());
    std::fs::remove_dir_all(&dir).unwrap();
}
