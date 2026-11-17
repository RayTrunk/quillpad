//! Leiste über der Tabelle: Filterfeld, Spaltenauswahl, „n von m Zeilen“ und bei
//! Sortierung der Hinweis „nur Ansicht“ samt „Reihenfolge übernehmen“.

use super::{col_label, ncols_of, parsed, TableCmd, TableOut, TableState};
use crate::csv::{Dialect, SortDir};
use crate::doc::Document;
use crate::editor::View;
use egui::{Align, Button, ComboBox, Key, Layout, Rect, Stroke, TextEdit, Ui, UiBuilder};

pub(super) const BAR_H: f32 = 30.0;

pub(super) fn show(ui: &mut Ui, rect: Rect, doc: &mut Document, v: &mut View, st: &mut TableState, d: Dialect, out: &mut TableOut) {
    let (p, chars) = parsed(doc);
    let vis = ui.visuals().clone();
    ui.painter().rect_filled(rect, 0.0, vis.panel_fill);
    ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0_f32, vis.widgets.noninteractive.bg_stroke.color));
    let inner = rect.shrink2(egui::vec2(6.0, 3.0));
    let mut bu = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));
    bu.spacing_mut().item_spacing.x = 6.0;
    let id = egui::Id::new(("table-filter", v.uid));
    let mut changed = false;
    if st.filter_open {
        bu.label(crate::t!("table.filter_label"));
        let r = bu.add(
            TextEdit::singleline(&mut st.rows.filter)
                .id(id)
                .desired_width(170.0)
                .hint_text(crate::t!("table.filter_hint")),
        );
        if st.filter_focus {
            st.filter_focus = false;
            r.request_focus();
        }
        changed |= r.changed();
        let close = r.lost_focus() && bu.input(|i| i.key_pressed(Key::Escape));
        let ncols = ncols_of(&p);
        let all = crate::t!("table.filter_all");
        let shown = st.rows.filter_col.map_or(all.clone(), |c| col_label(&p, &chars, d, c));
        ComboBox::from_id_salt(id.with("col")).selected_text(shown).width(140.0).show_ui(&mut bu, |ui| {
            if ui.selectable_label(st.rows.filter_col.is_none(), &all).clicked() {
                st.rows.filter_col = None;
                changed = true;
            }
            for c in 0..ncols {
                if ui.selectable_label(st.rows.filter_col == Some(c), col_label(&p, &chars, d, c)).clicked() {
                    st.rows.filter_col = Some(c);
                    changed = true;
                }
            }
        });
        if changed {
            st.rows.rebuild(&chars, &p, d, doc.rev);
        }
        let total = p.records.len().saturating_sub(usize::from(d.header));
        bu.label(crate::t!("table.filter_count", n = st.rows.rows.len(), m = total));
        let x = bu.add(Button::new("×").frame(false)).on_hover_text(crate::t!("table.filter_close"));
        if close || x.clicked() {
            st.filter_open = false;
            st.rows.filter.clear();
            st.rows.filter_col = None;
            st.rows.rebuild(&chars, &p, d, doc.rev);
            v.want_focus = true;
        }
    }
    if let Some((c, dir)) = st.rows.sort {
        if st.filter_open {
            bu.separator();
        }
        let arrow = if dir == SortDir::Asc { "▲" } else { "▼" };
        bu.label(format!("{arrow} {}", col_label(&p, &chars, d, c)));
        bu.label(egui::RichText::new(crate::t!("table.view_only")).weak());
        let ok = st.rows.filter.is_empty() && !doc.read_only;
        let b = bu
            .add_enabled(ok, Button::new(crate::t!("table.apply_order")))
            .on_hover_text(crate::t!("table.apply_order_tip"))
            .on_disabled_hover_text(crate::t!("table.apply_order_blocked"));
        if b.clicked() {
            out.cmds.push(TableCmd::ApplyOrder);
        }
        if bu.button(crate::t!("table.clear_sort")).clicked() {
            out.cmds.push(TableCmd::ClearSort);
        }
    }
}
