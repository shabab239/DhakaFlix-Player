//! Pages: home, browse (shelf / poster grid), title page, settings. Plus the reusable widgets.

use crate::config::{Link, PlayerKind};
use crate::covers::{self, pick_cover};
use crate::meta::{self, parse_title, TitleInfo};
use crate::scrape::{human_size, natural_cmp, Entry};
use crate::theme::{self, *};
use crate::{Action, App, Route, SortMode, LIBRARY};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, Id, Margin, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind,
    TextureHandle, Ui, Vec2,
};
use std::sync::Arc;

// ------------------------------------------------------------------------------------------------
// Folder classification

#[derive(Clone)]
pub struct Item {
    pub entry: Entry,
    pub info: TitleInfo,
}

#[derive(Default)]
pub struct FolderView {
    pub titles: Vec<Item>,
    pub groups: Vec<Entry>,
    pub seasons: Vec<Entry>,
    pub videos: Vec<Entry>,
    pub images: Vec<Entry>,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Kind {
    Title,
    Shelf,
    Grid,
}

impl FolderView {
    pub fn build(entries: &[Entry]) -> Self {
        let mut v = FolderView::default();
        for e in entries {
            if e.is_dir {
                if meta::is_season(&e.name) {
                    v.seasons.push(e.clone());
                } else if let Some(info) = parse_title(&e.name) {
                    v.titles.push(Item { entry: e.clone(), info });
                } else {
                    v.groups.push(e.clone());
                }
            } else if e.is_video() {
                v.videos.push(e.clone());
            } else if e.is_image() {
                v.images.push(e.clone());
            }
        }
        v.seasons.sort_by(|a, b| natural_cmp(&a.name, &b.name));
        v.videos.sort_by(|a, b| natural_cmp(&a.name, &b.name));
        v
    }

    pub fn kind(&self) -> Kind {
        if !self.videos.is_empty() || !self.seasons.is_empty() {
            Kind::Title
        } else if !self.groups.is_empty() && self.groups.len() >= self.titles.len() {
            Kind::Shelf
        } else {
            Kind::Grid
        }
    }
}

pub fn decoded_last_segment(url: &str) -> String {
    let seg = url.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    percent_encoding::percent_decode_str(seg).decode_utf8_lossy().into_owned()
}

pub fn title_info_for(url: &str) -> TitleInfo {
    let name = decoded_last_segment(url);
    parse_title(&name).unwrap_or(TitleInfo { title: meta::group_label(&name), ..Default::default() })
}

/// Root URL of the library containing `url`.
pub fn library_root(url: &str) -> Option<&'static str> {
    library_label(url).map(|(_, u)| u)
}

fn library_label(url: &str) -> Option<(&'static str, &'static str)> {
    LIBRARY
        .iter()
        .flat_map(|(_, items)| items.iter())
        .filter(|(_, u)| url.starts_with(*u))
        .max_by_key(|(_, u)| u.len())
        .map(|(l, u)| (*l, *u))
}

/// Breadcrumbs: Home › Library › group › title
pub fn crumbs(url: &str) -> Vec<(String, Route)> {
    let mut out = vec![("Home".to_string(), Route::Home)];
    let (start, label) = match library_label(url) {
        Some((l, u)) => (u.to_string(), l.to_string()),
        None => {
            let host = reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(String::from)).unwrap_or_default();
            let root = reqwest::Url::parse(url).ok().map(|mut u| {
                u.set_path("/");
                u.to_string()
            });
            (root.unwrap_or_default(), host)
        }
    };
    out.push((label, Route::Folder(start.clone())));
    let rest = url.strip_prefix(&start).unwrap_or("");
    let mut acc = start;
    for seg in rest.split('/').filter(|s| !s.is_empty()) {
        acc.push_str(seg);
        acc.push('/');
        let name = percent_encoding::percent_decode_str(seg).decode_utf8_lossy().into_owned();
        let label = parse_title(&name).map(|t| t.title).unwrap_or_else(|| meta::group_label(&name));
        out.push((label, Route::Folder(acc.clone())));
    }
    out
}

// ------------------------------------------------------------------------------------------------
// Small widgets

pub enum Btn {
    Primary,
    Ghost,
}

pub fn button(ui: &mut Ui, text: &str, kind: Btn, h: f32) -> Response {
    let font = semibold(if h >= 40.0 { 15.5 } else { 13.5 });
    let fg = match kind {
        Btn::Primary => Color32::WHITE,
        Btn::Ghost => TEXT,
    };
    let g = ui.painter().layout_no_wrap(text.to_string(), font, fg);
    let size = Vec2::new(g.size().x + h * 0.9, h);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hov = resp.hovered();
    let r = CornerRadius::same((h / 2.0) as u8);
    let p = ui.painter();
    match kind {
        Btn::Primary => {
            p.rect_filled(rect, r, if hov { ACCENT_HI } else { ACCENT });
        }
        Btn::Ghost => {
            p.rect_filled(rect, r, if hov { Color32::from_white_alpha(38) } else { Color32::from_white_alpha(20) });
        }
    }
    p.galley(rect.center() - g.size() / 2.0, g, fg);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn icon_button(ui: &mut Ui, icon: &str, enabled: bool, tip: &str) -> Response {
    let size = Vec2::splat(32.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let hov = enabled && resp.hovered();
    if hov {
        ui.painter().circle_filled(rect.center(), 16.0, CARD_HI);
    }
    let col = if enabled { TEXT } else { DIM.linear_multiply(0.6) };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, regular(16.0), col);
    let resp = resp.on_hover_text(tip);
    if enabled { resp.on_hover_cursor(CursorIcon::PointingHand) } else { resp }
}

pub fn chip(ui: &mut Ui, label: &str, selected: bool) -> Response {
    let font = if selected { semibold(13.5) } else { regular(13.5) };
    let fg = if selected { Color32::WHITE } else { SUB };
    let g = ui.painter().layout_no_wrap(label.to_string(), font, fg);
    let size = Vec2::new(g.size().x + 26.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fill = if selected {
        ACCENT
    } else if resp.hovered() {
        CARD_HI
    } else {
        CARD
    };
    ui.painter().rect_filled(rect, CornerRadius::same(16), fill);
    let fg = if resp.hovered() && !selected { TEXT } else { fg };
    ui.painter().galley_with_override_text_color(rect.center() - g.size() / 2.0, g, fg);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn section_title(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(text).font(bold(19.0)).color(TEXT));
    ui.add_space(2.0);
}

fn play_glyph(p: &egui::Painter, c: Pos2, r: f32, col: Color32) {
    let pts = vec![
        Pos2::new(c.x - r * 0.38, c.y - r * 0.55),
        Pos2::new(c.x + r * 0.6, c.y),
        Pos2::new(c.x - r * 0.38, c.y + r * 0.55),
    ];
    p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
}

fn wrapped(ui: &Ui, text: &str, font: egui::FontId, color: Color32, width: f32, rows: usize) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(text.to_string(), font, color, width);
    job.wrap.max_rows = rows;
    job.wrap.break_anywhere = false;
    job.wrap.overflow_character = Some('…');
    ui.painter().layout_job(job)
}

fn initials(s: &str) -> String {
    s.split_whitespace()
        .filter(|w| w.chars().next().is_some_and(|c| c.is_alphanumeric()))
        .take(2)
        .filter_map(|w| w.chars().next())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

pub fn player_switch(app: &mut App, ui: &mut Ui) {
    let kinds: Vec<PlayerKind> =
        PlayerKind::ALL.into_iter().filter(|k| *k != PlayerKind::Custom || !app.cfg.custom_path.is_empty()).collect();
    let h = 30.0;
    let font = semibold(12.5);
    let widths: Vec<f32> = kinds.iter().map(|k| ui.painter().layout_no_wrap(k.label().into(), font.clone(), TEXT).size().x + 22.0).collect();
    let total: f32 = widths.iter().sum::<f32>() + 6.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(total, h), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(15), CARD);
    let mut x = rect.left() + 3.0;
    for (k, w) in kinds.iter().zip(widths) {
        let r = Rect::from_min_size(Pos2::new(x, rect.top() + 3.0), Vec2::new(w, h - 6.0));
        let resp = ui.interact(r, Id::unique(("player", k.label())), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        let sel = app.cfg.player == *k;
        if sel {
            ui.painter().rect_filled(r, CornerRadius::same(12), ACCENT);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(12), CARD_HI);
        }
        ui.painter().text(r.center(), Align2::CENTER_CENTER, k.label(), font.clone(), if sel { Color32::WHITE } else { SUB });
        if resp.clicked() && !sel {
            app.cfg.player = *k;
            app.cfg.save();
        }
        x += w;
    }
}

// ------------------------------------------------------------------------------------------------
// Poster card

pub struct CardSpec {
    pub id: String,
    pub folder: String,
    pub name: String,
    pub info: Option<TitleInfo>,
    pub subtitle: Option<String>,
    pub cover: Option<String>,
}

pub fn card_height(w: f32) -> f32 {
    w * 1.5 + 66.0
}

/// Draws one poster card in `rect` (width = poster width). Returns the click response.
pub fn poster_card(app: &mut App, ui: &mut Ui, rect: Rect, c: &CardSpec) -> Response {
    let id = Id::unique(("card", &c.id));
    let resp = ui.interact(rect, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let t = ui.ctx().animate_bool_with_time(id, resp.hovered(), 0.14);
    let ph = rect.width() * 1.5;
    let base = Rect::from_min_size(rect.min, Vec2::new(rect.width(), ph));
    let prect = base.expand(3.0 * t).translate(Vec2::new(0.0, -4.0 * t));
    let p = ui.painter().clone();
    theme::shadow(&p, prect, 10, (70.0 + 80.0 * t) as u8);

    let cover = c.cover.clone().or_else(|| app.cover_for(&c.folder));
    let tex = cover.as_ref().and_then(|u| app.tex_get("t", u));
    let title = c.info.as_ref().map(|i| i.title.as_str()).unwrap_or(&c.name);
    paint_poster(ui, &p, prect, tex.as_ref(), title, 10);

    // badges
    let mut x = prect.left() + 8.0;
    if let Some(i) = &c.info {
        if let Some(r) = i.rank {
            x = pill(&p, Pos2::new(x, prect.top() + 8.0), &format!("#{r}"), bold(11.5), GOLD, Color32::from_rgb(40, 30, 0)).right() + 4.0;
        }
        if let Some(q) = &i.quality {
            let hd = q == "1080p" || q == "2160p" || q == "4K";
            let fill = if hd { ACCENT } else { Color32::from_black_alpha(170) };
            pill(&p, Pos2::new(x, prect.top() + 8.0), q, semibold(11.0), fill, Color32::WHITE);
        }
        if i.series {
            let g = p.layout_no_wrap("SERIES".into(), semibold(10.5), Color32::WHITE);
            let w = g.size().x + 14.0;
            pill(&p, Pos2::new(prect.right() - 8.0 - w, prect.top() + 8.0), "SERIES", semibold(10.5), Color32::from_black_alpha(170), Color32::WHITE);
        }
    }
    if t > 0.01 {
        p.rect_filled(prect, CornerRadius::same(10), Color32::from_black_alpha((90.0 * t) as u8));
        let r = 24.0 * (0.8 + 0.2 * t);
        p.circle_filled(prect.center(), r, ACCENT.linear_multiply(t));
        play_glyph(&p, prect.center(), r * 0.75, Color32::WHITE.linear_multiply(t));
        p.rect_stroke(prect, CornerRadius::same(10), Stroke::new(2.0, ACCENT.linear_multiply(t)), StrokeKind::Outside);
    }

    // text
    let ty = base.bottom() + 8.0;
    let g = wrapped(ui, title, semibold(14.0), TEXT, rect.width(), 2);
    let gh = g.size().y;
    p.galley(Pos2::new(rect.left(), ty), g, TEXT);
    let sub = c.subtitle.clone().or_else(|| {
        c.info.as_ref().map(|i| {
            let mut s = i.year.clone().unwrap_or_default();
            if let Some(tag) = i.tags.first() {
                if !s.is_empty() {
                    s.push_str("  ·  ");
                }
                s.push_str(tag);
            }
            s
        })
    });
    if let Some(s) = sub.filter(|s| !s.is_empty()) {
        let g = wrapped(ui, &s, regular(12.0), SUB, rect.width(), 1);
        p.galley(Pos2::new(rect.left(), ty + gh + 1.0), g, SUB);
    }
    resp
}

pub fn paint_poster(ui: &Ui, p: &egui::Painter, r: Rect, tex: Option<&TextureHandle>, title: &str, radius: u8) {
    if let Some(tex) = tex {
        let uv = cover_uv(tex.size_vec2(), r.size());
        egui::Image::from_texture(egui::load::SizedTexture::from_handle(tex)).uv(uv).corner_radius(radius).paint_at(ui, r);
    } else {
        let tint = tint_for(title);
        p.rect_filled(r, CornerRadius::same(radius), tint.linear_multiply(0.45));
        p.rect_stroke(r, CornerRadius::same(radius), Stroke::new(1.0, Color32::from_white_alpha(14)), StrokeKind::Inside);
        p.text(r.center() - Vec2::new(0.0, 8.0), Align2::CENTER_CENTER, initials(title), bold(r.width() * 0.22), Color32::from_white_alpha(170));
        let g = wrapped(ui, title, semibold(12.0), Color32::from_white_alpha(150), r.width() - 20.0, 2);
        p.galley(Pos2::new(r.center().x - g.size().x / 2.0, r.bottom() - 14.0 - g.size().y), g, Color32::WHITE);
    }
}

/// Lays out `n` cards in a responsive grid; only draws what's on screen.
fn grid(app: &mut App, ui: &mut Ui, n: usize, mut spec: impl FnMut(&mut App, usize) -> (CardSpec, Route)) {
    if n == 0 {
        return;
    }
    let avail = ui.available_width();
    let min_w = app.cfg.card_size.clamp(120.0, 260.0);
    let gap = 20.0;
    let cols = (((avail + gap) / (min_w + gap)).floor() as usize).max(1);
    let w = ((avail - gap * (cols as f32 - 1.0)) / cols as f32).floor();
    let ch = card_height(w);
    let rows = n.div_ceil(cols);
    let total = rows as f32 * ch + (rows.saturating_sub(1)) as f32 * 14.0;
    let (area, _) = ui.allocate_exact_size(Vec2::new(avail, total), Sense::hover());
    let clip = ui.clip_rect();
    let prefetch = clip.expand2(Vec2::new(0.0, clip.height() * 0.8));
    for i in 0..n {
        let (row, col) = (i / cols, i % cols);
        let r = Rect::from_min_size(
            Pos2::new(area.left() + col as f32 * (w + gap), area.top() + row as f32 * (ch + 14.0)),
            Vec2::new(w, ch),
        );
        if !prefetch.intersects(r) {
            continue;
        }
        let (c, route) = spec(app, i);
        if !clip.intersects(r) {
            app.cover_for(&c.folder); // warm up covers just below the fold
            continue;
        }
        let resp = poster_card(app, ui, r, &c);
        if resp.clicked() {
            app.act(Action::Nav(route.clone()));
        }
        let folder = c.folder.clone();
        let name = c.info.as_ref().map(|i| i.title.clone()).unwrap_or_else(|| c.name.clone());
        let cover = c.cover.clone();
        resp.context_menu(|ui| {
            if ui.button("Open").clicked() {
                app.act(Action::Nav(route.clone()));
                ui.close();
            }
            let fav = app.is_fav(&folder);
            if ui.button(if fav { "♥  Remove from favorites" } else { "♡  Add to favorites" }).clicked() {
                app.act(Action::ToggleFav(Link { name: name.clone(), url: folder.clone(), cover: cover.clone(), ..Default::default() }));
                ui.close();
            }
            if ui.button("Copy link").clicked() {
                app.act(Action::Copy(folder.clone()));
                ui.close();
            }
        });
    }
}

// ------------------------------------------------------------------------------------------------
// Pages

pub fn page_margin() -> Margin {
    Margin { left: 32, right: 32, top: 20, bottom: 28 }
}

pub fn home(app: &mut App, ui: &mut Ui) {
    let w = ui.available_width();
    hero(app, ui, w);
    egui::Frame::new().inner_margin(page_margin()).show(ui, |ui| {
        // Continue watching
        let mut seen = std::collections::HashSet::new();
        let cont: Vec<Link> = app
            .cfg
            .history
            .iter()
            .filter(|h| seen.insert(h.folder.clone().unwrap_or_else(|| h.url.clone())))
            .take(14)
            .cloned()
            .collect();
        if !cont.is_empty() {
            section_title(ui, "Continue watching");
            row(app, ui, "cont", cont.len(), |_, i| {
                let h = &cont[i];
                let spec = CardSpec {
                    id: format!("cont:{}", h.url),
                    folder: h.folder.clone().unwrap_or_default(),
                    name: h.title.clone().unwrap_or_else(|| meta::strip_ext(&h.name).to_string()),
                    info: None,
                    subtitle: Some(episode_label(&h.name).unwrap_or_else(|| "Movie".into())),
                    cover: h.cover.clone(),
                };
                (spec, Action::Play(vec![h.clone()], None))
            });
            ui.add_space(18.0);
        }
        let favs = app.cfg.favorites.clone();
        if !favs.is_empty() {
            section_title(ui, "Favorites");
            row(app, ui, "favs", favs.len(), |_, i| {
                let f = &favs[i];
                let info = parse_title(&decoded_last_segment(&f.url));
                let spec = CardSpec {
                    id: format!("fav:{}", f.url),
                    folder: f.url.clone(),
                    name: f.name.clone(),
                    subtitle: if info.is_some() { None } else { Some("Collection".into()) },
                    info,
                    cover: f.cover.clone(),
                };
                (spec, Action::Nav(Route::Folder(f.url.clone())))
            });
            ui.add_space(18.0);
        }
        for (group, items) in LIBRARY {
            section_title(ui, group);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(14.0, 14.0);
                for (label, url) in *items {
                    if library_tile(app, ui, label, url).clicked() {
                        app.act(Action::Nav(Route::Folder((*url).to_string())));
                    }
                }
            });
            ui.add_space(16.0);
        }
    });
}

fn episode_label(name: &str) -> Option<String> {
    let (s, e) = meta::episode(name)?;
    Some(match s {
        Some(s) => format!("S{s:02} · E{e:02}"),
        None => format!("Episode {e}"),
    })
}

/// Horizontal strip of cards (shift + wheel or drag the bar to scroll).
fn row(app: &mut App, ui: &mut Ui, id: &str, n: usize, mut spec: impl FnMut(&mut App, usize) -> (CardSpec, Action)) {
    let w = 150.0;
    let gap = 18.0;
    egui::ScrollArea::horizontal().id_salt(id).auto_shrink([false, true]).show(ui, |ui| {
        let (area, _) = ui.allocate_exact_size(Vec2::new(n as f32 * (w + gap), card_height(w) + 8.0), Sense::hover());
        let clip = ui.clip_rect();
        for i in 0..n {
            let r = Rect::from_min_size(Pos2::new(area.left() + i as f32 * (w + gap), area.top() + 6.0), Vec2::new(w, card_height(w)));
            if !clip.intersects(r) {
                continue;
            }
            let (c, action) = spec(app, i);
            let resp = poster_card(app, ui, r, &c);
            let folder = c.folder.clone();
            if resp.clicked() {
                app.act(action);
            }
            if !folder.is_empty() {
                resp.context_menu(|ui| {
                    if ui.button("Go to title").clicked() {
                        app.act(Action::Nav(Route::Folder(folder.clone())));
                        ui.close();
                    }
                });
            }
        }
    });
}

fn library_tile(app: &mut App, ui: &mut Ui, label: &str, url: &str) -> Response {
    let size = Vec2::new(236.0, 92.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let t = ui.ctx().animate_bool_with_time(Id::unique(("tile", url)), resp.hovered(), 0.12);
    let r = rect.translate(Vec2::new(0.0, -3.0 * t));
    let tint = tint_for(label);
    let p = ui.painter();
    theme::shadow(p, r, 12, (40.0 + 60.0 * t) as u8);
    p.rect_filled(r, CornerRadius::same(12), tint.linear_multiply(0.55 + 0.15 * t));
    p.rect_stroke(r, CornerRadius::same(12), Stroke::new(1.0, Color32::from_white_alpha(18 + (30.0 * t) as u8)), StrokeKind::Inside);
    let icon = if url.contains("Series") || url.contains("TV") || url.contains("Documentary") || url.contains("Wrestling") || url.contains("Tutorial") {
        "📺"
    } else {
        "🎬"
    };
    p.text(r.right_center() - Vec2::new(22.0, 0.0), Align2::RIGHT_CENTER, icon, regular(34.0), Color32::from_white_alpha(60));
    p.text(r.left_top() + Vec2::new(16.0, 18.0), Align2::LEFT_TOP, label, bold(16.5), Color32::WHITE);
    let status = match (app.listings.get(url), app.list_err.contains_key(url)) {
        (Some(l), _) => {
            let n = l.iter().filter(|e| e.is_dir).count();
            format!("{n} folders  ·  {}", host_of(url))
        }
        (None, true) => "Offline".to_string(),
        (None, false) => "Checking…".to_string(),
    };
    let col = if app.list_err.contains_key(url) { Color32::from_rgb(255, 190, 180) } else { Color32::from_white_alpha(170) };
    p.text(r.left_bottom() + Vec2::new(16.0, -18.0), Align2::LEFT_BOTTOM, status, regular(12.5), col);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn host_of(url: &str) -> String {
    reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(String::from)).unwrap_or_default()
}

fn hero(app: &mut App, ui: &mut Ui, w: f32) {
    let h = 300.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let p = ui.painter_at(rect);
    let last = app.cfg.history.first().cloned();
    let cover = last.as_ref().and_then(|l| l.cover.clone());
    let bd = cover.as_ref().and_then(|c| app.tex_get("b", c));
    paint_backdrop(&p, rect, bd.as_ref());
    if bd.is_none() {
        theme::hgradient(&p, rect, Color32::from_rgb(70, 30, 60), Color32::from_rgb(20, 40, 70));
        theme::vgradient(&p, rect, Color32::TRANSPARENT, BG);
    }
    let left = rect.left() + 36.0;
    match last {
        Some(l) => {
            let poster = cover.as_ref().and_then(|c| app.tex_get("p", c));
            let pr = Rect::from_min_size(Pos2::new(left, rect.top() + 40.0), Vec2::new(150.0, 225.0));
            theme::shadow(&p, pr, 10, 160);
            let title = l.title.clone().unwrap_or_else(|| meta::strip_ext(&l.name).to_string());
            paint_poster(ui, &p, pr, poster.as_ref(), &title, 10);
            let x = pr.right() + 28.0;
            let mut y = rect.top() + 70.0;
            p.text(Pos2::new(x, y), Align2::LEFT_TOP, "CONTINUE WATCHING", semibold(12.0), ACCENT_HI);
            y += 22.0;
            let g = wrapped(ui, &title, bold(34.0), Color32::WHITE, (rect.right() - x - 40.0).max(200.0), 2);
            let gh = g.size().y;
            p.galley(Pos2::new(x, y), g, Color32::WHITE);
            y += gh + 6.0;
            let sub = episode_label(&l.name).unwrap_or_else(|| meta::strip_ext(&l.name).to_string());
            let g = wrapped(ui, &sub, regular(14.0), SUB, (rect.right() - x - 40.0).max(200.0), 1);
            p.galley(Pos2::new(x, y), g, SUB);
            y += 34.0;
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(Pos2::new(x, y), Vec2::new(600.0, 48.0))));
            child.horizontal(|ui| {
                if button(ui, &format!("▶  Play in {}", app.cfg.player.label()), Btn::Primary, 44.0).clicked() {
                    app.act(Action::Play(vec![l.clone()], None));
                }
                if let Some(f) = l.folder.clone() {
                    if button(ui, "Open title", Btn::Ghost, 44.0).clicked() {
                        app.act(Action::Nav(Route::Folder(f)));
                    }
                }
            });
        }
        None => {
            p.text(Pos2::new(left, rect.top() + 90.0), Align2::LEFT_TOP, "Welcome to DhakaFlix", bold(38.0), Color32::WHITE);
            p.text(
                Pos2::new(left, rect.top() + 145.0),
                Align2::LEFT_TOP,
                "Movies and series from your local servers. Pick something, it streams straight to your player.",
                regular(15.0),
                SUB,
            );
        }
    }
}

fn paint_backdrop(p: &egui::Painter, rect: Rect, bd: Option<&TextureHandle>) {
    if let Some(bd) = bd {
        let uv = cover_uv(bd.size_vec2(), rect.size());
        p.image(bd.id(), rect, uv, Color32::from_gray(150));
        theme::hgradient(p, rect, Color32::from_black_alpha(170), Color32::from_black_alpha(20));
        let bottom = Rect::from_min_max(Pos2::new(rect.left(), rect.top() + rect.height() * 0.45), rect.max);
        theme::vgradient(p, bottom, Color32::TRANSPARENT, BG);
    }
}

pub fn folder(app: &mut App, ui: &mut Ui, url: &str) {
    let Some(view) = app.view(url) else {
        egui::Frame::new().inner_margin(page_margin()).show(ui, |ui| {
            if let Some(e) = app.list_err.get(url).cloned() {
                ui.add_space(40.0);
                ui.label(egui::RichText::new("Couldn't open this folder").font(bold(22.0)));
                ui.label(egui::RichText::new(e).color(SUB));
                ui.add_space(8.0);
                if button(ui, "Try again", Btn::Primary, 36.0).clicked() {
                    app.act(Action::Reload(url.to_string()));
                }
            } else {
                ui.add_space(60.0);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(egui::RichText::new("Loading…").color(SUB));
                });
            }
        });
        return;
    };
    match view.kind() {
        Kind::Title => title_page(app, ui, url, &view),
        Kind::Shelf => shelf(app, ui, url, &view),
        Kind::Grid => egui::Frame::new().inner_margin(page_margin()).show(ui, |ui| {
            header(app, ui, url, &view);
            browse_body(app, ui, &view, url);
        }).inner,
    }
}

fn header(app: &mut App, ui: &mut Ui, url: &str, view: &FolderView) {
    let name = library_label(url).filter(|(_, u)| *u == url).map(|(l, _)| l.to_string()).unwrap_or_else(|| title_info_for(url).title);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(&name).font(bold(28.0)).color(TEXT));
        ui.add_space(6.0);
        let n = view.titles.len();
        if n > 0 {
            ui.label(egui::RichText::new(format!("{n} title{}", if n == 1 { "" } else { "s" })).color(SUB));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let fav = app.is_fav(url);
            if button(ui, if fav { "♥  Saved" } else { "♡  Save" }, Btn::Ghost, 32.0).clicked() {
                app.act(Action::ToggleFav(Link::new(name.clone(), url)));
            }
            sort_switch(app, ui);
        });
    });
    ui.add_space(6.0);
}

fn sort_switch(app: &mut App, ui: &mut Ui) {
    // right_to_left layout: add in reverse order
    for (m, l) in [(SortMode::Added, "Recently added"), (SortMode::Year, "Year"), (SortMode::Name, "A–Z")] {
        if chip(ui, l, app.sort == m).clicked() {
            app.sort = m;
        }
    }
    ui.label(egui::RichText::new("Sort").color(DIM));
}

fn browse_body(app: &mut App, ui: &mut Ui, view: &FolderView, _url: &str) {
    if !view.groups.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for g in &view.groups {
                if chip(ui, &meta::group_label(&g.name), false).clicked() {
                    app.act(Action::Nav(Route::Folder(g.url.clone())));
                }
            }
        });
        ui.add_space(10.0);
    }
    let mut idx: Vec<usize> = (0..view.titles.len()).collect();
    let t = &view.titles;
    match app.sort {
        SortMode::Name => idx.sort_by(|&a, &b| natural_cmp(&t[a].entry.name, &t[b].entry.name)),
        SortMode::Year => idx.sort_by(|&a, &b| t[b].info.year.cmp(&t[a].info.year).then_with(|| natural_cmp(&t[a].info.title, &t[b].info.title))),
        SortMode::Added => idx.sort_by(|&a, &b| t[b].entry.modified.cmp(&t[a].entry.modified)),
    }
    if idx.is_empty() && !view.titles.is_empty() {
        ui.add_space(30.0);
        ui.label(egui::RichText::new(format!("Nothing here matches “{}”.", app.filter)).color(SUB));
    }
    ui.add_space(4.0);
    let titles = &view.titles;
    grid(app, ui, idx.len(), |_app, i| {
        let it = &titles[idx[i]];
        (
            CardSpec {
                id: it.entry.url.clone(),
                folder: it.entry.url.clone(),
                name: it.entry.name.clone(),
                info: Some(it.info.clone()),
                subtitle: None,
                cover: None,
            },
            Route::Folder(it.entry.url.clone()),
        )
    });
    if !view.videos.is_empty() {
        ui.add_space(16.0);
        section_title(ui, "Files");
        file_list(app, ui, &view.videos, None, None, None);
    }
}

fn shelf(app: &mut App, ui: &mut Ui, url: &str, view: &FolderView) {
    egui::Frame::new().inner_margin(page_margin()).show(ui, |ui| {
        // newest year first, then named groups, then this folder's own loose titles
        let mut years: Vec<(String, String)> = vec![];
        let mut named: Vec<(String, String)> = vec![];
        for g in &view.groups {
            let item = (meta::group_label(&g.name), g.url.clone());
            if meta::looks_like_year_group(&g.name) { years.push(item) } else { named.push(item) }
        }
        years.sort_by(|a, b| natural_cmp(&b.0, &a.0));
        let mut groups = years;
        groups.extend(named);
        const OWN: &str = "#own";
        if !view.titles.is_empty() {
            groups.push(("Collections".to_string(), OWN.to_string()));
        }
        let sel = app.shelf_sel.get(url).cloned().filter(|s| groups.iter().any(|g| &g.1 == s)).unwrap_or_else(|| groups[0].1.clone());
        let child = if sel == OWN {
            Some(Arc::new(FolderView { titles: view.titles.clone(), videos: view.videos.clone(), ..Default::default() }))
        } else {
            app.view(&sel)
        };
        let empty = FolderView::default();
        header(app, ui, url, child.as_deref().unwrap_or(&empty));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
            for (label, gurl) in &groups {
                if chip(ui, label, *gurl == sel).clicked() {
                    app.shelf_sel.insert(url.to_string(), gurl.clone());
                }
            }
        });
        ui.add_space(14.0);
        match child {
            Some(c) if c.kind() == Kind::Title => {
                // a group that is really one title: just link to it
                if button(ui, "Open", Btn::Primary, 36.0).clicked() {
                    app.act(Action::Nav(Route::Folder(sel.clone())));
                }
            }
            Some(c) => browse_body(app, ui, &c, &sel),
            None => {
                if let Some(e) = app.list_err.get(&sel).cloned() {
                    ui.label(egui::RichText::new(format!("Couldn't load: {e}")).color(SUB));
                    if button(ui, "Try again", Btn::Primary, 34.0).clicked() {
                        app.act(Action::Reload(sel.clone()));
                    }
                } else {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(egui::RichText::new("Loading…").color(SUB));
                    });
                }
            }
        }
    });
}

fn title_page(app: &mut App, ui: &mut Ui, url: &str, view: &FolderView) {
    let info = title_info_for(url);
    let cover = pick_cover(&view.images.iter().cloned().collect::<Vec<_>>()).or_else(|| app.cover_for(url));
    let w = ui.available_width();
    let hero_h = 400.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, hero_h), Sense::hover());
    let p = ui.painter_at(rect);
    let bd = cover.as_ref().and_then(|c| app.tex_get("b", c));
    paint_backdrop(&p, rect, bd.as_ref());
    if bd.is_none() {
        let tint = tint_for(&info.title);
        theme::hgradient(&p, rect, tint.linear_multiply(0.35), BG);
        theme::vgradient(&p, rect, Color32::TRANSPARENT, BG);
    }
    let poster = cover.as_ref().and_then(|c| app.tex_get("p", c));
    let pr = Rect::from_min_size(Pos2::new(rect.left() + 36.0, rect.top() + 36.0), Vec2::new(220.0, 330.0));
    theme::shadow(&p, pr, 12, 170);
    paint_poster(ui, &p, pr, poster.as_ref(), &info.title, 12);

    let x = pr.right() + 32.0;
    let tw = (rect.right() - x - 36.0).max(240.0);
    let mut y = rect.top() + 60.0;
    let kind_label = if !view.seasons.is_empty() || info.series {
        let n = view.seasons.len();
        if n > 0 { format!("SERIES  ·  {n} SEASON{}", if n == 1 { "" } else { "S" }) } else { "SERIES".into() }
    } else {
        "MOVIE".into()
    };
    let mut kx = x;
    if let Some(r) = info.rank {
        kx = pill(&p, Pos2::new(kx, y - 2.0), &format!("IMDb Top 250  #{r}"), bold(11.5), GOLD, Color32::from_rgb(40, 30, 0)).right() + 10.0;
    }
    p.text(Pos2::new(kx, y), Align2::LEFT_TOP, kind_label, semibold(12.0), ACCENT_HI);
    y += 24.0;
    let g = wrapped(ui, &info.title, bold(38.0), Color32::WHITE, tw, 2);
    let gh = g.size().y;
    p.galley(Pos2::new(x, y), g, Color32::WHITE);
    y += gh + 10.0;
    let mut px = x;
    if let Some(yr) = &info.year {
        px = pill(&p, Pos2::new(px, y), yr, semibold(12.5), Color32::from_white_alpha(28), TEXT).right() + 6.0;
    }
    if let Some(q) = &info.quality {
        px = pill(&p, Pos2::new(px, y), q, semibold(12.5), ACCENT, Color32::WHITE).right() + 6.0;
    }
    for t in &info.tags {
        px = pill(&p, Pos2::new(px, y), t, regular(12.5), Color32::from_white_alpha(28), TEXT).right() + 6.0;
    }
    let total: u64 = view.videos.iter().map(|v| v.bytes).sum();
    if total > 0 {
        let s = if view.videos.len() == 1 { human_size(total) } else { format!("{} files  ·  {}", view.videos.len(), human_size(total)) };
        p.text(Pos2::new(px + 6.0, y + 10.0), Align2::LEFT_CENTER, s, regular(13.0), SUB);
    }
    y += 44.0;

    // which videos does the big button play?
    let season_url = if view.seasons.is_empty() {
        None
    } else {
        let sel = app.season_sel.get(url).cloned().filter(|s| view.seasons.iter().any(|x| &x.url == s));
        Some(sel.unwrap_or_else(|| view.seasons[0].url.clone()))
    };
    let season_view = season_url.as_ref().and_then(|s| app.view(s));
    let videos: Vec<Entry> = match (&season_url, &season_view) {
        (Some(_), Some(sv)) => sv.videos.clone(),
        (Some(_), None) => vec![],
        (None, _) => view.videos.clone(),
    };
    let last = app.cfg.history.iter().find(|h| h.folder.as_deref() == Some(url)).map(|h| h.url.clone());
    let resume = last.as_ref().and_then(|u| videos.iter().position(|v| &v.url == u));

    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(Pos2::new(x, y), Vec2::new(tw, 50.0))));
    child.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        if !videos.is_empty() {
            let (label, start) = match resume {
                Some(i) if videos.len() > 1 => (format!("▶  Continue  {}", episode_label(&videos[i].name).unwrap_or_else(|| format!("#{}", i + 1))), i),
                _ if videos.len() > 1 && season_url.is_some() => (format!("▶  Play  {}", episode_label(&videos[0].name).unwrap_or_else(|| "from start".into())), 0),
                _ => ("▶  Play".to_string(), 0),
            };
            if button(ui, &label, Btn::Primary, 46.0).clicked() {
                let items = links_for(&videos[start..], url, &info, cover.clone());
                app.act(Action::Play(items, None));
            }
        }
        player_switch(app, ui);
        let fav = app.is_fav(url);
        if button(ui, if fav { "♥  Saved" } else { "♡  Save" }, Btn::Ghost, 38.0).clicked() {
            app.act(Action::ToggleFav(Link { name: info.title.clone(), url: url.to_string(), cover: cover.clone(), ..Default::default() }));
        }
    });

    egui::Frame::new().inner_margin(Margin { top: 0, ..page_margin() }).show(ui, |ui| {
        if let Some(sel) = &season_url {
            ui.horizontal_wrapped(|ui| {
                for s in &view.seasons {
                    if chip(ui, s.name.trim(), &s.url == sel).clicked() {
                        app.season_sel.insert(url.to_string(), s.url.clone());
                    }
                }
            });
            ui.add_space(12.0);
            match &season_view {
                Some(sv) => {
                    if sv.videos.len() > 1 {
                        section_title(ui, &format!("{} episodes", sv.videos.len()));
                    }
                    file_list(app, ui, &sv.videos, Some(url), Some(&info), cover.clone());
                }
                None => {
                    if let Some(e) = app.list_err.get(sel).cloned() {
                        ui.label(egui::RichText::new(format!("Couldn't load this season: {e}")).color(SUB));
                    } else {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(egui::RichText::new("Loading episodes…").color(SUB));
                        });
                    }
                }
            }
        } else {
            if !view.videos.is_empty() {
                section_title(ui, if view.videos.len() == 1 { "File" } else { "Files" });
                file_list(app, ui, &view.videos, Some(url), Some(&info), cover.clone());
            }
            if !view.groups.is_empty() || !view.titles.is_empty() {
                ui.add_space(10.0);
                section_title(ui, "More in this folder");
                ui.horizontal_wrapped(|ui| {
                    for g in view.groups.iter().chain(view.titles.iter().map(|t| &t.entry)) {
                        if chip(ui, g.name.trim(), false).clicked() {
                            app.act(Action::Nav(Route::Folder(g.url.clone())));
                        }
                    }
                });
            }
        }
    });
}

fn links_for(videos: &[Entry], folder: &str, info: &TitleInfo, cover: Option<String>) -> Vec<Link> {
    videos
        .iter()
        .map(|v| Link {
            name: v.name.clone(),
            url: v.url.clone(),
            cover: cover.clone(),
            folder: Some(folder.to_string()),
            title: Some(info.title.clone()),
        })
        .collect()
}

/// Episode / file rows. Click plays from that row to the end (so the player auto-advances).
fn file_list(app: &mut App, ui: &mut Ui, videos: &[Entry], folder: Option<&str>, info: Option<&TitleInfo>, cover: Option<String>) {
    let w = ui.available_width();
    let last = folder.and_then(|f| app.cfg.history.iter().find(|h| h.folder.as_deref() == Some(f)).map(|h| h.url.clone()));
    let fallback_info = TitleInfo::default();
    let info = info.unwrap_or(&fallback_info);
    for (i, v) in videos.iter().enumerate() {
        let h = 58.0;
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
        let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
        let hov = resp.hovered();
        let p = ui.painter();
        let is_last = last.as_deref() == Some(&v.url);
        p.rect_filled(rect, CornerRadius::same(10), if hov { CARD_HI } else { CARD });
        if is_last {
            p.rect_stroke(rect, CornerRadius::same(10), Stroke::new(1.0, ACCENT.linear_multiply(0.7)), StrokeKind::Inside);
        }
        let badge = Rect::from_center_size(Pos2::new(rect.left() + 32.0, rect.center().y), Vec2::new(40.0, 40.0));
        if hov {
            p.circle_filled(badge.center(), 19.0, ACCENT);
            play_glyph(p, badge.center(), 14.0, Color32::WHITE);
        } else {
            p.rect_filled(badge, CornerRadius::same(8), Color32::from_white_alpha(14));
            let num = meta::episode(&v.name).map(|(_, e)| e).unwrap_or(i as u32 + 1);
            p.text(badge.center(), Align2::CENTER_CENTER, format!("{num:02}"), semibold(14.0), SUB);
        }
        let tx = badge.right() + 14.0;
        let tw = rect.right() - tx - 130.0;
        let ep = meta::episode(&v.name);
        let main = match ep {
            Some((_, e)) => format!("Episode {e}"),
            None => meta::strip_ext(&v.name).to_string(),
        };
        let g = wrapped(ui, &main, semibold(14.5), TEXT, tw, 1);
        p.galley(Pos2::new(tx, rect.top() + 10.0), g, TEXT);
        let detail = if ep.is_some() {
            v.name.clone()
        } else {
            let (q, tags) = meta::quality_and_tags(&v.name);
            let ext = v.name.rsplit_once('.').map(|(_, e)| e.to_ascii_uppercase()).unwrap_or_default();
            [Some(ext), q].into_iter().flatten().chain(tags).collect::<Vec<_>>().join("  ·  ")
        };
        let sub = if is_last { format!("Last played  ·  {detail}") } else { detail };
        let g = wrapped(ui, &sub, regular(12.0), if is_last { ACCENT_HI } else { DIM }, tw, 1);
        p.galley(Pos2::new(tx, rect.top() + 32.0), g, DIM);
        p.text(rect.right_center() - Vec2::new(18.0, 0.0), Align2::RIGHT_CENTER, human_size(v.bytes), regular(13.0), SUB);
        let items = || links_for(&videos[i..], folder.unwrap_or(""), info, cover.clone());
        if resp.clicked() {
            let mut it = items();
            if folder.is_none() {
                it.iter_mut().for_each(|l| l.folder = None);
            }
            app.act(Action::Play(it, None));
        }
        resp.context_menu(|ui| {
            for k in PlayerKind::ALL {
                if ui.button(format!("▶  Play in {}", k.label())).clicked() {
                    app.act(Action::Play(items(), Some(k)));
                    ui.close();
                }
            }
            if ui.button("Play only this file").clicked() {
                app.act(Action::Play(items().into_iter().take(1).collect(), None));
                ui.close();
            }
            if ui.button("Copy link").clicked() {
                app.act(Action::Copy(v.url.clone()));
                ui.close();
            }
        });
        ui.add_space(2.0);
    }
}

pub fn search(app: &mut App, ui: &mut Ui, query: &str) {
    use crate::index::{ago, libraries, SearchKind};
    const LIMIT: usize = 400;
    let ctx = ui.ctx().clone();
    app.index.refresh_if_stale(&ctx);
    egui::Frame::new().inner_margin(page_margin()).show(ui, |ui| {
        let (running, built_at, offline) = {
            let s = app.index.shared.lock().unwrap();
            (s.running, s.built_at, s.offline)
        };
        let indexed = app.index.hits.len();
        let libs = libraries();
        let (cards, total): (Vec<(CardSpec, Route)>, usize) = {
            let (hits, total) = app.index.search(query, app.search_kind, app.search_scope.as_deref(), LIMIT);
            let cards = hits
                .iter()
                .map(|h| {
                    let lib = libs.get(h.raw.lib as usize).map(|l| l.0).unwrap_or("");
                    let sub = match &h.info.year {
                        Some(y) => format!("{y}  ·  {lib}"),
                        None => lib.to_string(),
                    };
                    (
                        CardSpec {
                            id: format!("s:{}", h.raw.url),
                            folder: h.raw.url.clone(),
                            name: h.raw.name.clone(),
                            info: Some(h.info.clone()),
                            subtitle: Some(sub),
                            cover: None,
                        },
                        Route::Folder(h.raw.url.clone()),
                    )
                })
                .collect();
            (cards, total)
        };

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Results for “{query}”")).font(bold(26.0)).color(TEXT));
            ui.add_space(6.0);
            let n = if total == 1 { "1 title".to_string() } else { format!("{} titles", thousands(total)) };
            ui.label(egui::RichText::new(n).color(SUB));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if running {
                    let msg = if indexed > 0 {
                        format!("Updating index in the background…  searching {} titles", thousands(indexed))
                    } else {
                        "Indexing libraries…".to_string()
                    };
                    ui.label(egui::RichText::new(msg).color(SUB));
                    ui.spinner();
                } else {
                    if button(ui, "⟳  Refresh index", Btn::Ghost, 30.0).clicked() {
                        app.act(Action::Reindex);
                    }
                    let mut s = format!("{} titles indexed  ·  updated {}", thousands(indexed), ago(built_at));
                    if offline > 0 {
                        s.push_str(&format!("  ·  {offline} librar{} offline", if offline == 1 { "y" } else { "ies" }));
                    }
                    ui.label(egui::RichText::new(s).color(DIM));
                }
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            for (k, l) in [(SearchKind::All, "All"), (SearchKind::Movies, "Movies"), (SearchKind::Series, "Series")] {
                if chip(ui, l, app.search_kind == k).clicked() {
                    app.search_kind = k;
                }
            }
            // scope: everywhere, or just the library you were browsing
            let lib = app.search_scope.clone().or_else(|| match app.back.last() {
                Some(Route::Folder(prev)) => library_root(prev).map(String::from),
                _ => None,
            });
            if let Some(lib) = lib {
                let name = library_label(&lib).map(|(l, _)| l).unwrap_or("this library");
                ui.add_space(14.0);
                ui.label(egui::RichText::new("in").color(DIM));
                if chip(ui, "All libraries", app.search_scope.is_none()).clicked() {
                    app.search_scope = None;
                }
                if chip(ui, name, app.search_scope.as_deref() == Some(lib.as_str())).clicked() {
                    app.search_scope = Some(lib.clone());
                }
            }
        });
        ui.add_space(14.0);
        if cards.is_empty() {
            ui.add_space(30.0);
            if indexed == 0 && running {
                ui.label(egui::RichText::new("Building the index for the first time, results will appear in a few seconds…").color(SUB));
            } else {
                let what = if app.search_scope.is_some() { "No titles match in this library." } else { "No titles match." };
                ui.label(egui::RichText::new(what).font(semibold(16.0)).color(TEXT));
                ui.label(egui::RichText::new("Try fewer words or another spelling. Year, quality and tags count too, e.g. “2023 1080p”.").color(SUB));
            }
            return;
        }
        let mut cards: Vec<Option<(CardSpec, Route)>> = cards.into_iter().map(Some).collect();
        let n = cards.len();
        grid(app, ui, n, |_, i| cards[i].take().expect("each card is built once per frame"));
        if total > LIMIT {
            ui.add_space(14.0);
            ui.label(egui::RichText::new(format!("Showing the best {LIMIT} of {total}. Add a word to narrow it down.")).color(SUB));
        }
    });
}

pub fn settings(app: &mut App, ui: &mut Ui) {
    egui::Frame::new().inner_margin(page_margin()).show(ui, |ui| {
        ui.label(egui::RichText::new("Settings").font(bold(28.0)).color(TEXT));
        ui.add_space(10.0);
        let card = |ui: &mut Ui, title: &str, add: &mut dyn FnMut(&mut Ui)| {
            egui::Frame::new().fill(PANEL).corner_radius(12).stroke(Stroke::new(1.0, LINE)).inner_margin(Margin::same(18)).show(ui, |ui| {
                ui.set_width(ui.available_width().min(760.0));
                ui.label(egui::RichText::new(title).font(semibold(16.0)).color(TEXT));
                ui.add_space(6.0);
                add(ui);
            });
            ui.add_space(12.0);
        };
        let mut changed = false;
        card(ui, "Default player", &mut |ui| {
            player_switch(app, ui);
            ui.add_space(8.0);
            egui::Grid::new("paths").num_columns(4).spacing([10.0, 10.0]).show(ui, |ui| {
                for kind in [PlayerKind::Vlc, PlayerKind::PotPlayer] {
                    ui.label(kind.label());
                    let field = if kind == PlayerKind::Vlc { &mut app.cfg.vlc_path } else { &mut app.cfg.pot_path };
                    changed |= ui.add_sized([430.0, 30.0], egui::TextEdit::singleline(field)).changed();
                    if ui.button("Detect").clicked() {
                        if let Some(p) = crate::player::detect(kind) {
                            *field = p;
                            changed = true;
                        }
                    }
                    let ok = std::path::Path::new(field.as_str()).is_file();
                    ui.label(egui::RichText::new(if ok { "✔ found" } else { "✖ missing" }).color(if ok { Color32::from_rgb(120, 210, 140) } else { ACCENT_HI }));
                    ui.end_row();
                }
            });
        });
        card(ui, "Custom player (mpv, MPC-HC, …)", &mut |ui| {
            egui::Grid::new("custom").num_columns(2).spacing([10.0, 10.0]).show(ui, |ui| {
                ui.label("Path");
                changed |= ui.add_sized([430.0, 30.0], egui::TextEdit::singleline(&mut app.cfg.custom_path).hint_text(r"C:\Program Files\mpv\mpv.exe")).changed();
                ui.end_row();
                ui.label("Arguments");
                changed |= ui.add_sized([430.0, 30.0], egui::TextEdit::singleline(&mut app.cfg.custom_args).hint_text("--fs {}")).changed();
                ui.end_row();
            });
            ui.label(egui::RichText::new("{} is replaced by the stream URL (or a playlist file when several episodes are queued).").color(DIM));
        });
        card(ui, "Appearance", &mut |ui| {
            ui.horizontal(|ui| {
                ui.label("Poster size");
                changed |= ui.add(egui::Slider::new(&mut app.cfg.card_size, 130.0..=240.0).show_value(false)).changed();
            });
        });
        card(ui, "Cache", &mut |ui| {
            let size = app.cache_bytes.get_or_insert_with(covers::cache_size);
            ui.label(egui::RichText::new(format!("Poster cache: {}", if *size == 0 { "empty".to_string() } else { human_size(*size) })).color(SUB));
            ui.label(egui::RichText::new(crate::config::data_dir().display().to_string()).color(DIM));
            ui.add_space(4.0);
            if ui.button("Clear poster cache").clicked() {
                app.act(Action::ClearCache);
            }
        });
        if changed {
            app.cfg.save();
        }
    });
}

fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
