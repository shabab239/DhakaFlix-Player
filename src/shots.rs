//! `DHAKAFLIX_SHOTS=<dir>`: walk a fixed set of pages, wait for listings and posters to settle,
//! save one PNG per page, then quit. Used to check the layout without clicking around.

use crate::{App, Route};
use eframe::egui;
use std::path::PathBuf;

pub struct Shots {
    dir: PathBuf,
    steps: Vec<(&'static str, Route)>,
    idx: usize,
    step_t0: f64,
    quiet_since: Option<f64>,
    shot_at: Option<f64>,
}

impl Shots {
    pub fn from_env() -> Option<Self> {
        let dir = PathBuf::from(std::env::var("DHAKAFLIX_SHOTS").ok()?);
        let _ = std::fs::create_dir_all(&dir);
        let steps = vec![
            ("01_home", Route::Home),
            ("01b_home_tiles", Route::Home),
            ("02_shelf_english1080", Route::Folder("http://172.16.50.14/DHAKA-FLIX-14/English%20Movies%20%281080p%29/".into())),
            ("03_imdb", Route::Folder("http://172.16.50.14/DHAKA-FLIX-14/IMDb%20Top-250%20Movies/".into())),
            ("04_tv", Route::Folder("http://172.16.50.12/DHAKA-FLIX-12/TV-WEB-Series/".into())),
            ("05_movie", Route::Folder("http://172.16.50.14/DHAKA-FLIX-14/Hindi%20Movies/%282023%29/12th%20Fail%20%282023%29%201080p/".into())),
            ("06_series", Route::Folder("http://172.16.50.12/DHAKA-FLIX-12/TV-WEB-Series/TV%20Series%20%E2%99%A5%20%20A%20%20%E2%80%94%20%20L/Alexa%20%26%20Katie%20%28TV%20Series%202018%E2%80%932020%29%20720p%20%5BDual%20Audio%5D/".into())),
            ("07_settings", Route::Settings),
            ("08_search", Route::Search("knight".into())),
        ];
        Some(Shots { dir, steps, idx: 0, step_t0: -1.0, quiet_since: None, shot_at: None })
    }

    /// Call once per frame before drawing.
    pub fn tick(app: &mut App, ctx: &egui::Context) {
        let Some(mut s) = app.shots.take() else { return };
        let now = ctx.input(|i| i.time);
        ctx.request_repaint();
        // give the screenshot request a few frames to land before changing page
        if s.shot_at.is_some_and(|t| now - t < 0.8) {
            app.shots = Some(s);
            return;
        }
        if s.idx >= s.steps.len() {
            if s.shot_at.is_some_and(|t| now - t > 1.5) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            app.shots = Some(s);
            return;
        }
        if s.step_t0 < 0.0 {
            app.route = s.steps[s.idx].1.clone();
            if let Route::Search(q) = &app.route {
                app.filter = q.clone();
            }
            if s.steps[s.idx].0.ends_with("_tiles") {
                app.force_scroll = Some(560.0);
            }
            s.step_t0 = now;
            s.quiet_since = None;
        }
        let busy = app.busy();
        if busy {
            s.quiet_since = None;
        } else if s.quiet_since.is_none() {
            s.quiet_since = Some(now);
        }
        let settled = s.quiet_since.is_some_and(|q| now - q > 1.2) && now - s.step_t0 > 1.5;
        if settled || now - s.step_t0 > 40.0 {
            let path = s.dir.join(format!("{}.png", s.steps[s.idx].0));
            ctx.request_screenshot(move |img| {
                let [w, h] = img.size;
                let mut buf = image::RgbaImage::new(w as u32, h as u32);
                for (i, p) in img.pixels.iter().enumerate() {
                    buf.put_pixel((i % w) as u32, (i / w) as u32, image::Rgba(p.to_array()));
                }
                let _ = buf.save(&path);
            });
            s.shot_at = Some(now);
            s.idx += 1;
            s.step_t0 = -1.0;
        }
        app.shots = Some(s);
    }
}
