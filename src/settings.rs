//! What survives a restart: skin, visualizer mode, volume, shuffle and repeat, and the playlist.
//! `~/.config/nightjar/settings.json`; `NIGHTJAR_CONFIG` points elsewhere.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub skin: usize,
    pub viz: usize,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: u8,
    /// 0 = local files, 1 = YouTube Music.
    pub source: u8,
    /// The modern layout (album art and up next) instead of the classic deck.
    pub modern: bool,
    pub playlist: Vec<PathBuf>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            skin: 0,
            viz: 0,
            volume: 0.8,
            shuffle: false,
            repeat: 0,
            source: 0,
            modern: false,
            playlist: Vec::new(),
        }
    }
}

fn path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("NIGHTJAR_CONFIG") {
        return Some(PathBuf::from(p));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/nightjar/settings.json"))
}

pub fn load() -> Settings {
    path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(s: &Settings) {
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(s) {
        // Write then rename, so a crash mid-write never leaves half a file.
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(tmp, p);
        }
    }
}
