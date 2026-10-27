//! Reads h5ai directory listings (the "fallback" HTML table) served by the DhakaFlix nginx boxes.

use reqwest::{blocking::Client, Url};
use scraper::{Html, Selector};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub url: String,
    pub is_dir: bool,
    pub modified: String,
    pub bytes: u64,
}

const VIDEO_EXT: &[&str] = &[
    "mkv", "mp4", "avi", "mov", "wmv", "m4v", "ts", "webm", "flv", "mpg", "mpeg", "m2ts", "vob", "divx",
];
const IMAGE_EXT: &[&str] = &["jpg", "jpeg", "png", "webp"];

impl Entry {
    fn ext(&self) -> String {
        self.name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
    }
    pub fn is_video(&self) -> bool {
        !self.is_dir && VIDEO_EXT.contains(&self.ext().as_str())
    }
    pub fn is_image(&self) -> bool {
        !self.is_dir && IMAGE_EXT.contains(&self.ext().as_str())
    }
}

pub fn make_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("dhakaflix-player/0.1")
        .build()
        .expect("http client")
}

pub fn fetch(client: &Client, url: &str) -> Result<Vec<Entry>, String> {
    let base = Url::parse(url).map_err(|e| format!("bad URL: {e}"))?;
    let body = client
        .get(base.clone())
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .text()
        .map_err(|e| e.to_string())?;
    parse(&body, &base)
}

pub fn parse(html: &str, base: &Url) -> Result<Vec<Entry>, String> {
    let doc = Html::parse_document(html);
    let row = Selector::parse("tr").unwrap();
    let link = Selector::parse("td.fb-n a").unwrap();
    let date = Selector::parse("td.fb-d").unwrap();
    let size = Selector::parse("td.fb-s").unwrap();

    let mut out = Vec::new();
    for tr in doc.select(&row) {
        let Some(a) = tr.select(&link).next() else { continue };
        let Some(href) = a.value().attr("href") else { continue };
        if href == ".." || href.starts_with('?') || href.starts_with('#') {
            continue;
        }
        let Ok(full) = base.join(href) else { continue };
        let name = a.text().collect::<String>().trim().to_string();
        if name == "Parent Directory" || name.is_empty() {
            continue;
        }
        let txt = |sel: &Selector| {
            tr.select(sel).next().map(|e| e.text().collect::<String>().trim().to_string()).unwrap_or_default()
        };
        let size_s = txt(&size);
        out.push(Entry {
            is_dir: href.ends_with('/'),
            bytes: parse_size(&size_s),
            modified: txt(&date),
            name,
            url: full.to_string(),
        });
    }
    if out.is_empty() && !html.contains("fb-n") {
        return Err("not an h5ai listing (no entries found)".into());
    }
    Ok(out)
}

/// "4561650 KB" / "1.2 GB" / "378 B" -> bytes
fn parse_size(s: &str) -> u64 {
    let mut it = s.split_whitespace();
    let Some(num) = it.next().and_then(|n| n.replace(',', "").parse::<f64>().ok()) else { return 0 };
    let mult = match it.next().map(|u| u.to_ascii_uppercase()).as_deref() {
        Some("KB") => 1024.0,
        Some("MB") => 1024.0 * 1024.0,
        Some("GB") => 1024.0 * 1024.0 * 1024.0,
        Some("TB") => 1024.0f64.powi(4),
        _ => 1.0,
    };
    (num * mult) as u64
}

pub fn human_size(bytes: u64) -> String {
    if bytes == 0 {
        return String::new();
    }
    let b = bytes as f64;
    if b >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.2} GB", b / (1024.0 * 1024.0 * 1024.0))
    } else if b >= 1024.0 * 1024.0 {
        format!("{:.1} MB", b / (1024.0 * 1024.0))
    } else {
        format!("{:.0} KB", b / 1024.0)
    }
}

/// Natural-ish case-insensitive compare ("Ep 2" < "Ep 10").
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut nx = String::new();
                let mut ny = String::new();
                while let Some(c) = ai.peek().copied().filter(|c| c.is_ascii_digit()) {
                    nx.push(c);
                    ai.next();
                }
                while let Some(c) = bi.peek().copied().filter(|c| c.is_ascii_digit()) {
                    ny.push(c);
                    bi.next();
                }
                let (vx, vy) = (nx.trim_start_matches('0'), ny.trim_start_matches('0'));
                let o = vx.len().cmp(&vy.len()).then_with(|| vx.cmp(vy));
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_h5ai_row() {
        let html = r#"<div id="fallback"><table><tr><td class="fb-i"></td><td class="fb-n"><a href="..">Parent Directory</a></td><td class="fb-d"></td><td class="fb-s"></td></tr>
<tr><td class="fb-n"><a href="/DHAKA-FLIX-14/Hindi%20Movies/%282023%29/">(2023)</a></td><td class="fb-d">2025-04-09 18:07</td><td class="fb-s"></td></tr>
<tr><td class="fb-n"><a href="/DHAKA-FLIX-14/a/Movie%20%282023%29.mkv">Movie (2023).mkv</a></td><td class="fb-d">2025-04-09 18:07</td><td class="fb-s">4561650 KB</td></tr></table></div>"#;
        let base = Url::parse("http://172.16.50.14/DHAKA-FLIX-14/a/").unwrap();
        let e = parse(html, &base).unwrap();
        assert_eq!(e.len(), 2);
        assert!(e[0].is_dir && e[0].name == "(2023)");
        assert_eq!(e[0].url, "http://172.16.50.14/DHAKA-FLIX-14/Hindi%20Movies/%282023%29/");
        assert!(e[1].is_video());
        assert_eq!(e[1].bytes, 4561650 * 1024);
        assert_eq!(e[1].url, "http://172.16.50.14/DHAKA-FLIX-14/a/Movie%20%282023%29.mkv");
    }

    /// Hits the real LAN server: `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn live_listing() {
        let c = make_client();
        let top = fetch(&c, "http://172.16.50.14/DHAKA-FLIX-14/Hindi%20Movies/").unwrap();
        assert!(top.iter().any(|e| e.is_dir));
        let movie = fetch(&c, "http://172.16.50.14/DHAKA-FLIX-14/Hindi%20Movies/%282023%29/12th%20Fail%20%282023%29%201080p/").unwrap();
        assert!(movie.iter().any(|e| e.is_video() && e.bytes > 1_000_000_000));
        assert!(movie.iter().any(|e| e.is_image()));
    }

    #[test]
    fn natural_sort() {
        let mut v = vec!["Ep 10", "Ep 2", "ep 1"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["ep 1", "Ep 2", "Ep 10"]);
    }
}
