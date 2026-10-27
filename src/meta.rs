//! Turns DhakaFlix folder / file names into display metadata.
//! e.g. "001. The Godfather (1972) 1080p [Dual Audio]" or
//! "Alexa & Katie (TV Series 2018–2020) 720p [Dual Audio]".

use regex::Regex;
use std::sync::LazyLock;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TitleInfo {
    pub title: String,
    pub year: Option<String>,
    pub quality: Option<String>,
    pub tags: Vec<String>,
    pub rank: Option<u32>,
    pub series: bool,
}

static TITLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:(\d{1,3})\.\s+)?(.+?)\s*\(\s*((?:TV\s+)?(?:Mini\s+)?(?:Series|Movie|Special)\s*)?(\d{4})([^)]*)\)\s*(.*)$").unwrap()
});
static QUALITY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(2160p|4k|1080p|720p|576p|480p|360p)\b").unwrap());
static BRACKET_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\[(]([^\])]+)[\])]").unwrap());
static SOURCE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(WEB-?Rip|WEB-?DL|BluRay|BRRip|HDRip|HDTV|DVDRip|S-Print|HDTC|CAMRip|HEVC|x265|10bit|HDR)\b").unwrap()
});
static SEASON_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(season|series|s)\s*0*(\d{1,3})\b|^specials?\b").unwrap());
static EPISODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bS(\d{1,2})\s*E(\d{1,3})\b|\b(?:ep|episode|e)\s*\.?\s*(\d{1,3})\b").unwrap());

/// Parse a title folder name. Returns None for grouping folders such as "(2023)" or "Chinese Language".
pub fn parse_title(name: &str) -> Option<TitleInfo> {
    let c = TITLE_RE.captures(name.trim())?;
    let title = c.get(2)?.as_str().trim().trim_end_matches(['-', '–', ':']).trim().to_string();
    if title.is_empty() {
        return None;
    }
    let series = c.get(3).is_some_and(|m| m.as_str().to_ascii_lowercase().contains("series"));
    let year_tail = c.get(5).map(|m| m.as_str().trim()).unwrap_or("");
    let year = c.get(4).map(|m| {
        let y = m.as_str().to_string();
        if year_tail.starts_with(['–', '-']) {
            let end = year_tail.trim_start_matches(['–', '-']).trim();
            if end.is_empty() { format!("{y}–") } else { format!("{y}–{end}") }
        } else {
            y
        }
    });
    let rest = c.get(6).map(|m| m.as_str()).unwrap_or("");
    let (quality, tags) = quality_and_tags(rest);
    Some(TitleInfo {
        title,
        year,
        quality,
        tags,
        rank: c.get(1).and_then(|m| m.as_str().parse().ok()),
        series,
    })
}

pub fn quality_and_tags(s: &str) -> (Option<String>, Vec<String>) {
    let quality = QUALITY_RE.find(s).map(|m| m.as_str().to_ascii_lowercase().replace("4k", "4K"));
    let mut tags: Vec<String> = Vec::new();
    for c in BRACKET_RE.captures_iter(s) {
        let t = c[1].trim();
        let year_only = t.len() == 4 && t.chars().all(|c| c.is_ascii_digit());
        if t.len() <= 24 && !year_only && !QUALITY_RE.is_match(t) && !tags.iter().any(|x| x.eq_ignore_ascii_case(t)) {
            tags.push(t.to_string());
        }
    }
    for m in SOURCE_RE.find_iter(s) {
        let t = m.as_str();
        if !tags.iter().any(|x| x.eq_ignore_ascii_case(t)) {
            tags.push(t.to_string());
        }
    }
    tags.truncate(3);
    (quality, tags)
}

pub fn is_season(name: &str) -> bool {
    SEASON_RE.is_match(name.trim())
}

/// "S01E03" -> Some((Some(1), 3)); "Episode 7" -> Some((None, 7))
pub fn episode(name: &str) -> Option<(Option<u32>, u32)> {
    let c = EPISODE_RE.captures(name)?;
    if let (Some(s), Some(e)) = (c.get(1), c.get(2)) {
        return Some((s.as_str().parse().ok(), e.as_str().parse().ok()?));
    }
    Some((None, c.get(3)?.as_str().parse().ok()?))
}

/// Grouping folders named by year ("(2023)", "2001", "(2000) & Before") should default to the newest.
pub fn looks_like_year_group(name: &str) -> bool {
    let t = name.trim().trim_start_matches('(');
    t.len() >= 4 && t[..4].chars().all(|c| c.is_ascii_digit())
}

/// Tidy label for a grouping folder: "TV Series ♥  A  —  L" -> "A – L", "(2023) 1080p" -> "2023".
pub fn group_label(name: &str) -> String {
    let n = name.trim();
    if looks_like_year_group(n) {
        let t = n.trim_start_matches('(');
        let year = &t[..4];
        let rest = t[4..].trim_start_matches(|c: char| c == ')' || c == '-' || c.is_ascii_digit());
        let rest = QUALITY_RE.replace_all(rest, "");
        let rest = rest.trim().trim_start_matches(')').trim();
        let range_end: String = t[4..].trim_start_matches('-').chars().take_while(|c| c.is_ascii_digit()).collect();
        let mut s = if range_end.len() == 4 { format!("{year}–{range_end}") } else { year.to_string() };
        if !rest.is_empty() {
            s.push(' ');
            s.push_str(rest);
        }
        return s;
    }
    if n.contains('—') {
        let cleaned: String = n
            .split_whitespace()
            .filter(|w| w.chars().all(|c| c.is_alphanumeric() || c == '—'))
            .filter(|w| !matches!(w.to_ascii_lowercase().as_str(), "tv" | "series" | "web"))
            .collect::<Vec<_>>()
            .join(" ");
        if !cleaned.is_empty() {
            return cleaned.replace('—', "–");
        }
    }
    // "Marvel Cinematic Universe-MCU Collection 1080p [Dual Audio]" -> "Marvel Cinematic Universe-MCU Collection"
    let cleaned = BRACKET_RE.replace_all(n, "");
    let cleaned = QUALITY_RE.replace_all(&cleaned, "");
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() { n.to_string() } else { cleaned }
}

pub fn strip_ext(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((a, e)) if e.len() <= 5 => a,
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movie() {
        let t = parse_title("001. The Shawshank Redemption (1994) 1080p [Dual Audio]").unwrap();
        assert_eq!(t.title, "The Shawshank Redemption");
        assert_eq!(t.year.as_deref(), Some("1994"));
        assert_eq!(t.quality.as_deref(), Some("1080p"));
        assert_eq!(t.tags, vec!["Dual Audio"]);
        assert_eq!(t.rank, Some(1));
        assert!(!t.series);
    }

    #[test]
    fn series() {
        let t = parse_title("Alexa & Katie (TV Series 2018–2020) 720p [Dual Audio]").unwrap();
        assert_eq!(t.title, "Alexa & Katie");
        assert_eq!(t.year.as_deref(), Some("2018–2020"));
        assert!(t.series);
        let t = parse_title("A Good Girl's Guide to Murder (TV Series 2024– ) 720p [Dual Audio]").unwrap();
        assert_eq!(t.year.as_deref(), Some("2024–"));
        let t = parse_title("72 Hoorain (2023) 1080p S-Print").unwrap();
        assert_eq!(t.tags, vec!["S-Print"]);
        let t = parse_title("12 Golden Ducks (2015) 720p (Hong Kong)").unwrap();
        assert_eq!(t.tags, vec!["Hong Kong"]);
    }

    #[test]
    fn groups() {
        assert!(parse_title("(2023)").is_none());
        assert!(parse_title("(2000) & Before").is_none());
        assert!(parse_title("Chinese Language").is_none());
        assert!(parse_title("2001").is_none());
        assert_eq!(parse_title("125. 1917 (2019) 1080p").unwrap().title, "1917");
        assert_eq!(parse_title("24 (TV Series 2001–2010) 1080p").unwrap().title, "24");
        assert_eq!(group_label("Marvel Cinematic Universe-MCU Collection 1080p [Dual Audio]"), "Marvel Cinematic Universe-MCU Collection");
        assert_eq!(group_label("(2023)"), "2023");
        assert_eq!(group_label("(2022) 1080p"), "2022");
        assert_eq!(group_label("(1960-1994)"), "1960–1994");
        assert_eq!(group_label("(2000) & Before"), "2000 & Before");
        assert_eq!(group_label("TV Series ♥  A  —  L"), "A – L");
        assert!(looks_like_year_group("2000 & Before"));
        assert!(is_season("Season 1") && is_season("S02") && !is_season("Seasons Greetings (2020)"));
    }

    #[test]
    fn episodes() {
        assert_eq!(episode("Alexa & Katie S01E03  720p NF.mkv"), Some((Some(1), 3)));
        assert_eq!(episode("Show Episode 12.mkv"), Some((None, 12)));
    }
}
