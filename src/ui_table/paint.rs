//! Zeichnen der Tabelle: Zellen, feste Kopfzeile und Nummernspalte, Eingabefeld, Bildlaufleisten.

use super::{col_name, first_line, fit, text_w, Ctx, Layout, PAD};
use crate::csv::{self, field_value, Parsed};
use crate::editor::{Metrics, Palette};
use egui::{pos2, vec2, Align2, Key, KeyboardShortcut, Modifiers, Rect, Sense, Stroke, StrokeKind, Ui};

fn line(w: f32, c: egui::Color32) -> Stroke {
    Stroke::new(w, c)
}

/// Zeichnet nur die sichtbaren Zeilen und Spalten; Kopfzeile und Nummernspalte bleiben stehen.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint(
    ui: &mut Ui,
    cx: &mut Ctx,
    lay: &Layout,
    p: &Parsed,
    chars: &[char],
    has_focus: bool,
    pal: &Palette,
    m: &mut Metrics,
    retro: bool,
    edit_id: egui::Id,
) {
    let Layout { rect, body, row_h, head_h, num_w, ncols, nrows, need_v, need_h, vbar, hbar } = *lay;
    let ctx = ui.ctx().clone();
    let d = cx.d;
    let active = cx.active(p);
    let active_vr = cx.view_row(active.0);
    // Zeichnen
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, pal.bg);
    let accent = pal.bookmark;
    // Kopf und Nummern heben sich ab, auch wo der Rand die Farbe des Hintergrunds hat.
    let head_bg = if pal.gutter_bg == pal.bg { pal.bg.lerp_to_gamma(pal.fg, 0.07) } else { pal.gutter_bg };
    let head_sel = pal.sel_bg.gamma_multiply(0.55);
    let grid = line(1.0, pal.guide);
    let (sr0, sr1, sc0, sc1) = cx.selection(p);
    let multi = sr0 != sr1 || sc0 != sc1;
    let hit = !cx.v.sels.main().is_empty();
    let scroll = cx.st.scroll;
    let widths = cx.st.widths.clone();
    let first_vr = (scroll.y / row_h).floor() as usize;
    let last_vr = (((scroll.y + body.height()) / row_h).ceil() as usize).min(nrows);
    let first_col = lay.hit_col(&widths, scroll.x, body.left()).unwrap_or(0);
    let row_y = |vr: usize| body.top() + vr as f32 * row_h - scroll.y;

    // Zellen
    let cells = painter.with_clip_rect(body);
    for (vr, &rec) in cx.st.rows.rows.iter().enumerate().take(last_vr).skip(first_vr) {
        let y = row_y(vr);
        let r = &p.records[rec];
        let mut x = lay.col_x(&widths, scroll.x, first_col);
        for (c, &w) in widths.iter().enumerate().take(ncols).skip(first_col) {
            if x > body.right() {
                break;
            }
            let cell = Rect::from_min_size(pos2(x, y), vec2(w, row_h));
            let selected = vr >= sr0 && vr <= sr1 && c >= sc0 && c <= sc1;
            let is_active = active == (rec, c);
            match r.fields.get(c) {
                Some(f) => {
                    if is_active && hit {
                        cells.rect_filled(cell, 0.0, pal.mark_bg);
                    } else if selected && (multi || !has_focus) {
                        cells.rect_filled(cell, 0.0, pal.sel_bg);
                    }
                    let value = field_value(chars, f);
                    let (line, more) = first_line(&value);
                    let mark_w = if more { m.w('⏎') + 4.0 } else { 0.0 };
                    let shown = fit(line, w - 2.0 * PAD - mark_w, m);
                    let tr = cells.text(pos2(x + PAD, y + row_h * 0.5), Align2::LEFT_CENTER, shown, m.font.clone(), pal.fg);
                    if more {
                        cells.text(pos2(tr.right() + 4.0, y + row_h * 0.5), Align2::LEFT_CENTER, "⏎", m.font.clone(), pal.symbol);
                    }
                }
                None => {
                    // Fehlende Zelle einer kurzen Zeile: schraffiert.
                    let fill = if selected { pal.sel_bg } else { pal.margin_bg };
                    cells.rect_filled(cell, 0.0, fill);
                    let hatch = painter.with_clip_rect(cell.intersect(body));
                    let mut k = -row_h;
                    while k < w {
                        hatch.line_segment([pos2(x + k, y + row_h), pos2(x + k + row_h, y)], line(1.0, pal.guide));
                        k += 7.0;
                    }
                }
            }
            cells.line_segment([pos2(x + w - 0.5, y), pos2(x + w - 0.5, y + row_h)], grid);
            x += w;
        }
        cells.line_segment([pos2(body.left(), y + row_h - 0.5), pos2(x.min(body.right()), y + row_h - 0.5)], grid);
    }
    // Rahmen der aktiven Zelle
    if let Some(vr) = active_vr {
        let c = active.1.min(ncols - 1);
        let cell = Rect::from_min_size(pos2(lay.col_x(&widths, scroll.x, c), row_y(vr)), vec2(widths[c], row_h));
        let col = if has_focus { accent } else { pal.gutter_fg };
        cells.rect_stroke(cell.shrink(0.5), 0.0, line(2.0, col), StrokeKind::Inside);
    }

    // Kopfzeile
    let head = Rect::from_min_max(pos2(body.left(), rect.top()), pos2(body.right(), body.top()));
    let hp = painter.with_clip_rect(head);
    hp.rect_filled(head, 0.0, head_bg);
    let mut x = lay.col_x(&widths, scroll.x, first_col);
    for (c, &w) in widths.iter().enumerate().take(ncols).skip(first_col) {
        if x > head.right() {
            break;
        }
        let cell = Rect::from_min_size(pos2(x, head.top()), vec2(w, head_h));
        let label = if d.header {
            p.records.first().and_then(|r| r.fields.get(c)).map(|f| field_value(chars, f)).unwrap_or_default()
        } else {
            col_name(c)
        };
        let label = if label.is_empty() { col_name(c) } else { first_line(&label).0.to_string() };
        let arrow = match cx.st.rows.sort {
            Some((sc, csv::SortDir::Asc)) if sc == c => Some("▲"),
            Some((sc, csv::SortDir::Desc)) if sc == c => Some("▼"),
            _ => None,
        };
        let arrow_w = if arrow.is_some() { m.ch * 2.0 } else { 0.0 };
        let in_sel = c >= sc0 && c <= sc1;
        if retro {
            if in_sel {
                hp.rect_filled(cell, 0.0, crate::theme::W98_LIGHT);
            }
            crate::theme::bevel_thin(&hp, cell, true);
        } else {
            if in_sel {
                hp.rect_filled(cell, 0.0, head_sel);
            }
            hp.line_segment([pos2(x + w - 0.5, cell.top() + 4.0), pos2(x + w - 0.5, cell.bottom() - 4.0)], line(1.0, pal.gutter_fg.gamma_multiply(0.5)));
        }
        if c == active.1 && !retro {
            hp.rect_filled(Rect::from_min_max(pos2(x, cell.bottom() - 2.0), pos2(x + w, cell.bottom())), 0.0, accent);
        }
        let shown = fit(&label, w - 2.0 * PAD - arrow_w, m);
        let font = if d.header { m.bold.clone() } else { m.font.clone() };
        let tc = if d.header { pal.fg } else { pal.gutter_fg };
        hp.text(pos2(x + PAD, cell.center().y), Align2::LEFT_CENTER, shown, font, tc);
        if let Some(a) = arrow {
            hp.text(pos2(x + w - PAD, cell.center().y), Align2::RIGHT_CENTER, a, m.font.clone(), pal.gutter_fg);
        }
        x += w;
    }
    if !retro {
        hp.line_segment([pos2(head.left(), head.bottom() - 0.5), pos2(head.right(), head.bottom() - 0.5)], line(1.0, pal.gutter_fg.gamma_multiply(0.6)));
    }

    // Nummernspalte: Datensatznummer in der Datei, damit sie beim Sortieren stabil bleibt.
    let nums = Rect::from_min_max(pos2(rect.left(), body.top()), pos2(body.left(), body.bottom()));
    let np = painter.with_clip_rect(nums);
    np.rect_filled(nums, 0.0, head_bg);
    for (vr, &rec) in cx.st.rows.rows.iter().enumerate().take(last_vr).skip(first_vr) {
        let y = row_y(vr);
        let cell = Rect::from_min_size(pos2(nums.left(), y), vec2(num_w, row_h));
        let in_sel = vr >= sr0 && vr <= sr1;
        let n = if d.header { rec } else { rec + 1 };
        if retro {
            if in_sel {
                np.rect_filled(cell, 0.0, crate::theme::W98_LIGHT);
            }
            crate::theme::bevel_thin(&np, cell, true);
        } else if in_sel {
            np.rect_filled(cell, 0.0, head_sel);
        }
        let (font, tc) = if Some(vr) == active_vr { (m.bold.clone(), pal.fg) } else { (m.font.clone(), pal.gutter_fg) };
        np.text(pos2(cell.right() - PAD, cell.center().y), Align2::RIGHT_CENTER, n.to_string(), font, tc);
    }
    if !retro {
        np.line_segment([pos2(nums.right() - 0.5, nums.top()), pos2(nums.right() - 0.5, nums.bottom())], line(1.0, pal.gutter_fg.gamma_multiply(0.6)));
    }

    // Ecke
    let corner = Rect::from_min_max(rect.min, pos2(body.left(), body.top()));
    painter.rect_filled(corner, 0.0, head_bg);
    if retro {
        crate::theme::bevel_thin(&painter, corner, true);
    } else {
        painter.line_segment([pos2(corner.right() - 0.5, corner.top()), pos2(corner.right() - 0.5, corner.bottom())], line(1.0, pal.gutter_fg.gamma_multiply(0.6)));
        painter.line_segment([pos2(corner.left(), corner.bottom() - 0.5), pos2(corner.right(), corner.bottom() - 0.5)], line(1.0, pal.gutter_fg.gamma_multiply(0.6)));
    }
    painter.text(corner.center(), Align2::CENTER_CENTER, "#", m.font.clone(), pal.gutter_fg);
    if need_v && need_h {
        painter.rect_filled(Rect::from_min_max(pos2(vbar.left(), hbar.top()), rect.max), 0.0, pal.scroll_bg);
    }

    // Eingabefeld über der Zelle
    if let (Some(vr), Some(_)) = (active_vr, cx.st.editing.as_ref()) {
        let c = active.1.min(ncols - 1);
        let x = lay.col_x(&widths, scroll.x, c);
        let y = row_y(vr);
        let buf = cx.st.editing.as_mut().expect("oben geprüft");
        let lines = buf.split('\n').count().clamp(1, 8) as f32;
        let longest = buf.split('\n').map(|l| text_w(l, m)).fold(0.0, f32::max);
        let w = (longest + 2.0 * PAD + m.ch * 2.0).max(widths[c]).min(body.right() - x).max(m.ch * 4.0);
        let h = (lines * m.line_h + 6.0).max(row_h).min(body.bottom() - y).max(row_h);
        let er = Rect::from_min_size(pos2(x, y), vec2(w, h)).intersect(body);
        painter.rect_filled(er, 0.0, pal.bg);
        painter.rect_stroke(er.shrink(0.5), 0.0, line(2.0, accent), StrokeKind::Inside);
        if cx.st.edit_fresh {
            let mut state = egui::text_edit::TextEditState::default();
            let n = buf.chars().count();
            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(n))));
            state.store(&ctx, edit_id);
            ctx.memory_mut(|mem| mem.request_focus(edit_id));
        }
        let te = egui::TextEdit::multiline(buf)
            .id(edit_id)
            .font(m.font.clone())
            .text_color(pal.fg)
            .frame(false)
            .lock_focus(true)
            .return_key(KeyboardShortcut::new(Modifiers::ALT, Key::Enter))
            .desired_width(er.width() - 2.0 * PAD)
            .desired_rows(1)
            .margin(vec2(PAD - 2.0, 3.0));
        ui.put(er, te);
        cx.st.edit_fresh = false;
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn scrollbar(ui: &mut Ui, id: egui::Id, r: Rect, vertical: bool, content: f32, view: f32, pos: &mut f32, drag: &mut bool, pal: &Palette) {
    let len = if vertical { r.height() } else { r.width() };
    let max = (content - view).max(1.0);
    let thumb = (len * view / content.max(1.0)).clamp(20.0_f32.min(len), len);
    let room = (len - thumb).max(1.0);
    let resp = ui.interact(r, id, Sense::click_and_drag());
    let along = |p: egui::Pos2| if vertical { p.y - r.top() } else { p.x - r.left() };
    let start = room * (*pos / max).clamp(0.0, 1.0);
    if resp.drag_started() {
        *drag = resp.interact_pointer_pos().is_some_and(|p| {
            let a = along(p);
            a >= start && a <= start + thumb
        });
    }
    if resp.dragged() && *drag {
        let dlt = if vertical { resp.drag_delta().y } else { resp.drag_delta().x };
        *pos += dlt * max / room;
    } else if resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            *pos += if along(p) < start { -view } else { view };
        }
    }
    if !resp.dragged() {
        *drag = false;
    }
    let start = room * (pos.clamp(0.0, max) / max);
    let painter = ui.painter_at(r);
    painter.rect_filled(r, 0.0, pal.scroll_bg);
    let t = if vertical {
        Rect::from_min_size(pos2(r.left(), r.top() + start), vec2(r.width(), thumb))
    } else {
        Rect::from_min_size(pos2(r.left() + start, r.top()), vec2(thumb, r.height()))
    };
    if pal.scroll_bevel {
        painter.rect_filled(t, 0.0, pal.scroll_fg);
        crate::theme::bevel(&painter, t, true);
    } else {
        painter.rect_filled(t.shrink(3.0), 3.0, pal.scroll_fg);
    }
}
