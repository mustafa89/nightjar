//! Tracks in the playlist: what files count as music, folder scans, and tags and durations read
//! on a background thread so a dropped folder shows at once and fills in as it's read.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey};
use symphonia::core::probe::Hint;

pub const EXTENSIONS: &[&str] = &["mp3", "flac", "wav", "m4a", "aac", "ogg", "oga", "mp4"];

#[derive(Clone, Debug, Default)]
pub struct Meta {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: Option<f64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub codec: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Track {
    pub path: PathBuf,
    pub meta: Meta,
    pub read: bool,
}

impl Track {
    pub fn new(path: PathBuf) -> Track {
        Track {
            path,
            meta: Meta::default(),
            read: false,
        }
    }

    /// The tag title, else the file name without its extension.
    pub fn title(&self) -> String {
        self.meta.title.clone().unwrap_or_else(|| {
            self.path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
    }

    pub fn artist(&self) -> &str {
        self.meta.artist.as_deref().unwrap_or("")
    }
}

pub fn is_music(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// Music files under `paths` (files as given, folders walked), each folder in name order.
pub fn expand(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in paths {
        walk(p, &mut out, 0);
    }
    out
}

fn walk(p: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if p.is_dir() {
        if depth > 8 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(p) else { return };
        let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort_by_key(|e| e.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        for e in entries {
            walk(&e, out, depth + 1);
        }
    } else if is_music(p) {
        out.push(p.to_path_buf());
    }
}

/// Reads tags and stream parameters without decoding audio.
pub fn read_meta(path: &Path) -> Meta {
    let mut meta = Meta::default();
    let Ok(file) = File::open(path) else {
        return meta;
    };
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let Ok(mut probed) = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    ) else {
        return meta;
    };
    // Tags ahead of the container (ID3v2 on an MP3) first, then the container's own.
    if let Some(rev) = probed
        .metadata
        .get()
        .as_ref()
        .and_then(|m| m.current().cloned())
    {
        apply(&mut meta, &rev);
    }
    if let Some(rev) = probed.format.metadata().current() {
        apply(&mut meta, rev);
    }
    if let Some(track) = probed.format.default_track() {
        let p = &track.codec_params;
        meta.sample_rate = p.sample_rate;
        meta.channels = p.channels.map(|c| c.count() as u16);
        if let (Some(frames), Some(rate)) = (p.n_frames, p.sample_rate) {
            meta.duration = Some(frames as f64 / rate as f64);
        }
        meta.codec = symphonia::default::get_codecs()
            .get_codec(p.codec)
            .map(|d| d.short_name.to_uppercase());
    }
    meta
}

fn apply(meta: &mut Meta, rev: &MetadataRevision) {
    for tag in rev.tags() {
        let value = tag.value.to_string().trim().to_string();
        if value.is_empty() {
            continue;
        }
        match tag.std_key {
            Some(StandardTagKey::TrackTitle) => meta.title.get_or_insert(value),
            Some(StandardTagKey::Artist) => meta.artist.get_or_insert(value),
            Some(StandardTagKey::AlbumArtist) => meta.artist.get_or_insert(value),
            Some(StandardTagKey::Album) => meta.album.get_or_insert(value),
            _ => continue,
        };
    }
}

/// A background reader: send paths in, get `(path, meta)` out.
pub struct MetaReader {
    tx: Sender<PathBuf>,
    pub rx: Receiver<(PathBuf, Meta)>,
}

impl MetaReader {
    pub fn spawn(waker: Option<rune::Waker>) -> MetaReader {
        let (tx, jobs) = channel::<PathBuf>();
        let (done, rx) = channel();
        std::thread::Builder::new()
            .name("nightjar-tags".into())
            .spawn(move || {
                for path in jobs {
                    let meta = read_meta(&path);
                    if done.send((path, meta)).is_err() {
                        break;
                    }
                    if let Some(w) = &waker {
                        w();
                    }
                }
            })
            .expect("spawn tag reader");
        MetaReader { tx, rx }
    }

    pub fn request(&self, path: PathBuf) {
        let _ = self.tx.send(path);
    }
}

pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}
