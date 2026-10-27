#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod covers;
mod index;
mod meta;
mod player;
mod scrape;
mod shots;
mod theme;
mod views;

use config::{Config, Link, PlayerKind};
use covers::{Cover, CoverIndex, Done, Job, Pool, Textures};
use eframe::egui::{self, Align2, Color32, CornerRadius, CursorIcon, Id, Margin, Pos2, Rect, Sense, Vec2};
use scrape::{natural_cmp, Entry};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use theme::*;
use views::FolderView;

pub const LIBRARY: &[(&str, &[(&str, &str)])] = &[
    (
        "Movies",
        &[
            ("English", "http://172.16.50.7/DHAKA-FLIX-7/English%20Movies/"),
            ("English 1080p", "http://172.16.50.14/DHAKA-FLIX-14/English%20Movies%20%281080p%29/"),
            ("IMDb Top 250", "http://172.16.50.14/DHAKA-FLIX-14/IMDb%20Top-250%20Movies/"),
            ("Hindi", "http://172.16.50.14/DHAKA-FLIX-14/Hindi%20Movies/"),
            ("South Hindi Dubbed", "http://172.16.50.14/DHAKA-FLIX-14/SOUTH%20INDIAN%20MOVIES/Hindi%20Dubbed/"),
            ("South Indian", "http://172.16.50.14/DHAKA-FLIX-14/SOUTH%20INDIAN%20MOVIES/South%20Movies/"),
            ("Kolkata Bangla", "http://172.16.50.7/DHAKA-FLIX-7/Kolkata%20Bangla%20Movies/"),
            ("Animation", "http://172.16.50.14/DHAKA-FLIX-14/Animation%20Movies/"),
            ("Animation 1080p", "http://172.16.50.14/DHAKA-FLIX-14/Animation%20Movies%20%281080p%29/"),
            ("Foreign Language", "http://172.16.50.7/DHAKA-FLIX-7/Foreign%20Language%20Movies/"),
            ("3D Movies", "http://172.16.50.7/DHAKA-FLIX-7/3D%20Movies/"),
        ],
    ),
    (
        "TV & More",
        &[
            ("TV & Web Series", "http://172.16.50.12/DHAKA-FLIX-12/TV-WEB-Series/"),
            ("Korean Series", "http://172.16.50.14/DHAKA-FLIX-14/KOREAN%20TV%20%26%20WEB%20Series/"),
            ("Anime & Cartoon", "http://172.16.50.10/DHAKA-FLIX-10/Anime%20%26%20Cartoon%20TV%20Series/"),
            ("Awards & TV Shows", "http://172.16.50.10/DHAKA-FLIX-10/Awards%20%26%20TV%20Shows/"),
            ("Documentary", "http://172.16.50.10/DHAKA-FLIX-10/Documentary/"),
            ("WWE & AEW", "http://172.16.50.10/DHAKA-FLIX-10/WWE%20%26%20AEW%20Wrestling/"),
            ("Tutorials", "http://172.16.50.10/DHAKA-FLIX-10/Tutorial/"),
        ],
    ),
];

#[derive(Clone, PartialEq, Debug)]
pub enum Route {
    Home,
    Folder(String),
    Settings,
    Search(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Name,
    Year,
    Added,
}

pub enum Action {
    Nav(Route),
    Back,
    Forward,
    Play(Vec<Link>, Option<PlayerKind>),
    ToggleFav(Link),
    Copy(String),
    Reload(String),
    ClearCache,
    Reindex,
}

struct Toast {
    text: String,
    err: bool,
    t0: f64,
}

pub struct App {
    pub cfg: Config,
    ctx: egui::Context,
    pool: Pool,
    tex: Textures,
    covers: CoverIndex,
    pub listings: HashMap<String, Arc<Vec<Entry>>>,
    pub list_err: HashMap<String, String>,
    views: HashMap<String, Arc<FolderView>>,
    inflight: HashSet<String>,
    route: Route,
    pub back: Vec<Route>,
    fwd: Vec<Route>,
    pub filter: String,
    pub sort: SortMode,
    pub shelf_sel: HashMap<String, String>,
    pub season_sel: HashMap<String, String>,
    pub cache_bytes: Option<u64>,
    actions: Vec<Action>,
    toast: Option<Toast>,
    last_save: f64,
    shots: Option<shots::Shots>,
    force_scroll: Option<f32>,
    pub index: index::Index,
    pub search_kind: index::SearchKind,
    /// Limit search to one library / folder (URL prefix).
    pub search_scope: Option<String>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let mut cfg = Config::load();
        player::fill_defaults(&mut cfg);
        if cfg.card_size < 100.0 {
            cfg.card_size = 168.0;
        }
        cfg.save();
        let client = scrape::make_client();
        let mut app = App {
            ctx: cc.egui_ctx.clone(),
            pool: Pool::new(client, cc.egui_ctx.clone(), 8),
            tex: Textures::new(),
            covers: CoverIndex::load(),
            listings: HashMap::new(),
            list_err: HashMap::new(),
            views: HashMap::new(),
            inflight: HashSet::new(),
            route: Route::Home,
            back: vec![],
            fwd: vec![],
            filter: String::new(),
            sort: SortMode::Name,
            shelf_sel: HashMap::new(),
            season_sel: HashMap::new(),
            cache_bytes: None,
            actions: vec![],
            toast: None,
            last_save: 0.0,
            shots: shots::Shots::from_env(),
            force_scroll: None,
            index: index::Index::load(),
            search_kind: index::SearchKind::All,
            search_scope: None,
            cfg,
        };
        // Warm up every library root so tiles can show counts / offline state.
        for (_, items) in LIBRARY {
            for (_, u) in *items {
                app.request(u, true);
            }
        }
        if let Some(u) = app.cfg.last_url.clone() {
            app.route = Route::Folder(u);
        }
        app
    }

    // ---------------------------------------------------------------- data access used by views

    pub fn act(&mut self, a: Action) {
        self.actions.push(a);
    }

    fn request(&mut self, url: &str, urgent: bool) {
        if url.is_empty() || self.listings.contains_key(url) || self.list_err.contains_key(url) || !self.inflight.insert(url.to_string()) {
            return;
        }
        if urgent {
            self.pool.push_urgent(Job::List(url.to_string()));
        } else {
            self.pool.push(Job::List(url.to_string()));
        }
    }

    /// Classified listing for `url`; requests it (with priority) when missing.
    pub fn view(&mut self, url: &str) -> Option<Arc<FolderView>> {
        if let Some(v) = self.views.get(url) {
            return Some(v.clone());
        }
        if let Some(l) = self.listings.get(url) {
            let v = Arc::new(FolderView::build(l));
            self.views.insert(url.to_string(), v.clone());
            return Some(v);
        }
        self.request(url, true);
        None
    }

    /// Poster image URL for a title folder; probes the folder in the background when unknown.
    pub fn cover_for(&mut self, folder: &str) -> Option<String> {
        if folder.is_empty() {
            return None;
        }
        match self.covers.get(folder) {
            Cover::Some(u) => Some(u),
            Cover::None => None,
            Cover::Pending => {
                if let Some(l) = self.listings.get(folder).cloned() {
                    self.covers.set_from_listing(folder, &l);
                    return covers::pick_cover(&l);
                }
                self.request(folder, false);
                None
            }
        }
    }

    pub fn tex_get(&mut self, kind: &str, url: &str) -> Option<egui::TextureHandle> {
        self.tex.get(&self.ctx, &self.pool, kind, url)
    }

    /// Anything still loading? (listings or images)
    pub fn busy(&self) -> bool {
        !self.inflight.is_empty() || self.tex.pending() > 0 || self.index.running()
    }

    pub fn is_fav(&self, url: &str) -> bool {
        self.cfg.favorites.iter().any(|f| f.url == url)
    }

    // ---------------------------------------------------------------- frame plumbing

    fn poll(&mut self) {
        while let Ok(d) = self.pool.rx.try_recv() {
            match d {
                Done::List(url, Ok(mut v)) => {
                    v.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| natural_cmp(&a.name, &b.name)));
                    self.covers.set_from_listing(&url, &v);
                    self.inflight.remove(&url);
                    self.list_err.remove(&url);
                    self.views.remove(&url);
                    self.listings.insert(url, Arc::new(v));
                }
                Done::List(url, Err(e)) => {
                    self.inflight.remove(&url);
                    self.list_err.insert(url, e);
                }
                Done::Image(key, img) => self.tex.insert(&self.ctx, key, img),
            }
        }
    }

    fn toast(&mut self, text: impl Into<String>, err: bool) {
        let t0 = self.ctx.input(|i| i.time);
        self.toast = Some(Toast { text: text.into(), err, t0 });
    }

    fn go(&mut self, r: Route) {
        if r != self.route {
            let old = std::mem::replace(&mut self.route, r);
            self.back.push(old);
            self.fwd.clear();
            self.after_nav();
        }
    }

    fn after_nav(&mut self) {
        self.filter = match &self.route {
            Route::Search(q) => q.clone(),
            _ => String::new(),
        };
        self.cfg.last_url = match &self.route {
            Route::Folder(u) => Some(u.clone()),
            _ => None,
        };
        self.cfg.save();
    }

    fn apply(&mut self) {
        for a in std::mem::take(&mut self.actions) {
            match a {
                Action::Nav(r) => self.go(r),
                Action::Back => {
                    if let Some(r) = self.back.pop() {
                        let cur = std::mem::replace(&mut self.route, r);
                        self.fwd.push(cur);
                        self.after_nav();
                    }
                }
                Action::Forward => {
                    if let Some(r) = self.fwd.pop() {
                        let cur = std::mem::replace(&mut self.route, r);
                        self.back.push(cur);
                        self.after_nav();
                    }
                }
                Action::Play(items, kind) => {
                    let kind = kind.unwrap_or(self.cfg.player);
                    match player::launch(&self.cfg, kind, &items) {
                        Ok(()) => {
                            let what = items.first().map(|l| l.title.clone().unwrap_or_else(|| meta::strip_ext(&l.name).to_string())).unwrap_or_default();
                            let extra = if items.len() > 1 { format!("  (+{} queued)", items.len() - 1) } else { String::new() };
                            self.toast(format!("Opening in {}: {what}{extra}", kind.label()), false);
                            if let Some(first) = items.into_iter().next() {
                                self.cfg.push_history(first);
                            }
                            self.cfg.save();
                        }
                        Err(e) => self.toast(e, true),
                    }
                }
                Action::ToggleFav(l) => {
                    if let Some(i) = self.cfg.favorites.iter().position(|f| f.url == l.url) {
                        self.cfg.favorites.remove(i);
                        self.toast("Removed from favorites", false);
                    } else {
                        let mut l = l;
                        if l.cover.is_none() {
                            l.cover = self.cover_for(&l.url);
                        }
                        self.cfg.favorites.insert(0, l);
                        self.toast("Saved to favorites", false);
                    }
                    self.cfg.save();
                }
                Action::Copy(u) => {
                    self.ctx.copy_text(u);
                    self.toast("Link copied", false);
                }
                Action::Reload(u) => {
                    self.listings.remove(&u);
                    self.list_err.remove(&u);
                    self.views.remove(&u);
                    self.request(&u, true);
                }
                Action::Reindex => self.index.rebuild(self.ctx.clone()),
                Action::ClearCache => {
                    let _ = std::fs::remove_dir_all(covers::thumbs_dir());
                    self.covers.clear();
                    self.cache_bytes = None;
                    self.toast("Poster cache cleared", false);
                }
            }
        }
    }

    // ---------------------------------------------------------------- chrome

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Dhaka").font(bold(24.0)).color(TEXT));
            ui.label(egui::RichText::new("Flix").font(bold(24.0)).color(ACCENT));
        });
        ui.add_space(14.0);
        let active_lib = match &self.route {
            Route::Folder(u) => LIBRARY
                .iter()
                .flat_map(|(_, i)| i.iter())
                .filter(|(_, l)| u.starts_with(*l))
                .max_by_key(|(_, l)| l.len())
                .map(|(_, l)| *l),
            _ => None,
        };
        if nav_item(ui, "⌂", "Home", self.route == Route::Home).clicked() {
            self.act(Action::Nav(Route::Home));
        }
        if nav_item(ui, "⚙", "Settings", self.route == Route::Settings).clicked() {
            self.act(Action::Nav(Route::Settings));
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (group, items) in LIBRARY {
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new(group.to_uppercase()).font(semibold(11.0)).color(DIM));
                });
                ui.add_space(2.0);
                for (label, url) in *items {
                    let offline = self.list_err.contains_key(*url);
                    let icon = if group.starts_with("TV") { "📺" } else { "🎬" };
                    let r = nav_item(ui, icon, label, active_lib == Some(*url));
                    if offline {
                        ui.painter().circle_filled(Pos2::new(r.rect.right() - 12.0, r.rect.center().y), 3.0, Color32::from_rgb(200, 80, 70));
                    }
                    if r.clicked() {
                        self.act(Action::Nav(Route::Folder((*url).to_string())));
                    }
                }
            }
            ui.add_space(10.0);
        });
    }

    fn topbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if views::icon_button(ui, "←", !self.back.is_empty(), "Back (Alt+←)").clicked() {
                self.act(Action::Back);
            }
            if views::icon_button(ui, "→", !self.fwd.is_empty(), "Forward").clicked() {
                self.act(Action::Forward);
            }
            if let Route::Folder(u) = &self.route {
                let u = u.clone();
                if views::icon_button(ui, "⟳", true, "Reload").clicked() {
                    self.act(Action::Reload(u.clone()));
                }
                ui.add_space(10.0);
                let crumbs = views::crumbs(&u);
                let n = crumbs.len();
                for (i, (label, route)) in crumbs.into_iter().enumerate() {
                    if i + 1 == n {
                        ui.label(egui::RichText::new(label).font(semibold(14.0)).color(TEXT));
                    } else {
                        let r = ui.add(egui::Label::new(egui::RichText::new(label).color(SUB)).sense(Sense::click()));
                        if r.hovered() {
                            ui.painter().line_segment(
                                [r.rect.left_bottom(), r.rect.right_bottom()],
                                egui::Stroke::new(1.0, SUB),
                            );
                        }
                        if r.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                            self.act(Action::Nav(route));
                        }
                        ui.label(egui::RichText::new("›").color(DIM));
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let search_id = Id::unique("search");
                if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
                    ui.memory_mut(|m| m.request_focus(search_id));
                }
                let mut changed = false;
                egui::Frame::new().fill(CARD).corner_radius(17).inner_margin(Margin::symmetric(12, 5)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if !self.filter.is_empty() && ui.add(egui::Label::new(egui::RichText::new("✕").color(SUB)).sense(Sense::click())).clicked() {
                            self.filter.clear();
                            changed = true;
                        }
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut self.filter)
                                .id(search_id)
                                .hint_text("Search all movies & series  (Ctrl+F)")
                                .frame(egui::Frame::NONE)
                                .desired_width(260.0),
                        );
                        changed |= r.changed();
                        ui.label(egui::RichText::new("🔍").color(DIM));
                    });
                });
                if changed {
                    self.on_search_edit();
                }
            });
        });
    }

    fn on_search_edit(&mut self) {
        let q = self.filter.trim().to_string();
        match (&mut self.route, q.is_empty()) {
            (Route::Search(_), true) => {
                self.route = self.back.pop().unwrap_or(Route::Home);
            }
            (Route::Search(cur), false) => *cur = q, // typing updates in place, no history spam
            (_, false) => {
                // a new search always starts across every library
                self.search_scope = None;
                let keep = self.filter.clone();
                self.go(Route::Search(q));
                self.filter = keep;
            }
            (_, true) => {}
        }
    }

    fn draw_toast(&mut self, ctx: &egui::Context) {
        let Some(t) = &self.toast else { return };
        let age = ctx.input(|i| i.time) - t.t0;
        if age > 3.5 {
            self.toast = None;
            return;
        }
        let alpha = ((3.5 - age) / 0.4).clamp(0.0, 1.0) as f32;
        let (text, err) = (t.text.clone(), t.err);
        egui::Area::new(Id::unique("toast")).anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -28.0)).order(egui::Order::Foreground).show(ctx, |ui| {
            egui::Frame::new()
                .fill(if err { Color32::from_rgb(120, 36, 36) } else { Color32::from_rgb(34, 38, 48) }.linear_multiply(alpha))
                .stroke(egui::Stroke::new(1.0, (if err { ACCENT } else { LINE }).linear_multiply(alpha)))
                .corner_radius(22)
                .shadow(egui::Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha((120.0 * alpha) as u8) })
                .inner_margin(Margin::symmetric(18, 10))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(text).color(TEXT.linear_multiply(alpha)));
                });
        });
        ctx.request_repaint();
    }
}

fn nav_item(ui: &mut egui::Ui, icon: &str, label: &str, active: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    let p = ui.painter();
    if active {
        p.rect_filled(rect, CornerRadius::same(8), ACCENT.linear_multiply(0.16));
        p.rect_filled(Rect::from_min_size(rect.left_top() + Vec2::new(0.0, 8.0), Vec2::new(3.0, 18.0)), CornerRadius::same(2), ACCENT);
    } else if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(8), CARD);
    }
    let col = if active { TEXT } else if resp.hovered() { TEXT } else { SUB };
    p.text(rect.left_center() + Vec2::new(22.0, 0.0), Align2::CENTER_CENTER, icon, regular(14.0), if active { ACCENT_HI } else { DIM });
    p.text(rect.left_center() + Vec2::new(40.0, 0.0), Align2::LEFT_CENTER, label, if active { semibold(14.0) } else { regular(14.0) }, col);
    resp
}

impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        self.poll();
        self.index.refresh_hits();
        if self.index.running() {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
        shots::Shots::tick(self, &ctx);

        ctx.input(|i| {
            if i.pointer.button_pressed(egui::PointerButton::Extra1) || (i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft)) {
                self.actions.push(Action::Back);
            }
            if i.pointer.button_pressed(egui::PointerButton::Extra2) || (i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight)) {
                self.actions.push(Action::Forward);
            }
        });

        egui::Panel::left("side")
            .exact_size(220.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(Margin { left: 12, right: 12, top: 18, bottom: 8 }))
            .show(root, |ui| self.sidebar(ui));

        egui::Panel::top("top")
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin { left: 18, right: 20, top: 10, bottom: 8 }))
            .show(root, |ui| self.topbar(ui));

        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(root, |ui| {
            let key = match &self.route {
                Route::Home => "home".to_string(),
                Route::Settings => "settings".to_string(),
                Route::Search(_) => "search".to_string(),
                Route::Folder(u) => u.clone(),
            };
            let route = self.route.clone();
            let mut sa = egui::ScrollArea::vertical().id_salt(key).auto_shrink([false, false]);
            if let Some(y) = self.force_scroll.take() {
                sa = sa.vertical_scroll_offset(y);
            }
            sa.show(ui, |ui| match &route {
                Route::Home => views::home(self, ui),
                Route::Settings => views::settings(self, ui),
                Route::Search(q) => views::search(self, ui, q),
                Route::Folder(u) => views::folder(self, ui, u),
            });
        });

        self.draw_toast(&ctx);
        self.apply();
        self.tex.evict(&ctx, 360);
        let now = ctx.input(|i| i.time);
        if now - self.last_save > 3.0 {
            self.covers.save_if_dirty();
            self.last_save = now;
        }
    }
}

fn main() -> eframe::Result<()> {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png")).ok();
    let mut viewport = egui::ViewportBuilder::default();
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let opts = eframe::NativeOptions {
        viewport: viewport
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("DhakaFlix"),
        ..Default::default()
    };
    eframe::run_native("DhakaFlix", opts, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
