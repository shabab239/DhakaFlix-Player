//! Background loading: folder probes (to find each title's poster), poster thumbnails with a
//! disk cache, and large poster + blurred backdrop for the title page.

use crate::config::data_dir;
use crate::scrape::{self, Entry};
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use reqwest::blocking::Client;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};

pub enum Job {
    /// Fetch a folder listing (navigation, cover probe, season list).
    List(String),
    /// Poster thumbnail (grid card).
    Thumb(String),
    /// Large poster + blurred backdrop (title page / hero).
    Large(String),
}

pub enum Done {
    List(String, Result<Vec<Entry>, String>),
    Image(String, Option<ColorImage>),
}

type Queue = Arc<(Mutex<Vec<Job>>, Condvar)>;

pub struct Pool {
    queue: Queue,
    urgent: Sender<Job>,
    pub rx: Receiver<Done>,
}

impl Pool {
    pub fn new(client: Client, ctx: egui::Context, workers: usize) -> Self {
        let queue: Queue = Arc::new((Mutex::new(Vec::new()), Condvar::new()));
        let (tx, rx) = channel::<Done>();
        for _ in 0..workers {
            let (q, tx, client, ctx) = (queue.clone(), tx.clone(), client.clone(), ctx.clone());
            std::thread::spawn(move || loop {
                let job = {
                    let (lock, cv) = &*q;
                    let mut v = lock.lock().unwrap();
                    while v.is_empty() {
                        v = cv.wait(v).unwrap();
                    }
                    v.pop().unwrap() // LIFO: what was asked for last (visible now) goes first
                };
                if tx.send(run(&client, job)).is_err() {
                    return;
                }
                ctx.request_repaint();
            });
        }
        // Navigation listings bypass the queue so a page never waits behind 100 cover probes.
        let (utx, urx) = channel::<Job>();
        {
            let (tx, client, ctx) = (tx.clone(), client.clone(), ctx.clone());
            std::thread::spawn(move || {
                for job in urx {
                    let (tx, client, ctx) = (tx.clone(), client.clone(), ctx.clone());
                    std::thread::spawn(move || {
                        let _ = tx.send(run(&client, job));
                        ctx.request_repaint();
                    });
                }
            });
        }
        Pool { queue, urgent: utx, rx }
    }

    pub fn push(&self, job: Job) {
        let (lock, cv) = &*self.queue;
        lock.lock().unwrap().push(job);
        cv.notify_one();
    }

    pub fn push_urgent(&self, job: Job) {
        let _ = self.urgent.send(job);
    }
}

fn run(client: &Client, job: Job) -> Done {
    match job {
        Job::List(url) => {
            let r = scrape::fetch(client, &url);
            Done::List(url, r)
        }
        Job::Thumb(url) => {
            let img = load_scaled(client, &url, "t", 320, 480);
            Done::Image(format!("t:{url}"), img)
        }
        Job::Large(url) => {
            let img = load_scaled(client, &url, "p", 700, 1050);
            Done::Image(format!("p:{url}"), img)
        }
    }
}

pub fn thumbs_dir() -> PathBuf {
    data_dir().join("thumbs")
}

fn hash(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Download (or read from disk cache), scale to fit `w x h`, cache as JPEG.
fn load_scaled(client: &Client, url: &str, kind: &str, w: u32, h: u32) -> Option<ColorImage> {
    let dir = thumbs_dir();
    let file = dir.join(format!("{kind}_{:016x}.jpg", hash(url)));
    let img = match std::fs::read(&file).ok().and_then(|b| image::load_from_memory(&b).ok()) {
        Some(i) => i,
        None => {
            let bytes = client.get(url).send().ok()?.error_for_status().ok()?.bytes().ok()?;
            let full = image::load_from_memory(&bytes).ok()?;
            let small = if full.width() > w || full.height() > h { full.thumbnail(w, h) } else { full };
            let _ = std::fs::create_dir_all(&dir);
            let rgb = small.to_rgb8();
            if let Ok(mut f) = std::fs::File::create(&file) {
                let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, 88);
                let _ = enc.encode_image(&rgb);
            }
            small
        }
    };
    let rgba = img.to_rgba8();
    Some(ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], &rgba))
}

/// Blurred, darkened copy for backdrops (done on the UI thread from an already small image: cheap).
pub fn backdrop_from(img: &ColorImage) -> ColorImage {
    let [w, h] = img.size;
    let mut buf = image::RgbaImage::new(w as u32, h as u32);
    for (i, p) in img.pixels.iter().enumerate() {
        let [r, g, b, a] = p.to_srgba_unmultiplied();
        buf.put_pixel((i % w) as u32, (i / w) as u32, image::Rgba([r, g, b, a]));
    }
    let small = image::imageops::thumbnail(&buf, 96, (96 * h / w.max(1)) as u32);
    let blurred = image::imageops::blur(&small, 3.0);
    ColorImage::from_rgba_unmultiplied([blurred.width() as usize, blurred.height() as usize], &blurred)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cover {
    Pending,
    None,
    Some(String),
}

/// Remembers which image each title folder uses, across runs.
pub struct CoverIndex {
    map: HashMap<String, Option<String>>,
    dirty: bool,
}

impl CoverIndex {
    fn file() -> PathBuf {
        data_dir().join("covers.json")
    }

    pub fn load() -> Self {
        let map = std::fs::read_to_string(Self::file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        CoverIndex { map, dirty: false }
    }

    pub fn save_if_dirty(&mut self) {
        if self.dirty {
            let _ = std::fs::create_dir_all(data_dir());
            if let Ok(s) = serde_json::to_string(&self.map) {
                let _ = std::fs::write(Self::file(), s);
            }
            self.dirty = false;
        }
    }

    pub fn get(&self, folder: &str) -> Cover {
        match self.map.get(folder) {
            Some(Some(u)) => Cover::Some(u.clone()),
            Some(None) => Cover::None,
            None => Cover::Pending,
        }
    }

    pub fn set_from_listing(&mut self, folder: &str, entries: &[Entry]) {
        let pick = pick_cover(entries);
        if self.map.get(folder) != Some(&pick) {
            self.map.insert(folder.to_string(), pick);
            self.dirty = true;
        }
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.dirty = true;
    }
}

/// Smallest real image in the folder (posters range 30 KB .. 7 MB; we only need ~320 px).
pub fn pick_cover(entries: &[Entry]) -> Option<String> {
    entries
        .iter()
        .filter(|e| e.is_image())
        .min_by_key(|e| if e.bytes < 8 * 1024 { u64::MAX } else { e.bytes })
        .map(|e| e.url.clone())
}

/// GPU textures keyed by "t:url" / "p:url" / "b:url", with simple LRU eviction.
pub struct Textures {
    map: HashMap<String, (TextureHandle, f64)>,
    requested: HashSet<String>,
    failed: HashSet<String>,
}

impl Textures {
    pub fn new() -> Self {
        Textures { map: HashMap::new(), requested: HashSet::new(), failed: HashSet::new() }
    }

    pub fn insert(&mut self, ctx: &egui::Context, key: String, img: Option<ColorImage>) {
        self.requested.remove(&key);
        match img {
            Some(img) => {
                if let Some(rest) = key.strip_prefix("p:") {
                    let bd = backdrop_from(&img);
                    let bkey = format!("b:{rest}");
                    let t = ctx.load_texture(&bkey, bd, TextureOptions::LINEAR);
                    self.map.insert(bkey, (t, now(ctx)));
                }
                let t = ctx.load_texture(&key, img, TextureOptions::LINEAR);
                self.map.insert(key, (t, now(ctx)));
            }
            None => {
                self.failed.insert(key);
            }
        }
    }

    /// Get a texture if loaded; otherwise ask the pool for it (once).
    pub fn get(&mut self, ctx: &egui::Context, pool: &Pool, key_kind: &str, url: &str) -> Option<TextureHandle> {
        let key = format!("{key_kind}:{url}");
        if let Some((t, used)) = self.map.get_mut(&key) {
            *used = now(ctx);
            return Some(t.clone());
        }
        let fetch_key = if key_kind == "b" { format!("p:{url}") } else { key.clone() };
        if !self.failed.contains(&fetch_key) && self.requested.insert(fetch_key.clone()) {
            pool.push(if key_kind == "t" { Job::Thumb(url.to_string()) } else { Job::Large(url.to_string()) });
        }
        None
    }

    pub fn pending(&self) -> usize {
        self.requested.len()
    }

    pub fn evict(&mut self, ctx: &egui::Context, max: usize) {
        if self.map.len() <= max {
            return;
        }
        let t = now(ctx);
        let mut v: Vec<(String, f64)> = self.map.iter().map(|(k, (_, u))| (k.clone(), *u)).collect();
        v.sort_by(|a, b| a.1.total_cmp(&b.1));
        let excess = self.map.len() - max;
        for (k, used) in v.into_iter().take(excess) {
            if t - used > 2.0 {
                self.map.remove(&k);
            }
        }
    }
}

fn now(ctx: &egui::Context) -> f64 {
    ctx.input(|i| i.time)
}

/// Disk usage of the thumbnail cache in bytes.
pub fn cache_size() -> u64 {
    std::fs::read_dir(thumbs_dir())
        .map(|rd| rd.filter_map(|e| e.ok()?.metadata().ok()).map(|m| m.len()).sum())
        .unwrap_or(0)
}
