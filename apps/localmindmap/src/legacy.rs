//! What the rename from Local Mind to Local Mindmap left behind.
//!
//! The product was called Local Mind, and its names went with it: the per-user folders, the
//! keychain service and the environment variables. This module does two things, both quietly and
//! both safe to run on every start:
//!
//! * [`migrate_all`] moves each old-named folder (preferences, the AI connections, the updater's
//!   data and cache, the portable data folder) to its new name, once. It never overwrites: when the
//!   new folder exists the old one is left alone, and when the move has to be a copy (another
//!   volume, a file still open) the old folder stays as it was. Every move is logged. The API keys
//!   in the OS keychain are copied to the new service name, and the old entries are kept.
//! * [`env_var_os`] reads `LOCALMINDMAP_<NAME>` and falls back to the old `LOCALMIND_<NAME>`.
//!
//! The maps are not touched: a `.tmind` file belongs to the user and keeps its format.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use local_ai::{App as AiApp, Connection};

use crate::ai::{AI_APP_ID, AI_APP_URL};
use crate::suite_entry::APP;

/// The app identity the connections file and the keychain entries were filed under before the
/// rename (`AI_APP_ID`'s old value; the catalog keeps it as a `former_ids` entry).
const AI_LEGACY_APP_ID: &str = "LocalMind";

/// The identity the AI connections and their keys belong to now, and the same identity under the
/// old name.
const AI_APP: AiApp = AiApp { id: AI_APP_ID, url: AI_APP_URL };
const AI_LEGACY_APP: AiApp = AiApp { id: AI_LEGACY_APP_ID, url: AI_APP_URL };

/// `(new, old)` for each folder-name convention the app used: the preferences folder (the display
/// name, which is also the updater's folder on Windows and macOS), the updater's per-user folder on
/// Linux (the app id), the AI connections folder and the keychain service (the AI app id), and the
/// portable data folder beside the executable (the display name without spaces).
const NAMES: [(&str, &str); 4] =
    [(APP.name, "Local Mind"), (APP.id, "localmind"), (AI_APP_ID, AI_LEGACY_APP_ID), ("LocalMindmapData", "LocalMindData")];

/// How deep [`copy_tree`] follows folders: settings are a few levels deep at most.
const MAX_DEPTH: u32 = 16;

/// What [`migrate_dir`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// There is no old folder, or the new one exists already: nothing to do.
    Nothing,
    /// The old folder was renamed to the new one.
    Moved,
    /// The contents were copied; the old folder is still there.
    Copied,
    /// Neither worked; the old folder is untouched and the next start tries again.
    Failed(String),
}

/// The variables [`env_var_os`] looks at for `name`, the current one first.
fn candidates(name: &str) -> [String; 2] {
    [format!("LOCALMINDMAP_{name}"), format!("LOCALMIND_{name}")]
}

/// The first of `names` that `lookup` returns a value for.
fn first_set<F: Fn(&str) -> Option<OsString>>(names: [String; 2], lookup: F) -> Option<OsString> {
    names.into_iter().find_map(|name| lookup(&name))
}

/// An environment variable of this app: `LOCALMINDMAP_<name>`, else the old `LOCALMIND_<name>`.
pub fn env_var_os(name: &str) -> Option<OsString> {
    first_set(candidates(name), |name| std::env::var_os(name))
}

/// The folder the old name used in place of `new` (the last component swapped), when `new` carries
/// one of the app's folder names.
pub fn old_dir_of(new: &Path) -> Option<PathBuf> {
    let name = new.file_name()?.to_str()?;
    let (_, old) = NAMES.iter().find(|(current, _)| *current == name)?;
    Some(new.with_file_name(old))
}

/// [`migrate_dir`] from the folder the old name used in place of `new`, when there is one.
pub fn migrate_from_old_name(new: &Path) -> Outcome {
    old_dir_of(new).map_or(Outcome::Nothing, |old| migrate_dir(&old, new))
}

/// Move `old` to `new`, once: only when `old` is a folder and `new` doesn't exist. A rename on one
/// volume; otherwise the contents are copied to a staging folder beside `new` that is renamed into
/// place when complete, so `new` never exists half-copied. Nothing at `new` is ever replaced.
pub fn migrate_dir(old: &Path, new: &Path) -> Outcome {
    if !old.is_dir() || std::fs::symlink_metadata(new).is_ok() {
        return Outcome::Nothing;
    }
    if let Some(parent) = new.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        log::warn!("couldn't move {} to {}: {e}", old.display(), new.display());
        return Outcome::Failed(e.to_string());
    }
    if std::fs::rename(old, new).is_ok() {
        log::info!("moved the Local Mind folder {} to {}", old.display(), new.display());
        return Outcome::Moved;
    }
    let name = new.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let stage = new.with_file_name(format!(".{name}.migrating-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&stage);
    let copied = copy_tree(old, &stage, 0).and_then(|()| {
        if std::fs::symlink_metadata(new).is_ok() {
            Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "the new folder appeared meanwhile"))
        } else {
            std::fs::rename(&stage, new)
        }
    });
    match copied {
        Ok(()) => {
            log::info!("copied the Local Mind folder {} to {} (the old one stays)", old.display(), new.display());
            Outcome::Copied
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&stage);
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                return Outcome::Nothing;
            }
            log::warn!("couldn't copy {} to {}: {e}; the old settings stay where they are", old.display(), new.display());
            Outcome::Failed(e.to_string())
        }
    }
}

/// Copy the folder `from` to the new folder `to`: plain files and folders only (links are skipped).
fn copy_tree(from: &Path, to: &Path, depth: u32) -> std::io::Result<()> {
    if depth > MAX_DEPTH {
        return Err(std::io::Error::other(format!("{} is nested too deep", from.display())));
    }
    std::fs::create_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &target, depth.saturating_add(1))?;
        } else if kind.is_file() {
            let mut src = std::fs::File::open(entry.path())?;
            let mut dst = std::fs::OpenOptions::new().write(true).create_new(true).open(&target)?;
            std::io::copy(&mut src, &mut dst)?;
            dst.sync_all()?;
        }
    }
    Ok(())
}

/// The folders this version reads and writes, which an old-named twin may sit beside.
fn current_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    // The preferences (the display name, which is also the updater's folder on Windows and macOS).
    if let Some(dir) = crate::prefs::config_dir() {
        dirs.push(dir.join(APP.name));
    }
    // The AI connections (`<data dir>/<AI app id>`).
    if let Some(dir) = Connection::store_path(&AI_APP).and_then(|file| file.parent().map(Path::to_path_buf)) {
        dirs.push(dir);
    }
    // The updater's records, logs and downloads (the portable root when `portable.txt` is there).
    if let Ok(paths) = craft_update::Paths::for_app(&APP) {
        dirs.push(paths.data);
        dirs.push(paths.cache);
    }
    // The portable data folder beside the executable.
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) {
        dirs.push(exe_dir.join(format!("{}Data", APP.name.replace(' ', ""))));
    }
    // On Windows the updater's data and cache folders are the same one.
    dirs.dedup();
    dirs
}

/// Move every old-named folder to its new name, then copy the keychain keys. Called before
/// anything reads those folders; never fails (what can't be moved is logged and tried again next
/// start).
pub fn migrate_all() {
    for new in current_dirs() {
        migrate_from_old_name(&new);
    }
    // The connections file moved with its folder; the keys live in the keychain, under the old
    // service name.
    copy_legacy_keys();
}

/// Copies the API keys saved under the old keychain service to the current one, for every saved
/// connection that has none yet. A key already under the current service is never replaced, and the
/// old entries are never deleted (there is no `clear_key` here on purpose: a copy that fails costs
/// the user one paste, a delete that shouldn't have happened costs the key).
fn copy_legacy_keys() {
    let all = connections_for_keys();
    if all.is_empty() {
        return;
    }
    let copied = copy_keys(&all, |c| c.has_key(&AI_APP), |c| c.key(&AI_LEGACY_APP), |c, key| c.set_key(&AI_APP, key));
    if copied > 0 {
        log::info!("copied {copied} API key(s) from the Local Mind keychain entry to the Local Mindmap one");
    }
}

/// The saved connections, read under the new name; when that store is empty (the folder could not
/// be moved, or nothing was ever saved) the old one is read instead, so its keys are copied all the
/// same.
fn connections_for_keys() -> Vec<Connection> {
    match Connection::load_all(&AI_APP) {
        Ok(all) if !all.is_empty() => all,
        Ok(_) => load_legacy_connections(),
        Err(e) => {
            log::warn!("legacy AI keys: the connections under `{AI_APP_ID}` can't be read ({e}); trying the old folder");
            load_legacy_connections()
        }
    }
}

/// The connections still filed under the old app id.
fn load_legacy_connections() -> Vec<Connection> {
    match Connection::load_all(&AI_LEGACY_APP) {
        Ok(all) => all,
        Err(e) => {
            log::warn!("legacy AI keys: the old connections can't be read ({e}); no key is copied");
            Vec::new()
        }
    }
}

/// [`copy_legacy_keys`] with the keychain behind three closures (so it can be tested without one).
fn copy_keys(
    all: &[Connection],
    has_current: impl Fn(&Connection) -> bool,
    legacy_key: impl Fn(&Connection) -> Result<String, String>,
    set_current: impl Fn(&Connection, &str) -> Result<(), String>,
) -> usize {
    let mut copied = 0;
    for connection in all {
        if has_current(connection) {
            continue;
        }
        let key = match legacy_key(connection) {
            Ok(key) => key,
            Err(e) => {
                log::warn!("legacy AI keys: the key of `{}` can't be read ({e})", connection.id);
                continue;
            }
        };
        if key.is_empty() {
            continue;
        }
        match set_current(connection, &key) {
            Ok(()) => {
                log::info!("copied the API key of connection `{}` to the Local Mindmap keychain entry", connection.id);
                copied += 1;
            }
            Err(e) => log::warn!("couldn't copy the API key of `{}`: {e}", connection.id),
        }
    }
    copied
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp(tag: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!("localmindmap-legacy-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("temp dir");
        d
    }

    fn old_folder(root: &Path, name: &str) -> PathBuf {
        let old = root.join(name);
        std::fs::create_dir_all(old.join("logs")).expect("old folder");
        std::fs::write(old.join("prefs.json"), b"{\"theme\":\"dark\"}").expect("prefs");
        std::fs::write(old.join("logs").join("update.log"), b"log").expect("log");
        old
    }

    fn connection(id: &str) -> Connection {
        Connection { id: id.to_owned(), provider: "openai".to_owned(), label: "OpenAI".to_owned(), base_url: String::new(), model: "gpt".to_owned() }
    }

    #[test]
    fn the_new_variable_name_comes_first() {
        assert_eq!(candidates("QA_RENDER"), ["LOCALMINDMAP_QA_RENDER", "LOCALMIND_QA_RENDER"]);
    }

    #[test]
    fn the_new_variable_wins_and_the_old_one_is_only_a_fallback() {
        // Both set: the current name is read.
        let both = |name: &str| Some(OsString::from(name));
        assert_eq!(first_set(candidates("QA_RENDER"), both).as_deref(), Some(OsStr::new("LOCALMINDMAP_QA_RENDER")));
        // Only the old one set: it is read, as before the rename.
        let old_only = |name: &str| (name == "LOCALMIND_QA_RENDER").then(|| OsString::from("1"));
        assert_eq!(first_set(candidates("QA_RENDER"), old_only).as_deref(), Some(OsStr::new("1")));
        // Neither: unset, not an error.
        assert_eq!(first_set(candidates("QA_RENDER"), |_| None), None);
    }

    #[test]
    fn every_folder_convention_is_paired_with_the_name_the_app_uses_today() {
        assert_eq!(NAMES[0].0, APP.name, "the preferences folder is the display name");
        assert_eq!(NAMES[1].0, APP.id, "the updater's Linux folder is the app id");
        assert_eq!(NAMES[2].0, AI_APP_ID, "the AI connections folder is the AI app id");
        assert_eq!(NAMES[3].0, format!("{}Data", APP.name.replace(' ', "")), "the portable folder is the display name without spaces");
        // The old side is the product's previous name, spelled as it shipped.
        assert_eq!(NAMES[0].1, "Local Mind");
        assert_eq!(NAMES[1].1, "localmind");
        assert_eq!(NAMES[2].1, "LocalMind");
        assert_eq!(NAMES[3].1, "LocalMindData");
    }

    #[test]
    fn the_old_name_of_each_folder_convention() {
        for (new, old) in NAMES {
            assert_eq!(old_dir_of(Path::new("/a").join(new).as_path()), Some(PathBuf::from("/a").join(old)));
        }
        assert_eq!(old_dir_of(Path::new("/a/Other")), None);
    }

    /// Every folder the app reads or writes today is named the way the table expects. A folder that
    /// drifts (a path built somewhere else, a rename in the updater) stops being migrated silently,
    /// which is exactly the loss this module exists to prevent.
    #[test]
    fn every_folder_the_app_uses_is_in_the_table() {
        let dirs = current_dirs();
        assert!(dirs.len() >= 4, "the app's folders are: {dirs:?}");
        for dir in dirs {
            let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            assert!(NAMES.iter().any(|(new, _)| *new == name), "{} is not in NAMES: the migration would never see it", dir.display());
        }
    }

    #[test]
    fn an_old_folder_is_moved_to_the_new_name_with_its_contents() {
        let root = temp("moved");
        let old = old_folder(&root, "Local Mind");
        let new = root.join("Local Mindmap");
        assert_eq!(migrate_from_old_name(&new), Outcome::Moved);
        assert!(!old.exists());
        assert_eq!(std::fs::read(new.join("prefs.json")).expect("prefs"), b"{\"theme\":\"dark\"}");
        assert_eq!(std::fs::read(new.join("logs/update.log")).expect("log"), b"log");
        // One time: a second start finds the new folder and does nothing.
        assert_eq!(migrate_from_old_name(&new), Outcome::Nothing);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn every_name_convention_migrates() {
        for (n, (new, old)) in NAMES.into_iter().enumerate() {
            // A root of its own each time: Windows and macOS folders ignore case, so two names that
            // differ only in case would be the same folder.
            let root = temp(&format!("all{n}"));
            old_folder(&root, old);
            assert_eq!(migrate_from_old_name(&root.join(new)), Outcome::Moved, "{old} -> {new}");
            assert!(root.join(new).join("prefs.json").is_file(), "{new}");
            assert!(!root.join(old).exists(), "{old}");
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn an_existing_new_folder_is_never_overwritten() {
        let root = temp("exists");
        let old = old_folder(&root, "Local Mind");
        let new = root.join("Local Mindmap");
        std::fs::create_dir_all(&new).expect("new folder");
        std::fs::write(new.join("prefs.json"), b"new").expect("prefs");
        assert_eq!(migrate_dir(&old, &new), Outcome::Nothing);
        assert_eq!(std::fs::read(new.join("prefs.json")).expect("prefs"), b"new");
        assert!(old.join("prefs.json").is_file(), "the old folder stays");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_old_folder_is_not_an_error_and_creates_nothing() {
        let root = temp("none");
        for (new, _) in NAMES {
            assert_eq!(migrate_from_old_name(&root.join(new)), Outcome::Nothing);
            assert!(!root.join(new).exists());
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_plain_file_with_the_old_name_is_not_a_folder_to_move() {
        let root = temp("file");
        std::fs::write(root.join("Local Mind"), b"x").expect("file");
        assert_eq!(migrate_from_old_name(&root.join("Local Mindmap")), Outcome::Nothing);
        assert!(!root.join("Local Mindmap").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_copy_path_leaves_the_old_folder_and_a_complete_new_one() {
        let root = temp("copy");
        let old = old_folder(&root, "LocalMindData");
        let new = root.join("LocalMindmapData");
        // The fallback taken when a rename isn't possible (another volume, a file held open).
        let stage = root.join("stage");
        copy_tree(&old, &stage, 0).expect("copy");
        std::fs::rename(&stage, &new).expect("rename into place");
        assert_eq!(std::fs::read(new.join("logs/update.log")).expect("log"), b"log");
        assert!(old.join("prefs.json").is_file(), "the copy keeps the original");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_nested_too_deep_tree_is_refused_not_followed() {
        let root = temp("deep");
        let mut deep = root.join("src");
        for _ in 0..(MAX_DEPTH + 3) {
            deep = deep.join("d");
        }
        std::fs::create_dir_all(&deep).expect("deep");
        assert!(copy_tree(&root.join("src"), &root.join("dst"), 0).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_key_is_copied_to_the_current_service_and_the_old_entry_stays() {
        let all = [connection("localmind")];
        let current = std::cell::RefCell::new(Vec::<String>::new());
        // The old side is only ever read: `copy_keys` has no way to delete an entry.
        let copied = copy_keys(
            &all,
            |_| false,
            |_| Ok("sk-old".to_owned()),
            |c, key| {
                current.borrow_mut().push(format!("{}={key}", c.id));
                Ok(())
            },
        );
        assert_eq!(copied, 1);
        assert_eq!(current.borrow().as_slice(), ["localmind=sk-old"]);
    }

    #[test]
    fn a_key_under_the_current_service_is_never_replaced() {
        let all = [connection("localmindmap")];
        let current = std::cell::RefCell::new(Vec::<String>::new());
        let copied = copy_keys(
            &all,
            |_| true,
            |_| Ok("sk-old".to_owned()),
            |c, key| {
                current.borrow_mut().push(format!("{}={key}", c.id));
                Ok(())
            },
        );
        assert_eq!(copied, 0);
        assert!(current.borrow().is_empty(), "a key already filed under the new service wins");
    }

    #[test]
    fn a_connection_without_an_old_key_copies_nothing() {
        let all = [connection("localmind")];
        let copied = copy_keys(&all, |_| false, |_| Ok(String::new()), |_, _| panic!("nothing to copy"));
        assert_eq!(copied, 0);
    }

    #[test]
    fn a_keychain_that_cannot_be_read_is_skipped_not_fatal() {
        let all = [connection("a"), connection("b")];
        let current = std::cell::RefCell::new(Vec::<String>::new());
        // `a` cannot be read; `b` can, and is copied: one broken entry does not stop the rest.
        let copied = copy_keys(
            &all,
            |_| false,
            |c| if c.id == "a" { Err("keychain locked".to_owned()) } else { Ok("sk-b".to_owned()) },
            |c, key| {
                current.borrow_mut().push(format!("{}={key}", c.id));
                Ok(())
            },
        );
        assert_eq!(copied, 1);
        assert_eq!(current.borrow().as_slice(), ["b=sk-b"]);
    }

    #[test]
    fn a_write_that_fails_is_counted_as_nothing_and_is_not_fatal() {
        let all = [connection("localmind")];
        let copied = copy_keys(&all, |_| false, |_| Ok("sk".to_owned()), |_, _| Err("locked".to_owned()));
        assert_eq!(copied, 0);
    }
}
