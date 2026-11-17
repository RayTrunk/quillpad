//! Maus und Tastatur der Tabelle. Tasten werden aus der Eingabe genommen, damit sie
//! weder globale Kürzel noch das Schreibfeld erreichen.

use super::{col_width, content_chars, parsed, set_active, Ctx, Layout, Step, MAX_FIT_CH, MIN_CH, SAMPLE_RECORDS};
use crate::csv::{self, Parsed};
use crate::editor::Metrics;
use egui::{Event, Key, Response, Ui};
use std::sync::Arc;

/// Klicken, Ziehen (Auswahl, Spaltenrand) und Doppelklick.
pub(super) fn pointer(ui: &Ui, cx: &mut Ctx, lay: &Layout, resp: &Response, p: &mut Arc<Parsed>, chars: &mut Arc<Vec<char>>, m: &mut Metrics) {
    let Layout { rect, body, row_h, ncols, nrows, need_v, need_h, vbar, hbar, .. } = *lay;
    let ctx = ui.ctx().clone();
    // Maus
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let (pressed, down, mods) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.modifiers));
    if let Some(pt) = pointer {
        let in_head = pt.y >= rect.top() && pt.y < body.top() && pt.x >= body.left() && pt.x < body.right();
        let border = if in_head { lay.hit_border(&cx.st.widths, cx.st.scroll.x, pt.x) } else { None };
        if (border.is_some() || cx.st.resize.is_some()) && ui.rect_contains_pointer(rect) {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        let on_bar = (need_v && vbar.contains(pt)) || (need_h && hbar.contains(pt));
        if pressed && resp.is_pointer_button_down_on() && !on_bar {
            if cx.st.editing.is_some() {
                cx.commit(Step::Stay);
                (*p, *chars) = parsed(cx.doc);
            }
            resp.request_focus();
            if let Some(c) = border {
                cx.st.resize = Some((c, pt.x, cx.st.widths[c]));
            } else if body.contains(pt) || (pt.x < body.left() && pt.y >= body.top() && pt.y < body.bottom()) {
                let vr = ((pt.y - body.top() + cx.st.scroll.y) / row_h).floor().max(0.0) as usize;
                if vr < nrows {
                    if pt.x < body.left() {
                        // Nummer: ganze Zeile
                        let rec = cx.st.rows.rows[vr];
                        let ar = if mods.shift { cx.st.anchor.map_or(cx.active(p).0, |a| a.0) } else { rec };
                        cx.st.anchor = Some((ar, 0));
                        let rev = cx.doc.rev;
                        set_active(cx.v, cx.st, p, rev, rec, ncols - 1);
                    } else if let Some(c) = lay.hit_col(&cx.st.widths, cx.st.scroll.x, pt.x) {
                        cx.move_to(p, vr, c, mods.shift);
                        cx.st.sel_drag = true;
                    }
                }
            }
        } else if down && cx.st.resize.is_some() {
            if let Some((c, x0, w0)) = cx.st.resize {
                cx.st.widths[c] = (w0 + pt.x - x0).max(MIN_CH * m.ch);
            }
        } else if down && cx.st.sel_drag {
            if pt.y < body.top() {
                cx.st.scroll.y -= row_h * 0.5;
            } else if pt.y > body.bottom() {
                cx.st.scroll.y += row_h * 0.5;
            }
            let vr = ((pt.y.clamp(body.top(), body.bottom() - 1.0) - body.top() + cx.st.scroll.y) / row_h).floor().max(0.0) as usize;
            let c = lay.hit_col(&cx.st.widths, cx.st.scroll.x, pt.x.clamp(body.left(), body.right() - 1.0));
            if let Some(c) = c {
                let target = (cx.st.rows.rows.get(vr.min(nrows.saturating_sub(1))).copied(), c);
                if let (Some(rec), c) = target {
                    if cx.active(p) != (rec, c) {
                        cx.move_to(p, vr, c, true);
                    }
                }
            }
            ctx.request_repaint();
        }
        if resp.double_clicked() {
            if let Some(c) = border {
                let n = content_chars(p, chars, c, SAMPLE_RECORDS * 5, MAX_FIT_CH);
                cx.st.widths[c] = col_width(n, m);
            } else if body.contains(pt) {
                cx.start_edit(p, chars, None);
            }
        }
    }
    if !down {
        cx.st.sel_drag = false;
        cx.st.resize = None;
    }
}

/// Tastatur: außerhalb der Bearbeitung die Tabellentasten, in der Bearbeitung Enter, Tab und Esc.
#[allow(clippy::too_many_arguments)]
pub(super) fn keys(
    ui: &Ui,
    cx: &mut Ctx,
    resp: &Response,
    base_id: egui::Id,
    edit_id: egui::Id,
    p: &mut Arc<Parsed>,
    chars: &mut Arc<Vec<char>>,
    page_rows: usize,
) {
    let ctx = ui.ctx().clone();
    // Tastatur außerhalb der Bearbeitung
    if resp.has_focus() && cx.st.editing.is_none() {
        ctx.memory_mut(|mem| {
            mem.set_focus_lock_filter(
                base_id,
                egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true },
            )
        });
        let events = ctx.input_mut(|i| std::mem::take(&mut i.events));
        let mut rest = Vec::new();
        for ev in events {
            if cx.st.editing.is_some() {
                // Nach Bearbeitungsbeginn gehört der Rest dem Eingabefeld.
                rest.push(ev);
                continue;
            }
            let used = table_key(cx, p, chars, &ev, page_rows);
            if !used {
                rest.push(ev);
            }
        }
        ctx.input_mut(|i| {
            rest.append(&mut i.events);
            i.events = rest;
        });
    }

    // Tasten der Bearbeitung: Enter, Tab und Esc gehören der Tabelle, nicht dem Eingabefeld.
    if cx.st.editing.is_some() {
        let edit_focus = ctx.memory(|mem| mem.has_focus(edit_id));
        let nobody = ctx.memory(|mem| mem.focused().is_none());
        if edit_focus || cx.st.edit_fresh || nobody {
            let mut step: Option<Option<Step>> = None;
            ctx.input_mut(|i| {
                i.events.retain(|e| {
                    if step.is_some() {
                        return true;
                    }
                    let Event::Key { key, pressed: true, modifiers, .. } = e else { return true };
                    if modifiers.alt || modifiers.ctrl || modifiers.command {
                        return true;
                    }
                    step = match key {
                        Key::Enter => Some(Some(if modifiers.shift { Step::Up } else { Step::Down })),
                        Key::Tab => Some(Some(if modifiers.shift { Step::Left } else { Step::Right })),
                        Key::Escape => Some(None),
                        _ => return true,
                    };
                    false
                })
            });
            match step {
                Some(Some(s)) => {
                    cx.commit(s);
                    (*p, *chars) = parsed(cx.doc);
                    resp.request_focus();
                }
                Some(None) => {
                    cx.st.editing = None;
                    resp.request_focus();
                }
                None => {}
            }
        } else {
            // Fokus woanders hin (Klick außerhalb): übernehmen wie in jeder Tabelle.
            cx.commit(Step::Stay);
            (*p, *chars) = parsed(cx.doc);
        }
    }
}

/// Eine Tastatureingabe außerhalb der Bearbeitung. `true` = verbraucht.
fn table_key(
    cx: &mut Ctx,
    p: &mut Arc<Parsed>,
    chars: &mut Arc<Vec<char>>,
    ev: &Event,
    page_rows: usize,
) -> bool {
    let reparse = |cx: &mut Ctx, p: &mut Arc<Parsed>, chars: &mut Arc<Vec<char>>| {
        (*p, *chars) = parsed(cx.doc);
    };
    match ev {
        Event::Text(t) if !t.is_empty() && t.chars().all(|c| !c.is_control()) => {
            cx.start_edit(p, chars, Some(t.clone()));
            true
        }
        Event::Copy => {
            cx.out.clipboard = Some(csv::to_tsv(&cx.selected_values(p, chars)));
            true
        }
        Event::Cut => {
            cx.out.clipboard = Some(csv::to_tsv(&cx.selected_values(p, chars)));
            cx.clear_selection(p, chars);
            reparse(cx, p, chars);
            true
        }
        Event::Paste(t) => {
            cx.paste(p, t);
            reparse(cx, p, chars);
            true
        }
        Event::Key { key, pressed: true, modifiers: mo, .. } => {
            let ctrl = mo.ctrl || mo.command;
            if mo.alt {
                return false;
            }
            let ext = mo.shift;
            let (rec, col) = cx.active(p);
            let vr = cx.view_row(rec).unwrap_or(0);
            let last_row = cx.st.rows.rows.len().saturating_sub(1);
            let last_col = cx.ncols - 1;
            match key {
                // Danach neu parsen: weitere Tasten desselben Bildes brauchen die neuen Feldgrenzen.
                Key::Z if ctrl => {
                    cx.undo(mo.shift);
                    reparse(cx, p, chars);
                }
                Key::Y if ctrl => {
                    cx.undo(true);
                    reparse(cx, p, chars);
                }
                Key::A if ctrl => {
                    if let (Some(&a), Some(&b)) = (cx.st.rows.rows.first(), cx.st.rows.rows.last()) {
                        cx.st.anchor = Some((a, 0));
                        let rev = cx.doc.rev;
                        set_active(cx.v, cx.st, p, rev, b, last_col);
                    }
                }
                _ if ctrl && !matches!(key, Key::Home | Key::End | Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight) => {
                    return false;
                }
                Key::ArrowUp if ctrl => cx.move_to(p, 0, col, ext),
                Key::ArrowDown if ctrl => cx.move_to(p, last_row, col, ext),
                Key::ArrowLeft if ctrl => cx.move_to(p, vr, 0, ext),
                Key::ArrowRight if ctrl => cx.move_to(p, vr, last_col, ext),
                Key::ArrowUp => cx.move_by(p, -1, 0, ext),
                Key::ArrowDown => cx.move_by(p, 1, 0, ext),
                Key::ArrowLeft => cx.move_by(p, 0, -1, ext),
                Key::ArrowRight => cx.move_by(p, 0, 1, ext),
                Key::Home if ctrl => cx.move_to(p, 0, 0, ext),
                Key::End if ctrl => cx.move_to(p, last_row, last_col, ext),
                Key::Home => cx.move_to(p, vr, 0, ext),
                Key::End => cx.move_to(p, vr, last_col, ext),
                Key::PageUp => cx.move_to(p, vr.saturating_sub(page_rows), col, ext),
                Key::PageDown => cx.move_to(p, vr + page_rows, col, ext),
                Key::Enter => cx.step(p, if mo.shift { Step::Up } else { Step::Down }, false),
                Key::Tab => cx.step(p, if mo.shift { Step::Left } else { Step::Right }, false),
                Key::F2 if !mo.shift => cx.start_edit(p, chars, None),
                Key::Backspace => cx.start_edit(p, chars, Some(String::new())),
                Key::Delete => {
                    cx.clear_selection(p, chars);
                    reparse(cx, p, chars);
                }
                Key::Escape => cx.st.anchor = None,
                _ => return false,
            }
            true
        }
        _ => false,
    }
}
