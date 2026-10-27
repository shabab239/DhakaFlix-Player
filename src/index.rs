//! Library-wide title index: crawls every library's grouping folders (years, A-L ranges,
//! languages) in the background, keeps the result on disk, and answers searches instantly.

use crate::config::data_dir;
use crate::meta::{self, TitleInfo};
use crate::scrape;
use crate::LIBRARY;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_DEPTH: usize = 4;
const WORKERS: usize = 8;
pub const STALE_SECS: u64 = 24 * 3600;

#[derive(Serialize, Deserialize, Clone)]
pub struct Raw {
    pub name: String,
    pub url: String,
    /// Index into the flattened LIBRARY list.
    pub lib: u16,
    pub modified: String,
}

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    built_at: u64,
    items: Vec<Raw>,
}

#[derive(Default)]
pub struct Shared {
    pub items: Vec<Raw>,
    pub running: bool,
    pub offline: usize,
    pub built_at: u64,
    pub generation: u64,
}

pub struct Hit {
    pub raw: Raw,
    pub info: TitleInfo,
    hay: String,
}

pub struct Index {
    pub shared: Arc<Mutex<Shared>>,
    seen_gen: u64,
    pub hits: Vec<Hit>,
    /// Running on the snapshot shipped inside the exe (no crawl has finished on this PC yet).
    pub bundled: bool,
    refresh_checked: bool,
}

/// Snapshot of every library, taken when the app was built, so search works on first launch.
/// Regenerate: copy %LOCALAPPDATA%\dhakaflix-player\index.json through gzip into assets/.
static BUNDLED: &[u8] = include_bytes!("../assets/default_index.json.gz");

fn bundled() -> Saved {
    use std::io::Read;
    let mut s = String::new();
    let mut gz = flate2::read::GzDecoder::new(BUNDLED);
    if gz.read_to_string(&mut s).is_err() {
        return Saved::default();
    }
    serde_json::from_str(&s).unwrap_or_default()
}

pub fn libraries() -> Vec<(&'static str, &'static str)> {
    LIBRARY.iter().flat_map(|(_, items)| items.iter().copied()).collect()
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn file() -> std::path::PathBuf {
    data_dir().join("index.json")
}

impl Index {
    pub fn load() -> Self {
        let disk: Option<Saved> = std::fs::read_to_string(file())
            .ok()
            .and_then(|s| serde_json::from_str::<Saved>(&s).ok())
            .filter(|s| !s.items.is_empty());
        let is_bundled = disk.is_none();
        let saved = disk.unwrap_or_else(bundled);
        let shared = Shared { items: saved.items, built_at: saved.built_at, generation: 1, ..Default::default() };
        let mut ix = Index { shared: Arc::new(Mutex::new(shared)), seen_gen: 0, hits: vec![], bundled: is_bundled, refresh_checked: false };
        ix.refresh_hits();
        ix
    }

    pub fn is_stale(&self) -> bool {
        let s = self.shared.lock().unwrap();
        s.items.is_empty() || now().saturating_sub(s.built_at) > STALE_SECS
    }

    pub fn running(&self) -> bool {
        self.shared.lock().unwrap().running
    }

    /// Pick up new crawl results (cheap when nothing changed).
    pub fn refresh_hits(&mut self) {
        let s = self.shared.lock().unwrap();
        if s.generation == self.seen_gen {
            return;
        }
        self.seen_gen = s.generation;
        let libs = libraries();
        self.hits = s
            .items
            .iter()
            .filter_map(|r| {
                let info = meta::parse_title(&r.name)?;
                let lib = libs.get(r.lib as usize).map(|l| l.0).unwrap_or("");
                let hay = fold(&format!(
                    "{} {} {} {} {}",
                    info.title,
                    info.year.as_deref().unwrap_or(""),
                    info.quality.as_deref().unwrap_or(""),
                    info.tags.join(" "),
                    lib
                ));
                Some(Hit { raw: r.clone(), info, hay })
            })
            .collect();
    }

    /// Called when a search is made: once per session, refresh in the background if the index
    /// is older than a day (or still the bundled snapshot). Results keep coming from the old index.
    pub fn refresh_if_stale(&mut self, ctx: &eframe::egui::Context) {
        if self.refresh_checked {
            return;
        }
        self.refresh_checked = true;
        if self.bundled || self.is_stale() {
            self.rebuild(ctx.clone());
        }
    }

    /// Start a background crawl unless one is running.
    pub fn rebuild(&self, ctx: eframe::egui::Context) {
        {
            let mut s = self.shared.lock().unwrap();
            if s.running {
                return;
            }
            s.running = true;
        }
        let shared = self.shared.clone();
        std::thread::spawn(move || {
            let (found, offline_libs) = crawl(&shared, &ctx);
            let mut s = shared.lock().unwrap();
            // keep what we knew about libraries whose server is down right now
            let mut items: Vec<Raw> = s.items.iter().filter(|r| offline_libs.contains(&r.lib)).cloned().collect();
            items.extend(found);
            s.items = items;
            s.running = false;
            s.built_at = now();
            s.generation += 1;
            let saved = Saved { built_at: s.built_at, items: s.items.clone() };
            drop(s);
            let _ = std::fs::create_dir_all(data_dir());
            if let Ok(j) = serde_json::to_string(&saved) {
                let _ = std::fs::write(file(), j);
            }
            ctx.request_repaint();
        });
    }

    /// All query words must appear (case/diacritic-insensitive) in title, year, quality, tags or library.
    /// Ranked: title starts with query > word-start match > anywhere; then newest year.
    pub fn search(&self, query: &str, kind: SearchKind, scope: Option<&str>, limit: usize) -> (Vec<&Hit>, usize) {
        let q = fold(query);
        let words: Vec<&str> = q.split_whitespace().collect();
        if words.is_empty() {
            return (vec![], 0);
        }
        let mut seen = HashSet::new();
        let mut v: Vec<(u8, &Hit)> = self
            .hits
            .iter()
            .filter(|h| match kind {
                SearchKind::All => true,
                SearchKind::Movies => !h.info.series,
                SearchKind::Series => h.info.series,
            })
            .filter(|h| scope.is_none_or(|s| h.raw.url.starts_with(s)))
            .filter(|h| words.iter().all(|w| h.hay.contains(w)))
            .filter(|h| seen.insert(h.raw.url.as_str()))
            .map(|h| {
                let t = fold(&h.info.title);
                let bare = ["the ", "a ", "an "].iter().find_map(|a| t.strip_prefix(a)).unwrap_or(&t);
                let score = if t.starts_with(&q) || bare.starts_with(&q) {
                    0
                } else if t.split_whitespace().any(|tw| tw.starts_with(words[0])) {
                    1
                } else {
                    2
                };
                (score, h)
            })
            .collect();
        let total = v.len();
        v.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.info.year.cmp(&a.1.info.year)));
        (v.into_iter().take(limit).map(|(_, h)| h).collect(), total)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    All,
    Movies,
    Series,
}

/// Lowercase and drop punctuation so "spider-man" finds "Spider Man" and "Spiderman: X".
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_alphanumeric() {
            out.push(c);
        } else if c == '\'' || c == '’' {
            // "girl's" -> "girls"
        } else if !out.ends_with(' ') {
            out.push(' ');
        }
    }
    out.trim().to_string()
}

/// Breadth-first: list library roots, record title folders, descend into grouping folders.
/// Returns the titles found and the libraries that could not be reached.
fn crawl(shared: &Arc<Mutex<Shared>>, ctx: &eframe::egui::Context) -> (Vec<Raw>, HashSet<u16>) {
    let had_items = !shared.lock().unwrap().items.is_empty();
    let mut offline_libs: HashSet<u16> = HashSet::new();
    let client = scrape::make_client();
    let mut frontier: Vec<(String, u16)> = libraries().iter().enumerate().map(|(i, (_, u))| (u.to_string(), i as u16)).collect();
    let mut found: Vec<Raw> = vec![];
    let mut visited: HashSet<String> = HashSet::new();
    let mut offline = 0;
    for depth in 0..MAX_DEPTH {
        frontier.retain(|(u, _)| visited.insert(u.clone()));
        if frontier.is_empty() {
            break;
        }
        let queue = Mutex::new(frontier.clone());
        let results: Mutex<Vec<(u16, Result<Vec<scrape::Entry>, String>)>> = Mutex::new(vec![]);
        std::thread::scope(|sc| {
            for _ in 0..WORKERS {
                sc.spawn(|| loop {
                    let Some((url, lib)) = queue.lock().unwrap().pop() else { return };
                    let r = scrape::fetch(&client, &url);
                    results.lock().unwrap().push((lib, r));
                });
            }
        });
        let mut next = vec![];
        for (lib, r) in results.into_inner().unwrap() {
            match r {
                Ok(entries) => {
                    for e in entries.into_iter().filter(|e| e.is_dir) {
                        if meta::parse_title(&e.name).is_some() {
                            found.push(Raw { name: e.name, url: e.url, lib, modified: e.modified });
                        } else if !meta::is_season(&e.name) {
                            next.push((e.url, lib));
                        }
                    }
                }
                Err(_) if depth == 0 => {
                    offline += 1;
                    offline_libs.insert(lib);
                }
                Err(_) => {}
            }
        }
        frontier = next;
        let mut s = shared.lock().unwrap();
        if depth == 0 {
            s.offline = offline;
        }
        // first-ever crawl: publish partial results so search works while it continues
        // (a refresh keeps serving the old index until the new one is complete)
        if !had_items && found.len() > s.items.len() {
            s.items = found.clone();
            s.generation += 1;
        }
        drop(s);
        ctx.request_repaint();
    }
    (found, offline_libs)
}

pub fn ago(secs: u64) -> String {
    let d = now().saturating_sub(secs);
    match d {
        0..=89 => "just now".into(),
        90..=3599 => format!("{} min ago", d / 60),
        3600..=86_399 => format!("{} h ago", d / 3600),
        _ => format!("{} days ago", d / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_snapshot_loads() {
        let s = bundled();
        assert!(s.items.len() > 10_000, "bundled index has {} items", s.items.len());
        assert!(s.items.iter().all(|r| (r.lib as usize) < libraries().len()));
    }

    #[test]
    fn folding() {
        assert_eq!(fold("Spider-Man: No Way Home"), "spider man no way home");
        assert_eq!(fold("A Good Girl's Guide"), "a good girls guide");
    }

    #[test]
    fn search_ranks_and_filters() {
        let raw = |n: &str| Raw { name: n.into(), url: format!("http://h/{n}/"), lib: 0, modified: String::new() };
        let shared = Shared {
            items: vec![
                raw("The Dark Knight (2008) 1080p [Dual Audio]"),
                raw("Knight and Day (2010) 720p"),
                raw("A Knight of the Seven Kingdoms (TV Series 2026– ) 1080p [Dual Audio]"),
                raw("(2023)"),
            ],
            generation: 1,
            ..Default::default()
        };
        let mut ix = Index { shared: Arc::new(Mutex::new(shared)), seen_gen: 0, hits: vec![], bundled: false, refresh_checked: false };
        ix.refresh_hits();
        assert_eq!(ix.hits.len(), 3, "grouping folders are not titles");
        let (r, n) = ix.search("knight", SearchKind::All, None, 10);
        assert_eq!(n, 3);
        assert_eq!(r[0].info.title, "A Knight of the Seven Kingdoms", "article-stripped prefix, newest first");
        assert_eq!(r[1].info.title, "Knight and Day");
        assert_eq!(r[2].info.title, "The Dark Knight");
        let (r, _) = ix.search("knight", SearchKind::Series, None, 10);
        assert_eq!(r.len(), 1);
        let (r, _) = ix.search("dark 2008 dual", SearchKind::All, None, 10);
        assert_eq!(r.len(), 1);
        assert_eq!(ix.search("knight", SearchKind::All, Some("http://h/Knight"), 10).1, 1);
        assert_eq!(ix.search("   ", SearchKind::All, None, 10).1, 0);
    }
}
