//! Reading and writing the user's `.tmind` files.
//!
//! The format belongs to the user: it is the JSON the Electron LocalMind has been writing since
//! 0.1, and keeping it readable by both apps is the whole point of this crate. Two consequences:
//!
//! - a save writes a sibling file and renames it over the target, so an interrupted write leaves
//!   the previous file intact (the user's map is never half written);
//! - what this build does not understand is not thrown away: [`localmindmap_model`] carries unknown
//!   fields and writes them back, and this crate only decides layout of the bytes
//!   (`serde_json::to_string_pretty`, two spaces, no trailing newline: what the other app writes).

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable))]

use std::path::{Path, PathBuf};

use localmindmap_model::MindMap;

/// The extension of a mind map file, without the dot.
pub const EXTENSION: &str = "tmind";

/// The largest file that is read. A map is text a person typed; anything past this is not one, and
/// reading it would only stall the app.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Why a map could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The file could not be read or written.
    Io(String),
    /// The bytes are not a mind map.
    Format(String),
    /// The file is larger than [`MAX_FILE_BYTES`].
    TooLarge,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(message) => write!(f, "{message}"),
            Error::Format(message) => write!(f, "{message}"),
            Error::TooLarge => write!(f, "the file is larger than {} MB", MAX_FILE_BYTES / (1024 * 1024)),
        }
    }
}

impl std::error::Error for Error {}

/// Parses a map from the text of a `.tmind`.
pub fn parse(text: &str) -> Result<MindMap, Error> {
    serde_json::from_str(text).map_err(|e| Error::Format(e.to_string()))
}

/// Reads a map from disk.
pub fn load(path: &Path) -> Result<MindMap, Error> {
    let size = std::fs::metadata(path).map_err(|e| Error::Io(e.to_string()))?.len();
    if size > MAX_FILE_BYTES {
        return Err(Error::TooLarge);
    }
    let text = std::fs::read_to_string(path).map_err(|e| Error::Io(e.to_string()))?;
    parse(&text)
}

/// The bytes a save writes: pretty JSON, two spaces, exactly the shape the other app produces.
pub fn to_json(map: &MindMap) -> Result<String, Error> {
    serde_json::to_string_pretty(map).map_err(|e| Error::Format(e.to_string()))
}

/// Writes the map to `path` through a sibling file and a rename.
pub fn save(map: &MindMap, path: &Path) -> Result<(), Error> {
    let text = to_json(map)?;
    save_text(&text, path)
}

/// Writes `text` to `path` through a sibling file and a rename.
pub fn save_text(text: &str, path: &Path) -> Result<(), Error> {
    save_bytes(text.as_bytes(), path)
}

/// Writes `bytes` to `path` through a sibling file and a rename. Both a map and an export go through
/// here, so an interrupted write never leaves half a file.
pub fn save_bytes(bytes: &[u8], path: &Path) -> Result<(), Error> {
    let temp = temp_sibling(path);
    std::fs::write(&temp, bytes).map_err(|e| Error::Io(e.to_string()))?;
    // `rename` replaces an existing file on both Windows (MoveFileEx with MOVEFILE_REPLACE_EXISTING)
    // and Unix, and it is the only step the user can perceive either way.
    match std::fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&temp);
            Err(Error::Io(e.to_string()))
        }
    }
}

/// The temporary file a save writes before renaming it over the target: same directory (same
/// volume, so the rename is atomic) and a name no other instance will pick.
fn temp_sibling(path: &Path) -> PathBuf {
    let name = path.file_name().unwrap_or_else(|| std::ffi::OsStr::new("map"));
    let mut temp = name.to_os_string();
    temp.push(format!(".{}.tmp", std::process::id()));
    path.with_file_name(temp)
}

/// The path a save dialog starts from: `<dir>/<title>.tmind`, with a title that is safe to write
/// on Windows and Linux.
pub fn suggested_name(title: &str) -> String {
    let cleaned: String = title.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { ' ' }).collect();
    let cleaned = cleaned.trim();
    let stem = if cleaned.is_empty() { "map" } else { cleaned };
    format!("{stem}.{EXTENSION}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use localmindmap_model::Side;

    fn sample() -> MindMap {
        let mut map = MindMap::new("Mapa", "Central");
        let root = map.root_id().map(str::to_string).unwrap_or_default();
        map.add_child(&root, "Ramo", Some(Side::Right));
        map.set_note(&root, Some("contexto".into()));
        map
    }

    #[test]
    fn a_map_survives_a_round_trip() {
        let map = sample();
        let text = to_json(&map).expect("serialize");
        let back = parse(&text).expect("parse");
        assert_eq!(map, back);
    }

    /// The other app writes `parentId`, `createdAt`, `updatedAt` and `fontSize`: the file has to
    /// read with those exact names, not with Rust's snake_case.
    #[test]
    fn the_file_keys_are_camel_case() {
        let text = to_json(&sample()).expect("serialize");
        assert!(text.contains("\"parentId\""), "{text}");
        assert!(text.contains("\"createdAt\""), "{text}");
        assert!(text.contains("\"updatedAt\""), "{text}");
        assert!(!text.contains("parent_id"), "{text}");
    }

    /// Fields a newer build wrote are carried, not dropped: opening and saving a map must not be a
    /// lossy operation.
    #[test]
    fn unknown_fields_survive_a_round_trip() {
        let text = r#"{
          "id": "m1", "title": "t", "createdAt": 1, "updatedAt": 2,
          "nodes": [{ "id": "r", "text": "R", "parentId": null, "sticker": "star" }],
          "collaborators": ["ana"]
        }"#;
        let map = parse(text).expect("parse");
        let back = to_json(&map).expect("serialize");
        assert!(back.contains("\"sticker\": \"star\""), "{back}");
        assert!(back.contains("\"collaborators\""), "{back}");
    }

    /// A file with all the optional node fields absent opens: `collapsed`, `color`, `note`,
    /// `width`, `fontSize` and `side` are exactly that, optional.
    #[test]
    fn a_minimal_file_opens() {
        let text = r#"{"id":"m","title":"t","createdAt":0,"updatedAt":0,
                       "nodes":[{"id":"r","text":"R","parentId":null}]}"#;
        let map = parse(text).expect("parse");
        let root = map.root().expect("root");
        assert_eq!(root.text, "R");
        assert_eq!(root.collapsed, None);
        assert_eq!(root.side, None);
    }

    /// `side` is written exactly as the other app wrote it (`"left"`, `"up"`, .).
    #[test]
    fn the_side_is_written_in_lowercase() {
        let mut map = sample();
        let root = map.root_id().map(str::to_string).unwrap_or_default();
        map.add_child(&root, "Cima", Some(Side::Up));
        let text = to_json(&map).expect("serialize");
        assert!(text.contains("\"side\": \"up\""), "{text}");
    }

    /// A file that is not a map reports why instead of opening an empty canvas.
    #[test]
    fn a_broken_file_is_an_error() {
        assert!(matches!(parse("não é json"), Err(Error::Format(_))));
        assert!(matches!(parse("{\"nodes\": 3}"), Err(Error::Format(_))));
    }

    #[test]
    fn save_writes_and_replaces_the_file() {
        let dir = std::env::temp_dir().join(format!("localmindmap-io-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch");
        let path = dir.join("mapa.tmind");
        let map = sample();
        save(&map, &path).expect("first save");
        let mut changed = map.clone();
        changed.title = "Outro".into();
        save(&changed, &path).expect("second save");
        let loaded = load(&path).expect("load");
        assert_eq!(loaded.title, "Outro");
        // No temporary file is left behind on the happy path.
        let leftovers: Vec<_> =
            std::fs::read_dir(&dir).expect("dir").flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_suggested_name_is_safe_on_both_platforms() {
        assert_eq!(suggested_name("Meu mapa"), "Meu mapa.tmind");
        assert_eq!(suggested_name("a/b:c*?"), "a b c.tmind");
        assert_eq!(suggested_name(""), "map.tmind");
    }
}
