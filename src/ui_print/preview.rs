//! Vorschau im Druckfenster: ein weißes Blatt mit den Runs der Seite, darunter das Blättern.
use super::{page_geom, PrintUi};
use crate::app::App;
use crate::print::{self, Measure};
use crate::t;
use egui::{pos2, vec2, Align2, Button, Color32, FontId, Key, Sense, Stroke, StrokeKind, Ui};

/// So viel Scrollweg blättert eine Seite.
const SCROLL_PAGE: f32 = 50.0;

impl App {
    /// Weißes Blatt im Seitenverhältnis des Papiers, darunter das Blättern.
    pub(super) fn print_preview(&self, ui: &mut Ui, p: &mut PrintUi, max_h: f32) {
        let g = p.geom.or_else(|| p.paper.as_ref().map(|x| page_geom(x, p.opts.margin_mm))).unwrap_or_else(|| page_geom(&print::default_paper(), 15.0));
        let scale = max_h / g.h;
        let (rect, resp) = ui.allocate_exact_size(vec2(g.w * scale, max_h), Sense::click());
        if p.focus {
            p.focus = false;
            resp.request_focus();
        }
        let n = p.shown().len();
        // Blättern: Mausrad über dem Blatt, Bild-auf/ab solange der Editor keinen Fokus hat.
        // Die Bewegung wird gesammelt, damit ein Touchpad nicht bei jedem Bild eine Seite weiterspringt;
        // eine Rastung des Mausrads (50 px) ist genau eine Seite.
        let mut step = 0i32;
        if resp.hovered() {
            p.scroll_acc += ui.input(|i| i.raw_scroll_delta.y);
            while p.scroll_acc >= SCROLL_PAGE - 0.5 {
                p.scroll_acc -= SCROLL_PAGE;
                step -= 1;
            }
            while p.scroll_acc <= -(SCROLL_PAGE - 0.5) {
                p.scroll_acc += SCROLL_PAGE;
                step += 1;
            }
        } else {
            p.scroll_acc = 0.0;
        }
        if !self.editor_focused {
            ui.input(|i| {
                step += i.key_pressed(Key::PageDown) as i32 - i.key_pressed(Key::PageUp) as i32;
            });
        }
        p.page_idx = (p.page_idx as i64 + step as i64).clamp(0, n.saturating_sub(1) as i64) as usize;

        // Das Blatt ist immer weiß, unabhängig vom Design.
        let painter = ui.painter();
        painter.rect_filled(rect.translate(vec2(3.0, 3.0)), 2.0, Color32::from_black_alpha(60));
        painter.rect_filled(rect, 0.0, Color32::WHITE);
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, Color32::from_gray(150)), StrokeKind::Outside);
        let clip = painter.with_clip_rect(rect);
        let busy = p.busy();
        if let (Some(page), Some(font)) = (p.shown().get(p.page_idx), &p.font) {
            let asc = font.ascent();
            // Die Bildschirmschrift so verkleinern, dass ihre Zeichen nicht breiter sind als die
            // der Druckschrift. Jedes Zeichen steht an seiner Papierposition: Bei so kleinen
            // Größen rundet egui die Breiten, ganze Runs würden sich sonst überlappen.
            let screen_w = ui.fonts(|f| f.glyph_width(&FontId::monospace(100.0), '0')) / 100.0;
            let fit = if screen_w > 0.0 { (font.advance('0', false) / screen_w).min(1.0) } else { 1.0 };
            for r in &page.runs {
                let color = Color32::from_rgb(r.color[0], r.color[1], r.color[2]);
                let id = FontId::monospace(r.size * scale * fit);
                let top = rect.top() + (r.y - asc * r.size) * scale;
                let mut x = r.x;
                let mut buf = [0u8; 4];
                for c in r.text.chars() {
                    if !c.is_whitespace() {
                        let at = pos2(rect.left() + x * scale, top);
                        let s: &str = c.encode_utf8(&mut buf);
                        clip.text(at, Align2::LEFT_TOP, s, id.clone(), color);
                        if r.bold {
                            // egui hat keine fette Festbreitschrift: leicht versetzt nachzeichnen.
                            clip.text(at + vec2(0.35, 0.0), Align2::LEFT_TOP, s, id.clone(), color);
                        }
                    }
                    x += font.advance(c, r.bold) * r.size;
                }
            }
            // Dieselbe Farbe wie auf dem Papier (`print::GRAY`).
            let gray = Color32::from_rgb(print::GRAY[0], print::GRAY[1], print::GRAY[2]);
            for &[x0, x1, y] in &page.rules {
                let y = rect.top() + y * scale;
                clip.line_segment([pos2(rect.left() + x0 * scale, y), pos2(rect.left() + x1 * scale, y)], Stroke::new(0.6_f32, gray));
            }
        }
        // Abgerissener Thread: dessen Meldung statt „wird berechnet“.
        let note = match p.error() {
            Some(e) => Some(e.to_string()),
            None => busy.then(|| t!("print.building")),
        };
        if let Some(note) = note {
            let at = rect.center_bottom() - vec2(0.0, 24.0);
            let galley = ui.painter().layout_no_wrap(note, FontId::proportional(13.0), Color32::from_gray(60));
            let bg = Align2::CENTER_CENTER.anchor_size(at, galley.size()).expand(6.0);
            clip.rect_filled(bg, 3.0, Color32::from_gray(235));
            clip.galley(bg.min + vec2(6.0, 6.0), galley, Color32::from_gray(60));
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(p.page_idx > 0, Button::new("◀")).clicked() {
                p.page_idx -= 1;
            }
            ui.label(t!("print.page_of", page = (p.page_idx + 1).min(n), total = n));
            if ui.add_enabled(p.page_idx + 1 < n, Button::new("▶")).clicked() {
                p.page_idx += 1;
            }
        });
    }
}
