//! Farbdesigns: Oberfläche, Schreibfeld und Syntaxfarben je Design an einer Stelle.
use crate::editor::Palette;
use crate::ui_chrome::Chrome;
use egui::{pos2, Color32, Painter, Rect, Stroke};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ThemeId {
    #[default]
    Light,
    Dark,
    SolarizedLight,
    SolarizedDark,
    Nord,
    Monokai,
    Sepia,
    HighContrast,
    Win98,
}

impl ThemeId {
    pub const ALL: [ThemeId; 9] = [
        ThemeId::Light,
        ThemeId::Dark,
        ThemeId::SolarizedLight,
        ThemeId::SolarizedDark,
        ThemeId::Nord,
        ThemeId::Monokai,
        ThemeId::Sepia,
        ThemeId::HighContrast,
        ThemeId::Win98,
    ];

    pub fn label(self) -> String {
        match self {
            ThemeId::Light => crate::t!("theme.light"),
            ThemeId::Dark => crate::t!("theme.dark"),
            ThemeId::SolarizedLight => crate::t!("theme.solarized_light"),
            ThemeId::SolarizedDark => crate::t!("theme.solarized_dark"),
            ThemeId::Nord => "Nord".into(),
            ThemeId::Monokai => "Monokai".into(),
            ThemeId::Sepia => crate::t!("theme.sepia"),
            ThemeId::HighContrast => crate::t!("theme.high_contrast"),
            ThemeId::Win98 => "Windows 98".into(),
        }
    }

    /// Kurzname für --state theme=…
    pub fn key(self) -> &'static str {
        match self {
            ThemeId::Light => "light",
            ThemeId::Dark => "dark",
            ThemeId::SolarizedLight => "solarized-light",
            ThemeId::SolarizedDark => "solarized-dark",
            ThemeId::Nord => "nord",
            ThemeId::Monokai => "monokai",
            ThemeId::Sepia => "sepia",
            ThemeId::HighContrast => "contrast",
            ThemeId::Win98 => "win98",
        }
    }

    pub fn from_key(k: &str) -> Option<ThemeId> {
        ThemeId::ALL.into_iter().find(|t| t.key() == k)
    }

    pub fn is_dark(self) -> bool {
        matches!(self, ThemeId::Dark | ThemeId::SolarizedDark | ThemeId::Nord | ThemeId::Monokai | ThemeId::HighContrast)
    }

    /// Klassischer Look mit 3D-Kanten, eigener Schrift und bunten Symbolen.
    pub fn retro(self) -> bool {
        self == ThemeId::Win98
    }
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Syntaxfarben eines Designs.
pub struct Syntax {
    pub fg: u32,
    pub bg: u32,
    pub comment: u32,
    pub string: u32,
    pub number: u32,
    pub keyword: u32,
    pub typ: u32,
    pub pre: u32,
    pub op: u32,
    pub func: u32,
    pub bad: u32,
    pub attr: u32,
    pub inserted: u32,
}

pub fn syntax(t: ThemeId) -> Syntax {
    match t {
        ThemeId::Light => Syntax {
            fg: 0x1E1E1E, bg: 0xFFFFFF, comment: 0x6A7B7D, string: 0x1C7C54, number: 0xB35C00, keyword: 0x7A3FBF,
            typ: 0x1F6FB2, pre: 0x9C5D27, op: 0x4A4A4A, func: 0x2457A6, bad: 0xD12F2F, attr: 0xAD6A00, inserted: 0x2E7D32,
        },
        // Nur Farben der klassischen 16-Farben-Palette: Navy, Teal, Maroon, Purple, Olive, Grün, Blau.
        ThemeId::Win98 => Syntax {
            fg: 0x000000, bg: 0xFFFFFF, comment: 0x008080, string: 0x800000, number: 0x800080, keyword: 0x000080,
            typ: 0x008000, pre: 0x808000, op: 0x000000, func: 0x0000FF, bad: 0xFF0000, attr: 0x800080, inserted: 0x008000,
        },
        ThemeId::Dark => Syntax {
            fg: 0xDCDCDC, bg: 0x1F1F1F, comment: 0x8FBC6F, string: 0xD0A070, number: 0xE0A050, keyword: 0x7FB4F0,
            typ: 0xC8A0F0, pre: 0xC09060, op: 0xC0C8F0, func: 0xE0D080, bad: 0xF08070, attr: 0xF0A0A0, inserted: 0x8FBC6F,
        },
        ThemeId::SolarizedLight => Syntax {
            fg: 0x586E75, bg: 0xFDF6E3, comment: 0x93A1A1, string: 0x2AA198, number: 0xD33682, keyword: 0x859900,
            typ: 0xB58900, pre: 0xCB4B16, op: 0x657B83, func: 0x268BD2, bad: 0xDC322F, attr: 0x6C71C4, inserted: 0x859900,
        },
        ThemeId::SolarizedDark => Syntax {
            fg: 0x93A1A1, bg: 0x002B36, comment: 0x586E75, string: 0x2AA198, number: 0xD33682, keyword: 0x859900,
            typ: 0xB58900, pre: 0xCB4B16, op: 0x839496, func: 0x268BD2, bad: 0xDC322F, attr: 0x6C71C4, inserted: 0x859900,
        },
        ThemeId::Nord => Syntax {
            fg: 0xD8DEE9, bg: 0x2E3440, comment: 0x7B88A1, string: 0xA3BE8C, number: 0xB48EAD, keyword: 0x81A1C1,
            typ: 0x8FBCBB, pre: 0x5E81AC, op: 0x81A1C1, func: 0x88C0D0, bad: 0xBF616A, attr: 0xD08770, inserted: 0xA3BE8C,
        },
        ThemeId::Monokai => Syntax {
            fg: 0xF8F8F2, bg: 0x272822, comment: 0x75715E, string: 0xE6DB74, number: 0xAE81FF, keyword: 0xF92672,
            typ: 0x66D9EF, pre: 0xF92672, op: 0xF92672, func: 0xA6E22E, bad: 0xF44747, attr: 0xA6E22E, inserted: 0xA6E22E,
        },
        ThemeId::Sepia => Syntax {
            fg: 0x433422, bg: 0xF4ECD8, comment: 0x8A7A5C, string: 0x8C4A2F, number: 0xA0522D, keyword: 0x2F5D7C,
            typ: 0x6B4C9A, pre: 0x7A5C1E, op: 0x5A4632, func: 0x1F6B5A, bad: 0xB22222, attr: 0x9C5B1F, inserted: 0x4F7A28,
        },
        ThemeId::HighContrast => Syntax {
            fg: 0xFFFFFF, bg: 0x000000, comment: 0x00FF00, string: 0xFFFF00, number: 0xFF80FF, keyword: 0x00FFFF,
            typ: 0xFFB000, pre: 0xFF8040, op: 0xFFFFFF, func: 0x80C0FF, bad: 0xFF4040, attr: 0xFFB000, inserted: 0x00FF00,
        },
    }
}

/// Farben des Schreibfelds.
pub fn palette(t: ThemeId) -> Palette {
    let s = syntax(t);
    let base = Palette {
        bg: rgb(s.bg),
        fg: rgb(s.fg),
        caret: rgb(s.fg),
        ..if t.is_dark() { Palette::dark() } else { Palette::light() }
    };
    match t {
        ThemeId::Light => Palette::light(),
        ThemeId::Dark => Palette::dark(),
        ThemeId::SolarizedLight => Palette {
            line_bg: rgb(0xEEE8D5), sel_bg: rgb(0xD9D2BC), gutter_bg: rgb(0xEEE8D5), gutter_fg: rgb(0x93A1A1),
            margin_bg: rgb(0xF5EFDC), symbol: rgb(0xC5BEA8), guide: rgb(0xE0DAC6), brace_bg: rgb(0xF2C9B8),
            smart_bg: rgb(0xD5E3A6), mark_bg: rgb(0xB8E0DC), bookmark: rgb(0x268BD2), fold: rgb(0x93A1A1),
            scroll_bg: rgb(0xF5EFDC), scroll_fg: rgb(0xD3CBB4), ..base
        },
        ThemeId::SolarizedDark => Palette {
            line_bg: rgb(0x073642), sel_bg: rgb(0x0E4B5A), gutter_bg: rgb(0x073642), gutter_fg: rgb(0x586E75),
            margin_bg: rgb(0x03313C), symbol: rgb(0x2C5560), guide: rgb(0x154452), brace_bg: rgb(0x5A3A2A),
            smart_bg: rgb(0x2D4A1A), mark_bg: rgb(0x0F5552), bookmark: rgb(0x268BD2), fold: rgb(0x839496),
            scroll_bg: rgb(0x03313C), scroll_fg: rgb(0x1B4D5A), ..base
        },
        ThemeId::Nord => Palette {
            line_bg: rgb(0x3B4252), sel_bg: rgb(0x434C5E), gutter_bg: rgb(0x2E3440), gutter_fg: rgb(0x4C566A),
            margin_bg: rgb(0x2B303B), symbol: rgb(0x4C566A), guide: rgb(0x3B4252), brace_bg: rgb(0x5E4A57),
            smart_bg: rgb(0x445A4A), mark_bg: rgb(0x3A5A66), bookmark: rgb(0x88C0D0), fold: rgb(0x81A1C1),
            caret: rgb(0xECEFF4), scroll_bg: rgb(0x2B303B), scroll_fg: rgb(0x4C566A), ..base
        },
        ThemeId::Monokai => Palette {
            line_bg: rgb(0x3E3D32), sel_bg: rgb(0x49483E), gutter_bg: rgb(0x272822), gutter_fg: rgb(0x90908A),
            margin_bg: rgb(0x23241E), symbol: rgb(0x5A5A50), guide: rgb(0x3B3A32), brace_bg: rgb(0x6A4A30),
            smart_bg: rgb(0x4A5A28), mark_bg: rgb(0x2C5A66), bookmark: rgb(0x66D9EF), fold: rgb(0xA0A090),
            caret: rgb(0xF8F8F0), scroll_bg: rgb(0x23241E), scroll_fg: rgb(0x55554B), ..base
        },
        ThemeId::Sepia => Palette {
            line_bg: rgb(0xEADFC4), sel_bg: rgb(0xD8C9A3), gutter_bg: rgb(0xE8DDC2), gutter_fg: rgb(0x9C8B6C),
            margin_bg: rgb(0xEFE5CD), symbol: rgb(0xC2B392), guide: rgb(0xDDD0B2), brace_bg: rgb(0xEBC0A8),
            smart_bg: rgb(0xCFDDA8), mark_bg: rgb(0xB9D8CF), bookmark: rgb(0x2F5D7C), fold: rgb(0x8A7A5C),
            scroll_bg: rgb(0xEFE5CD), scroll_fg: rgb(0xCDBE9C), ..base
        },
        ThemeId::HighContrast => Palette {
            line_bg: rgb(0x1A1A40), sel_bg: rgb(0x0000C0), gutter_bg: rgb(0x000000), gutter_fg: rgb(0xFFFF00),
            margin_bg: rgb(0x101010), symbol: rgb(0x808080), guide: rgb(0x606060), brace_bg: rgb(0x800000),
            smart_bg: rgb(0x006000), mark_bg: rgb(0x006060), bookmark: rgb(0x00FFFF), fold: rgb(0xFFFFFF),
            scroll_bg: rgb(0x000000), scroll_fg: rgb(0xC0C0C0), ..base
        },
        ThemeId::Win98 => Palette {
            line_bg: rgb(0xFFFFE8), sel_bg: rgb(0xA6CAF0), gutter_bg: rgb(0xC0C0C0), gutter_fg: rgb(0x000000),
            margin_bg: rgb(0xDFDFDF), symbol: rgb(0x808080), guide: rgb(0xC0C0C0), brace_bg: rgb(0xFFFF99),
            smart_bg: rgb(0xCCFFCC), mark_bg: rgb(0xC0FFFF), bookmark: rgb(0x000080), fold: rgb(0x000000),
            scroll_bg: rgb(0xDFDFDF), scroll_fg: rgb(0xC0C0C0), scroll_bevel: true, ..base
        },
    }
}

/// Farben der Oberfläche um das Schreibfeld.
pub fn chrome(t: ThemeId) -> Chrome {
    let c = |window, menu, text, text_dim, tab_inactive, tab_active, tab_text, accent, line, hover, hover_line| Chrome {
        window: rgb(window),
        menu: rgb(menu),
        text: rgb(text),
        text_dim: rgb(text_dim),
        tab_inactive: rgb(tab_inactive),
        tab_active: rgb(tab_active),
        tab_text: rgb(tab_text),
        accent: rgb(accent),
        line: rgb(line),
        hover: rgb(hover),
        hover_line: rgb(hover_line),
        hover_text: rgb(text),
    };
    match t {
        ThemeId::Light => c(0xF3F3F3, 0xF7F7F7, 0x202020, 0x505050, 0xE2E2E2, 0xFFFFFF, 0x505050, 0x2F6FDB, 0xC4C4C4, 0xDCE8F8, 0x8FB4EA),
        ThemeId::Dark => c(0x2D2D30, 0x2D2D30, 0xE0E0E0, 0xA0A0A0, 0x3C3C40, 0x1F1F1F, 0xB0B0B0, 0x4C9AFF, 0x555558, 0x3E4A58, 0x5080B0),
        ThemeId::SolarizedLight => c(0xEEE8D5, 0xEEE8D5, 0x586E75, 0x839496, 0xDDD6C1, 0xFDF6E3, 0x657B83, 0xB58900, 0xC9C2AD, 0xE0DCC8, 0x93A1A1),
        ThemeId::SolarizedDark => c(0x073642, 0x073642, 0x93A1A1, 0x839496, 0x0A3F4C, 0x002B36, 0x839496, 0xB58900, 0x1D4F5C, 0x0E4B5A, 0x268BD2),
        ThemeId::Nord => c(0x3B4252, 0x3B4252, 0xECEFF4, 0xD8DEE9, 0x434C5E, 0x2E3440, 0xD8DEE9, 0x88C0D0, 0x4C566A, 0x4C566A, 0x88C0D0),
        ThemeId::Monokai => c(0x1E1F1C, 0x1E1F1C, 0xF8F8F2, 0xA0A090, 0x34352F, 0x272822, 0xC0C0B0, 0xF92672, 0x45463F, 0x3E3D32, 0xA6E22E),
        ThemeId::Sepia => c(0xE8DDC2, 0xEBE1C8, 0x433422, 0x6B5A40, 0xD6C8A6, 0xF4ECD8, 0x5A4A32, 0xB5762B, 0xBFAF8C, 0xDCCDA8, 0xB5762B),
        ThemeId::HighContrast => c(0x000000, 0x000000, 0xFFFFFF, 0xFFFF00, 0x202020, 0x000000, 0xFFFFFF, 0x00FFFF, 0xFFFFFF, 0x0000C0, 0x00FFFF),
        ThemeId::Win98 => Chrome {
            hover_text: Color32::WHITE,
            ..c(0xC0C0C0, 0xC0C0C0, 0x000000, 0x404040, 0xC0C0C0, 0xC0C0C0, 0x000000, 0x000080, 0x808080, 0x000080, 0x000080)
        },
    }
}

/// Titel eines Dialogfensters; im klassischen Look weiß und fett auf der blauen Leiste.
pub fn window_title(t: ThemeId, text: &str) -> egui::RichText {
    let r = egui::RichText::new(text);
    if t.retro() { r.color(Color32::WHITE).strong() } else { r }
}

// Die Grautöne der klassischen 3D-Kanten.
pub const W98_WHITE: Color32 = rgb(0xFFFFFF);
pub const W98_LIGHT: Color32 = rgb(0xDFDFDF);
pub const W98_SHADOW: Color32 = rgb(0x808080);
pub const W98_DARK: Color32 = rgb(0x000000);

fn edges(p: &Painter, r: Rect, tl: Color32, br: Color32) {
    let s = |c| Stroke::new(1.0, c);
    // Halbe Pixel, damit die Linien scharf auf dem Raster liegen.
    let (l, t, rr, bb) = (r.min.x + 0.5, r.min.y + 0.5, r.max.x - 0.5, r.max.y - 0.5);
    p.line_segment([pos2(l, bb), pos2(l, t)], s(tl));
    p.line_segment([pos2(l, t), pos2(rr, t)], s(tl));
    p.line_segment([pos2(rr, t), pos2(rr, bb)], s(br));
    p.line_segment([pos2(l, bb), pos2(rr + 0.5, bb)], s(br));
}

/// Zweifache Kante wie bei Knöpfen (erhaben) oder Eingabefeldern (vertieft).
pub fn bevel(p: &Painter, r: Rect, raised: bool) {
    if raised {
        edges(p, r, W98_WHITE, W98_DARK);
        edges(p, r.shrink(1.0), W98_LIGHT, W98_SHADOW);
    } else {
        edges(p, r, W98_SHADOW, W98_WHITE);
        edges(p, r.shrink(1.0), W98_DARK, W98_LIGHT);
    }
}

/// Einfache Kante wie bei Werkzeugknöpfen und Feldern der Statuszeile.
pub fn bevel_thin(p: &Painter, r: Rect, raised: bool) {
    if raised {
        edges(p, r, W98_WHITE, W98_SHADOW);
    } else {
        edges(p, r, W98_SHADOW, W98_WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_has_a_key_and_readable_text() {
        for t in ThemeId::ALL {
            assert_eq!(ThemeId::from_key(t.key()), Some(t));
            let s = syntax(t);
            let lum = |c: u32| ((c >> 16) & 255) * 3 + ((c >> 8) & 255) * 6 + (c & 255);
            let diff = lum(s.fg).abs_diff(lum(s.bg));
            assert!(diff > 1000, "{t:?}: Text hebt sich nicht vom Hintergrund ab");
            assert_eq!(t.is_dark(), lum(s.bg) < lum(s.fg), "{t:?}: hell/dunkel passt nicht");
        }
    }
}
