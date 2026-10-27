use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum PlayerKind {
    #[default]
    Vlc,
    PotPlayer,
    Custom,
}

impl PlayerKind {
    pub const ALL: [PlayerKind; 3] = [PlayerKind::Vlc, PlayerKind::PotPlayer, PlayerKind::Custom];
    pub fn label(self) -> &'static str {
        match self {
            PlayerKind::Vlc => "VLC",
            PlayerKind::PotPlayer => "PotPlayer",
            PlayerKind::Custom => "Custom",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Link {
    pub name: String,
    pub url: String,
    /// Poster image URL, when known.
    pub cover: Option<String>,
    /// Title folder this file belongs to (for "Continue watching").
    pub folder: Option<String>,
    /// Display title of that folder.
    pub title: Option<String>,
}

impl Link {
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Link { name: name.into(), url: url.into(), ..Default::default() }
    }
}

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(default)]
pub struct Config {
    pub player: PlayerKind,
    pub vlc_path: String,
    pub pot_path: String,
    pub custom_path: String,
    /// Extra args for the custom player; `{}` is replaced by the URL (appended if absent).
    pub custom_args: String,
    pub favorites: Vec<Link>,
    pub history: Vec<Link>,
    pub last_url: Option<String>,
    pub card_size: f32,
}

fn path() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("DHAKAFLIX_DATA_DIR") {
        return Some(PathBuf::from(d).join("config.json"));
    }
    dirs::config_dir().map(|d| d.join("dhakaflix-player").join("config.json"))
}

/// Cache dir (playlists, thumbnails, cover index). Override with DHAKAFLIX_DATA_DIR for tests.
pub fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("DHAKAFLIX_DATA_DIR") {
        return PathBuf::from(d);
    }
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("dhakaflix-player")
}

impl Config {
    pub fn load() -> Self {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(p) = path() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(s) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(p, s);
            }
        }
    }

    pub fn push_history(&mut self, l: Link) {
        self.history.retain(|h| h.url != l.url);
        self.history.insert(0, l);
        self.history.truncate(50);
    }
}
