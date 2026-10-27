//! Look and feel: palette, fonts, and a few painting helpers.

use eframe::egui::{
    self, epaint, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Painter, Pos2, Rect, Shadow, Stroke,
    Vec2,
};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(14, 16, 21);
pub const PANEL: Color32 = Color32::from_rgb(19, 22, 28);
pub const CARD: Color32 = Color32::from_rgb(28, 32, 40);
pub const CARD_HI: Color32 = Color32::from_rgb(38, 43, 54);
pub const LINE: Color32 = Color32::from_rgb(40, 45, 56);
pub const TEXT: Color32 = Color32::from_rgb(236, 238, 242);
pub const SUB: Color32 = Color32::from_rgb(160, 167, 180);
pub const DIM: Color32 = Color32::from_rgb(110, 117, 130);
pub const ACCENT: Color32 = Color32::from_rgb(255, 99, 82);
pub const ACCENT_HI: Color32 = Color32::from_rgb(255, 128, 110);
pub const GOLD: Color32 = Color32::from_rgb(245, 197, 66);

/// Soft tile colours for libraries / placeholders.
pub const TINTS: [Color32; 8] = [
    Color32::from_rgb(92, 72, 180),
    Color32::from_rgb(201, 76, 92),
    Color32::from_rgb(35, 132, 150),
    Color32::from_rgb(214, 128, 52),
    Color32::from_rgb(52, 140, 96),
    Color32::from_rgb(160, 70, 160),
    Color32::from_rgb(60, 100, 190),
    Color32::from_rgb(170, 110, 60),
];

pub fn tint_for(s: &str) -> Color32 {
    let h = s.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    TINTS[(h as usize) % TINTS.len()]
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}
pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let read = |f: &str| std::fs::read(format!(r"C:\Windows\Fonts\{f}")).ok();
    let add = |fonts: &mut FontDefinitions, name: &str, file: &str| -> bool {
        if let Some(b) = read(file) {
            fonts.font_data.insert(name.into(), Arc::new(FontData::from_owned(b)));
            true
        } else {
            false
        }
    };
    // Script fallbacks so Korean / Hindi / Bengali / CJK titles don't render as boxes.
    let mut fallbacks = vec![];
    for (n, f) in [("nirmala", "Nirmala.ttc"), ("malgun", "malgun.ttf"), ("msyh", "msyh.ttc"), ("segsym", "seguisym.ttf")] {
        if add(&mut fonts, n, f) {
            fallbacks.push(n.to_string());
        }
    }
    let defaults = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let fam = |primary: Option<&str>| {
        let mut v: Vec<String> = primary.into_iter().map(String::from).collect();
        v.extend(defaults.iter().cloned());
        v.extend(fallbacks.iter().cloned());
        v
    };
    let has_segoe = add(&mut fonts, "segoe", "segoeui.ttf");
    let has_sb = add(&mut fonts, "segoe-sb", "seguisb.ttf");
    let has_b = add(&mut fonts, "segoe-b", "segoeuib.ttf");
    let base = if has_segoe { Some("segoe") } else { None };
    fonts.families.insert(FontFamily::Proportional, fam(base));
    fonts.families.insert(FontFamily::Name("semibold".into()), fam(if has_sb { Some("segoe-sb") } else { base }));
    fonts.families.insert(FontFamily::Name("bold".into()), fam(if has_b { Some("segoe-b") } else { base }));
    let mono = fonts.families.entry(FontFamily::Monospace).or_default();
    for f in &fallbacks {
        mono.push(f.clone());
    }
    ctx.set_fonts(fonts);

    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(24, 27, 34);
    v.faint_bg_color = PANEL;
    v.override_text_color = None;
    v.hyperlink_color = ACCENT_HI;
    v.selection.bg_fill = ACCENT.linear_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.window_corner_radius = CornerRadius::same(12);
    v.window_stroke = Stroke::new(1.0, LINE);
    v.window_shadow = Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(120) };
    v.popup_shadow = Shadow { offset: [0, 6], blur: 16, spread: 0, color: Color32::from_black_alpha(110) };
    v.menu_corner_radius = CornerRadius::same(10);
    let r = CornerRadius::same(8);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, SUB);
    v.widgets.inactive.bg_fill = CARD;
    v.widgets.inactive.weak_bg_fill = CARD;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.bg_fill = CARD_HI;
    v.widgets.hovered.weak_bg_fill = CARD_HI;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(60, 66, 80));
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.active.bg_fill = ACCENT.linear_multiply(0.6);
    v.widgets.active.weak_bg_fill = ACCENT.linear_multiply(0.6);
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    ctx.set_visuals(v);

    ctx.global_style_mut(|s| {
        use egui::TextStyle::*;
        s.text_styles.insert(Body, regular(14.5));
        s.text_styles.insert(Button, regular(14.0));
        s.text_styles.insert(Small, regular(12.0));
        s.text_styles.insert(Heading, bold(24.0));
        s.spacing.item_spacing = Vec2::new(8.0, 8.0);
        s.spacing.button_padding = Vec2::new(12.0, 6.0);
        s.spacing.interact_size.y = 30.0;
        s.spacing.scroll.bar_width = 8.0;
        s.spacing.scroll.floating = true;
    });
}

/// Vertical gradient (no rounding).
pub fn vgradient(p: &Painter, r: Rect, top: Color32, bottom: Color32) {
    let mut m = epaint::Mesh::default();
    m.colored_vertex(r.left_top(), top);
    m.colored_vertex(r.right_top(), top);
    m.colored_vertex(r.left_bottom(), bottom);
    m.colored_vertex(r.right_bottom(), bottom);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 2, 3);
    p.add(m);
}

/// Horizontal gradient (no rounding).
pub fn hgradient(p: &Painter, r: Rect, left: Color32, right: Color32) {
    let mut m = epaint::Mesh::default();
    m.colored_vertex(r.left_top(), left);
    m.colored_vertex(r.right_top(), right);
    m.colored_vertex(r.left_bottom(), left);
    m.colored_vertex(r.right_bottom(), right);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 2, 3);
    p.add(m);
}

pub fn shadow(p: &Painter, r: Rect, radius: u8, strength: u8) {
    let s = Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(strength) };
    p.add(s.as_shape(r, CornerRadius::same(radius)));
}

/// Small rounded label; returns its rect. `pos` is the left-top corner.
pub fn pill(p: &Painter, pos: Pos2, text: &str, font: FontId, fill: Color32, fg: Color32) -> Rect {
    let g = p.layout_no_wrap(text.to_string(), font, fg);
    let pad = Vec2::new(7.0, 3.0);
    let r = Rect::from_min_size(pos, g.size() + pad * 2.0);
    p.rect_filled(r, CornerRadius::same((r.height() / 2.0) as u8), fill);
    p.galley(r.min + pad, g, fg);
    r
}

/// Cover-fit UV: crop the texture so it fills `target` without stretching.
pub fn cover_uv(tex: Vec2, target: Vec2) -> Rect {
    let (ta, ra) = (tex.x / tex.y.max(1.0), target.x / target.y.max(1.0));
    if ta > ra {
        let w = ra / ta;
        Rect::from_min_max(Pos2::new((1.0 - w) / 2.0, 0.0), Pos2::new((1.0 + w) / 2.0, 1.0))
    } else {
        let h = ta / ra;
        // bias toward the top: faces and titles usually live there
        let y0 = (1.0 - h) * 0.3;
        Rect::from_min_max(Pos2::new(0.0, y0), Pos2::new(1.0, y0 + h))
    }
}
