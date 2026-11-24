//! The app's own settings, kept in a small JSON file next to the rest of the user's data.
//!
//! Only what the app itself needs: the interface language and the theme. The maps live in the files
//! the user opens, never here, and nothing about them is cached in this file.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::i18n;
use crate::suite_entry::APP;

/// The file name inside the app's folder.
const FILE: &str = "prefs.json";

/// What the app remembers between runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prefs {
    /// The `language` setting: `auto` or a code from `i18n::LANGUAGES`.
    #[serde(default = "auto_language")]
    pub language: String,
    /// The `theme` setting: `system` (follow the OS, the default), `light` or `dark` (`craft_icons::ThemeMode`).
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn auto_language() -> String {
    i18n::AUTO.to_string()
}

fn default_theme() -> String {
    craft_icons::ThemeMode::System.id().to_string()
}

impl Default for Prefs {
    fn default() -> Self {
        Self { language: auto_language(), theme: default_theme() }
    }
}

impl Prefs {
    /// Reads the settings, falling back to the defaults for a missing or unreadable file: a broken
    /// preferences file must never keep the app from opening.
    pub fn load() -> Self {
        let Some(path) = path() else { return Self::default() };
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(prefs) => prefs,
                Err(e) => {
                    log::warn!("prefs: {} is not readable ({e}); using the defaults", path.display());
                    Self::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => {
                log::warn!("prefs: {} could not be read ({e}); using the defaults", path.display());
                Self::default()
            }
        }
    }

    /// Saves the settings. A failure is logged and dropped: losing a language choice is not worth
    /// interrupting the user.
    pub fn save(&self) {
        let Some(path) = path() else { return };
        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            log::warn!("prefs: {} could not be created: {e}", parent.display());
            return;
        }
        match serde_json::to_string_pretty(self) {
            Ok(text) => {
                if let Err(e) = localmindmap_io::save_text(&text, &path) {
                    log::warn!("prefs: {} could not be written: {e}", path.display());
                }
            }
            Err(e) => log::warn!("prefs: could not be serialised: {e}"),
        }
    }
}

/// `<config>/<App Name>/prefs.json`, following the environment (and a redirected Windows profile).
pub fn path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(APP.name).join(FILE))
}

fn config_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::home_dir().map(|home| home.join("Library").join("Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::home_dir().map(|home| home.join(".config")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_language_reads_as_auto() {
        let prefs: Prefs = serde_json::from_str("{}").expect("defaults");
        assert_eq!(prefs.language, i18n::AUTO);
        assert_eq!(prefs.theme, "system");
    }

    #[test]
    fn the_language_and_the_theme_survive_a_round_trip() {
        let prefs = Prefs { language: "pt-br".into(), theme: "dark".into() };
        let text = serde_json::to_string(&prefs).expect("serialize");
        let back: Prefs = serde_json::from_str(&text).expect("parse");
        assert_eq!(back.language, "pt-br");
        assert_eq!(crate::theme::mode_from_pref(&back.theme), craft_icons::ThemeMode::Dark);
    }

    #[test]
    fn the_path_ends_with_the_app_folder_and_the_file() {
        if let Some(path) = path() {
            assert!(path.ends_with(FILE));
            assert!(path.to_string_lossy().contains(APP.name));
        }
    }
}
