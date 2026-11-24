//! The open map: its nodes, where it came from, and whether it has unsaved changes.

use std::path::{Path, PathBuf};

use localmindmap_io as map_io;
use localmindmap_model::MindMap;

/// One open map.
pub struct Document {
    pub map: MindMap,
    /// The file it came from, `None` until it is saved somewhere.
    pub path: Option<PathBuf>,
    /// Edits since the last save.
    pub dirty: bool,
}

impl Document {
    /// A new, empty map.
    pub fn untitled(title: &str, root_text: &str) -> Self {
        Self { map: MindMap::new(title, root_text), path: None, dirty: false }
    }

    /// Reads a map from disk. The path becomes the document's own only when the read worked.
    pub fn open(path: &Path) -> Result<Self, map_io::Error> {
        let map = map_io::load(path)?;
        Ok(Self { map, path: Some(path.to_path_buf()), dirty: false })
    }

    /// Writes the map to `path`, which becomes the document's file.
    pub fn save(&mut self, path: &Path) -> Result<(), map_io::Error> {
        map_io::save(&self.map, path)?;
        self.path = Some(path.to_path_buf());
        self.dirty = false;
        Ok(())
    }

    /// The file name, or `untitled` for a map that was never saved.
    pub fn file_label(&self, untitled: &str) -> String {
        self.path.as_ref().and_then(|p| p.file_name()).map_or_else(|| untitled.to_string(), |name| name.to_string_lossy().into_owned())
    }

    /// The root's id, which is what a fresh selection points at.
    pub fn root_id(&self) -> Option<String> {
        self.map.root_id().map(str::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("localmindmap-doc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    #[test]
    fn a_new_map_is_untitled_and_clean() {
        let doc = Document::untitled("Novo mapa", "Central");
        assert!(doc.path.is_none());
        assert!(!doc.dirty);
        assert_eq!(doc.file_label("Sem título"), "Sem título");
        assert_eq!(doc.root_id(), doc.map.root_id().map(str::to_string));
    }

    #[test]
    fn opening_and_saving_round_trips_through_the_file() {
        let dir = scratch("roundtrip");
        let path = dir.join("mapa.tmind");
        let mut doc = Document::untitled("Mapa", "Central");
        doc.save(&path).expect("save");
        assert!(!doc.dirty);
        assert_eq!(doc.file_label("x"), "mapa.tmind");

        let opened = Document::open(&path).expect("open");
        assert_eq!(opened.map.title, "Mapa");
        assert_eq!(opened.path.as_deref(), Some(path.as_path()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opening_a_file_that_is_not_a_map_reports_the_error() {
        let dir = scratch("broken");
        let path = dir.join("quebrado.tmind");
        std::fs::write(&path, "não é json").expect("write");
        assert!(Document::open(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
