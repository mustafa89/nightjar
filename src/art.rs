//! Album art for the modern layout: the cover embedded in a file's tags, else a cover image in
//! its folder, or the YouTube player's artwork URL. One worker finds the bytes; Rune's image
//! cache decodes them off the main thread.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardVisualKey};
use symphonia::core::probe::Hint;

/// Bigger files aren't album art.
const MAX_BYTES: u64 = 16 * 1024 * 1024;
/// Decoded covers kept; older ones go when the cache is full.
const KEEP: usize = 24;
const COVER_NAMES: &[&str] = &["cover", "folder", "front", "album", "albumart"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Local(PathBuf),
    Remote(String),
}

impl Source {
    pub fn key(&self) -> String {
        match self {
            Source::Local(p) => format!("file:{}", p.display()),
            Source::Remote(u) => u.clone(),
        }
    }
}

pub struct ArtStore {
    cache: rune::ImageCache,
    jobs: Sender<Source>,
    done: Receiver<(String, Option<Vec<u8>>)>,
    /// Finished lookups: the image, or None when the track has no art.
    found: HashMap<String, Option<rune::Image>>,
    order: Vec<String>,
    asked: std::collections::HashSet<String>,
}

impl ArtStore {
    pub fn new(waker: rune::Waker) -> ArtStore {
        let (jobs, rx) = channel::<Source>();
        let (tx, done) = channel();
        let wake = waker.clone();
        std::thread::Builder::new()
            .name("nightjar-art".into())
            .spawn(move || {
                for source in rx {
                    let bytes = match &source {
                        Source::Local(p) => embedded(p).or_else(|| folder_cover(p)),
                        Source::Remote(url) => fetch(url),
                    };
                    if tx.send((source.key(), bytes)).is_err() {
                        break;
                    }
                    wake();
                }
            })
            .expect("spawn art worker");
        ArtStore {
            cache: rune::ImageCache::new(waker),
            jobs,
            done,
            found: HashMap::new(),
            order: Vec::new(),
            asked: Default::default(),
        }
    }

    /// The art for `source` if it's known: Some(Some(image)) found, Some(None) none exists,
    /// None still looking (the lookup starts on the first call).
    pub fn get(&mut self, source: &Source) -> Option<Option<rune::Image>> {
        let key = source.key();
        if let Some(found) = self.found.get(&key) {
            return Some(found.clone());
        }
        if self.asked.insert(key) {
            let _ = self.jobs.send(source.clone());
        }
        None
    }

    /// Takes finished lookups. Returns true when any arrived.
    pub fn poll(&mut self) -> bool {
        let mut any = false;
        for (key, bytes) in self.done.try_iter() {
            any = true;
            self.asked.remove(&key);
            let image = bytes.map(|b| self.cache.load(b));
            self.found.insert(key.clone(), image);
            self.order.push(key);
            if self.order.len() > KEEP {
                let old = self.order.remove(0);
                self.found.remove(&old);
            }
        }
        any
    }

    /// Straight RGBA, for headless scenes.
    pub fn rgba(&self, width: u32, height: u32, bytes: Vec<u8>) -> rune::Image {
        self.cache.rgba(width, height, bytes)
    }
}

/// The front cover in the file's tags (ID3 APIC, FLAC picture, MP4 covr), else any picture.
pub fn embedded(path: &Path) -> Option<Vec<u8>> {
    let file = File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;
    let pick = |rev: &MetadataRevision| {
        let visuals = rev.visuals();
        visuals
            .iter()
            .find(|v| v.usage == Some(StandardVisualKey::FrontCover))
            .or(visuals.first())
            .map(|v| v.data.to_vec())
    };
    let before = probed
        .metadata
        .get()
        .as_ref()
        .and_then(|m| m.current().and_then(pick));
    before.or_else(|| probed.format.metadata().current().and_then(pick))
}

/// cover.jpg, folder.png, Front.jpeg... beside the file, in any letter case.
pub fn folder_cover(path: &Path) -> Option<Vec<u8>> {
    let dir = path.parent()?;
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let ext = p
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            COVER_NAMES.contains(&stem.as_str()) && ["jpg", "jpeg", "png"].contains(&ext.as_str())
        })
        .collect();
    // "cover" before "folder" before the rest, as COVER_NAMES orders them.
    candidates.sort_by_key(|p| {
        let stem = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        COVER_NAMES.iter().position(|n| *n == stem)
    });
    let p = candidates.first()?;
    if std::fs::metadata(p).ok()?.len() > MAX_BYTES {
        return None;
    }
    std::fs::read(p).ok()
}

/// HTTPS only, ten seconds, 16 MB at most.
fn fetch(url: &str) -> Option<Vec<u8>> {
    if !url.starts_with("https://") {
        return None;
    }
    let response = ureq::get(url)
        .timeout(Duration::from_secs(10))
        .call()
        .ok()?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    (!bytes.is_empty()).then_some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cover_file_beside_the_music_is_found_in_any_case() {
        let dir = std::env::temp_dir().join(format!("nightjar-art-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("song.mp3"), b"").unwrap();
        std::fs::write(dir.join("notes.jpg"), b"not a cover").unwrap();
        std::fs::write(dir.join("Folder.JPG"), b"folder").unwrap();
        std::fs::write(dir.join("cover.png"), b"cover").unwrap();
        assert_eq!(
            folder_cover(&dir.join("song.mp3")).as_deref(),
            Some(&b"cover"[..]),
            "cover wins over folder"
        );
        std::fs::remove_file(dir.join("cover.png")).unwrap();
        assert_eq!(
            folder_cover(&dir.join("song.mp3")).as_deref(),
            Some(&b"folder"[..])
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_https_is_fetched() {
        assert_eq!(fetch("http://example.com/a.jpg"), None);
        assert_eq!(fetch("file:///etc/passwd"), None);
    }
}
