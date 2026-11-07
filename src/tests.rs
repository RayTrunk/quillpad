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
}

impl Rig {
    fn new() -> Rig {
        // Die Tests prüfen deutsche Texte.
        crate::i18n::set_lang(crate::i18n::Lang::De);
        let ctx = Context::default();
        let app = App::new(ctx.clone(), None, Default::default(), Vec::new());
        let mut r = Rig { ctx, app, time: 0.0 };
        r.frames(3);
        r
    }

    fn frame_with(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 650.0))),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let app = &mut self.app;
        let _ = self.ctx.run(input, |ctx| app.frame(ctx));
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
        let mut r = Rig { ctx, app, time: 0.0 };
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
    let mut r = Rig { ctx, app, time: 0.0 };
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

#[test]
fn print_page_contains_escaped_colored_text() {
    let mut r = Rig::new();
    r.set_text("// a<b\nfn x() {}");
    r.act(Action::SetLanguage(Some("Rust".into())));
    let id = r.app.cur_id();
    let html = crate::app::print_html(r.app.docs.get_mut(&id).unwrap());
    assert!(html.contains("window.print()"));
    assert!(html.contains("a&lt;b"));
    let comment = format!("color:#{:06x}", crate::theme::syntax(crate::theme::ThemeId::Light).comment);
    assert!(html.to_lowercase().contains(&comment), "Kommentar in der Farbe des Designs");
    assert!(!html.contains("a<b"));
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
