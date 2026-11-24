//! What the rename from Local Mind to Local Mindmap left behind.
//!
//! The product was called Local Mind, and its names went with it: the per-user folders, the
//! keychain service and the environment variables. This module does two things, both quietly and
//! both safe to run on every start:
//!
//! * [`migrate_all`] moves each old-named folder (preferences, the AI connections, the updater's
//!   data and cache, the portable data folder) to its new name, once. It never overwrites: when the
//!   new folder exists the old one is left alone, and when the move has to be a copy (another
//!   volume, a file still open) the old folder stays as it was. A failure is logged and printed
//!   ([`report`]), because the app keeps no log file and a silent `warn` would reach nobody. The
//!   API keys in the OS keychain are copied to the new service name **once**, marked with a file,
//!   and the old entries are kept.
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

/// The file in the preferences folder that says the keychain keys were copied already.
const KEYS_MARKER: &str = "legacy-keys-copied";

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

/// A failure the user may have to act on: logged (for a build that installs a logger) and printed,
/// the way `main.rs` reports its own errors. Without the print, a folder that could not be moved
/// would leave the user with the defaults and no word about it.
fn report(message: std::fmt::Arguments<'_>) {
    log::warn!("{message}");
    eprintln!("{}: {message}", APP.id);
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
    old_dir_of(new).map_or(Outcome::Nothing, |old| {
        // An interrupted copy left its staging folder beside the destination: it goes first, or a
        // rename that succeeds would keep it around forever (the copy path's cleanup never runs).
        clear_stale_stages(new);
        migrate_dir(&old, new)
    })
}

/// Move `old` to `new`, once: only when `old` is a folder and `new` doesn't exist. A rename on one
/// volume; otherwise the contents are copied to a staging folder beside `new` that is renamed into
/// place when complete, so `new` never exists half-copied. Nothing at `new` is ever replaced.
pub fn migrate_dir(old: &Path, new: &Path) -> Outcome {
    migrate_dir_with(old, new, rename_path)
}

/// The real rename, as a plain function: [`std::fs::rename`] is generic over `AsRef<Path>` and does
/// not satisfy the higher-ranked closure the copy path is tested with.
fn rename_path(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}

/// [`migrate_dir`] with the rename passed in, so both the copy path (another volume, a file held
/// open) and the race that ends in [`Outcome::Nothing`] can be tested.
fn migrate_dir_with(old: &Path, new: &Path, rename: impl Fn(&Path, &Path) -> std::io::Result<()>) -> Outcome {
    if !old.is_dir() || std::fs::symlink_metadata(new).is_ok() {
        return Outcome::Nothing;
    }
    if let Some(parent) = new.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        report(format_args!("couldn't move {} to {}: {e}", old.display(), new.display()));
        return Outcome::Failed(e.to_string());
    }
    if rename(old, new).is_ok() {
        log::info!("moved the Local Mind folder {} to {}", old.display(), new.display());
        return Outcome::Moved;
    }
    // `migrate_from_old_name` sweeps a staging folder left by an interrupted copy before it tries
    // the rename; a direct call to this function gets the same before the copy starts.
    clear_stale_stages(new);
    let stage = stage_of(new);
    let copied = copy_tree(old, &stage, 0).and_then(|()| {
        if std::fs::symlink_metadata(new).is_ok() {
            Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "the new folder appeared meanwhile"))
        } else {
            rename(&stage, new)
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
            report(format_args!("couldn't copy {} to {}: {e}; the old settings stay where they are", old.display(), new.display()));
            Outcome::Failed(e.to_string())
        }
    }
}

/// Where a copy lands before being renamed into place: beside `new`, named after it and this
/// process, so two instances never write the same staging folder.
fn stage_of(new: &Path) -> PathBuf {
    let name = new.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    new.with_file_name(format!(".{name}.migrating-{}", std::process::id()))
}

/// Removes the staging folders an interrupted copy left beside `new`: same parent, same name, the
/// `.{name}.migrating-<pid>` shape [`stage_of`] writes, and nothing else — a file, a folder with a
/// name that only looks similar, or the staging of another destination is left alone. Another
/// instance copying this same folder right now would be interrupted; its copy is logged as failed
/// and tried again next start.
fn clear_stale_stages(new: &Path) {
    let (Some(parent), Some(name)) = (new.parent(), new.file_name().and_then(|n| n.to_str())) else { return };
    let prefix = format!(".{name}.migrating-");
    let Ok(entries) = std::fs::read_dir(parent) else { return };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else { continue };
        let Some(pid) = file_name.strip_prefix(&prefix) else { continue };
        if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            let _ = std::fs::remove_dir_all(entry.path());
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
            // A plain copy takes the contents and the default mode with them. On Unix the mode
            // carries the executable bit the updater's tarball installs need (`<data>/apps/…`), so
            // it is copied over; on Windows the read-only bit lives in the attributes and a new
            // file inherits its folder's permissions, so there is nothing to carry.
            #[cfg(unix)]
            if let Ok(meta) = entry.metadata()
                && let Err(e) = std::fs::set_permissions(&target, meta.permissions())
            {
                // The contents are there already: a mode that could not be copied is logged, not fatal.
                log::warn!("copied {} but not its permissions: {e}", target.display());
            }
        }
    }
    Ok(())
}

/// The folders this version reads and writes beside the executable: the updater's records, logs and
/// downloads (under the portable root when `portable.txt` is there) and the portable root itself.
fn dirs_beside(exe: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(paths) = craft_update::Paths::for_exe(&APP, exe) {
        dirs.push(paths.data);
        dirs.push(paths.cache);
    }
    if let Some(exe_dir) = exe.parent() {
        dirs.push(exe_dir.join(format!("{}Data", APP.name.replace(' ', ""))));
    }
    // On Windows the updater's data and cache folders are the same one.
    dirs.dedup();
    dirs
}

/// The folders this version reads and writes, asked of whoever uses them (the preferences file, the
/// AI connections store, the updater and the portable root), which an old-named twin may sit beside.
fn current_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = crate::prefs::path().and_then(|file| file.parent().map(Path::to_path_buf)) {
        dirs.push(dir);
    }
    if let Some(dir) = Connection::store_path(&AI_APP).and_then(|file| file.parent().map(Path::to_path_buf)) {
        dirs.push(dir);
    }
    if let Ok(exe) = std::env::current_exe() {
        dirs.extend(dirs_beside(&exe));
    }
    dirs
}

/// Move every old-named folder to its new name, then copy the keychain keys. Called before anything
/// reads those folders; never fails (what can't be moved is reported and tried again next start).
pub fn migrate_all() {
    for new in current_dirs() {
        migrate_from_old_name(&new);
    }
    // The connections file moved with its folder; the keys live in the keychain, under the old
    // service name.
    copy_legacy_keys();
}

/// `<preferences folder>/legacy-keys-copied`.
fn keys_marker() -> Option<PathBuf> {
    crate::prefs::path().map(|prefs| prefs.with_file_name(KEYS_MARKER))
}

/// Copies the API keys saved under the old keychain service to the current one, once. The old
/// entries are never deleted (there is no `clear_key` here on purpose: a copy that fails costs the
/// user one paste, a delete that shouldn't have happened costs the key).
fn copy_legacy_keys() {
    let Some(marker) = keys_marker() else { return };
    // The one-shot rule lives in `copy_keys_once`; this is only so a finished copy stops opening
    // the connections file on every start.
    if marker.is_file() {
        return;
    }
    let all = connections_for_keys();
    let copied = copy_keys_once(&marker, &all, |c| c.has_key(&AI_APP), |c| c.key(&AI_LEGACY_APP), |c, key| c.set_key(&AI_APP, key));
    if copied > 0 {
        log::info!("copied {copied} API key(s) from the Local Mind keychain entry to the Local Mindmap one");
    }
}

/// [`copy_legacy_keys`] with the marker, the connections and the keychain passed in (so the one-shot
/// rule can be tested on a temporary folder).
///
/// The marker, not an empty entry, decides whether the copy already happened: the app writes `""`
/// when the user saves a connection that needs no key (a local provider), so an empty entry is a
/// decision, and copying the old key over it would send that provider's key as
/// `Authorization: Bearer` to the new base URL.
///
/// The mark is written on the **first attempt**, whether it worked or not. A key the keychain
/// refused (or a connections list that was empty at that moment) costs the user one paste;
/// retrying later is the dangerous direction — by then the entry may be empty on purpose, and the
/// copy would put the old provider's key back.
fn copy_keys_once(
    marker: &Path,
    all: &[Connection],
    has_current: impl Fn(&Connection) -> bool,
    legacy_key: impl Fn(&Connection) -> Result<String, String>,
    set_current: impl Fn(&Connection, &str) -> Result<(), String>,
) -> usize {
    if marker.is_file() {
        return 0;
    }
    let (copied, failed) = copy_keys(all, has_current, legacy_key, set_current);
    if failed > 0 {
        log::warn!("legacy AI keys: {failed} key(s) couldn't be copied; they have to be pasted again");
    }
    mark_keys_copied(marker);
    copied
}

/// Writes the marker, once and never over an existing one. The marker lives in the preferences
/// folder, so it never creates that folder while the old one is still waiting to be moved: a new
/// folder would make the old one unmovable for good. (The mark is then deferred, and the next start
/// copies the keys again — the only case where it does.)
fn mark_keys_copied(marker: &Path) {
    let Some(parent) = marker.parent() else { return };
    if !parent.is_dir() {
        if old_dir_of(parent).is_some_and(|old| old.is_dir()) {
            log::warn!("legacy AI keys: {} is waiting to be moved, so the keys aren't marked as copied yet", parent.display());
            return;
        }
        if let Err(e) = std::fs::create_dir_all(parent) {
            report(format_args!("legacy AI keys: {} couldn't be created ({e}); the keys aren't marked as copied", parent.display()));
            return;
        }
    }
    match std::fs::OpenOptions::new().write(true).create_new(true).open(marker) {
        Ok(file) => {
            let _ = file.sync_all();
        }
        // Another start won the race: the copy is marked all the same.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => report(format_args!("legacy AI keys: {} couldn't be written ({e}); the keys aren't marked as copied", marker.display())),
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

/// [`copy_keys_once`] with the keychain behind three closures (so it can be tested without one).
/// Returns how many keys were copied and how many could not be read or written.
fn copy_keys(
    all: &[Connection],
    has_current: impl Fn(&Connection) -> bool,
    legacy_key: impl Fn(&Connection) -> Result<String, String>,
    set_current: impl Fn(&Connection, &str) -> Result<(), String>,
) -> (usize, usize) {
    let mut copied = 0;
    let mut failed = 0;
    for connection in all {
        if has_current(connection) {
            continue;
        }
        let key = match legacy_key(connection) {
            Ok(key) => key,
            Err(e) => {
                log::warn!("legacy AI keys: the key of `{}` can't be read ({e})", connection.id);
                failed += 1;
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
            Err(e) => {
                log::warn!("couldn't copy the API key of `{}`: {e}", connection.id);
                failed += 1;
            }
        }
    }
    (copied, failed)
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

    /// A rename that fails the first time (another volume, a file held open) and really renames
    /// after that, so a copy is taken and placed.
    fn cross_volume_once() -> impl Fn(&Path, &Path) -> std::io::Result<()> {
        let calls = std::cell::Cell::new(0);
        move |from: &Path, to: &Path| {
            let n = calls.get() + 1;
            calls.set(n);
            if n == 1 {
                Err(std::io::Error::other(format!("cross-device: {} -> {}", from.display(), to.display())))
            } else {
                std::fs::rename(from, to)
            }
        }
    }

    fn stages(root: &Path, name: &str) -> Vec<String> {
        let prefix = format!(".{name}.migrating-");
        std::fs::read_dir(root)
            .expect("dir")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(&prefix))
            .collect()
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
    /// which is exactly the loss this module exists to prevent, so the list comes from the same
    /// function the migration reads.
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
    fn a_portable_install_migrates_the_folder_beside_the_executable() {
        let root = temp("portable");
        std::fs::write(root.join("portable.txt"), b"").expect("portable");
        let dirs = dirs_beside(&root.join("localmindmap.exe"));
        let portable = root.join("LocalMindmapData");
        assert!(dirs.contains(&portable), "the portable root is not visited: {dirs:?}");
        assert!(dirs.contains(&portable.join("data")), "the updater's portable data folder is not visited: {dirs:?}");
        // The old portable folder beside the executable is the one that moves, with what it holds.
        let old = root.join("LocalMindData");
        std::fs::create_dir_all(old.join("logs")).expect("old");
        std::fs::write(old.join("logs").join("update.log"), b"log").expect("log");
        assert_eq!(migrate_from_old_name(&portable), Outcome::Moved);
        assert_eq!(std::fs::read(portable.join("logs/update.log")).expect("log"), b"log");
        assert!(!old.exists());
        let _ = std::fs::remove_dir_all(&root);
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
        let old = old_folder(&root, "Local Mind");
        let new = root.join("Local Mindmap");
        assert_eq!(migrate_dir_with(&old, &new, cross_volume_once()), Outcome::Copied);
        assert_eq!(std::fs::read(new.join("prefs.json")).expect("prefs"), b"{\"theme\":\"dark\"}");
        assert_eq!(std::fs::read(new.join("logs/update.log")).expect("log"), b"log");
        assert!(old.join("prefs.json").is_file(), "the copy keeps the original");
        assert!(stages(&root, "Local Mindmap").is_empty(), "a staging folder stayed behind: {:?}", stages(&root, "Local Mindmap"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_folder_that_appears_during_the_copy_is_left_as_it_is() {
        let root = temp("raced");
        let old = old_folder(&root, "Local Mind");
        let new = root.join("Local Mindmap");
        let calls = std::cell::Cell::new(0);
        let rename = |_from: &Path, to: &Path| {
            let n = calls.get() + 1;
            calls.set(n);
            if n == 1 {
                return Err(std::io::Error::other("cross-device"));
            }
            // Another instance placed the folder while this one was copying.
            std::fs::create_dir_all(to)?;
            std::fs::write(to.join("theirs.json"), b"x")?;
            Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "the new folder appeared meanwhile"))
        };
        assert_eq!(migrate_dir_with(&old, &new, rename), Outcome::Nothing);
        assert!(new.join("theirs.json").is_file(), "the other instance's folder is kept as it is");
        assert!(old.join("prefs.json").is_file(), "the old folder stays");
        assert!(stages(&root, "Local Mindmap").is_empty(), "the staging folder stayed behind");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_interrupted_copy_leaves_no_staging_folder_behind_the_next_time() {
        let root = temp("stale");
        let old = old_folder(&root, "Local Mind");
        let new = root.join("Local Mindmap");
        // What a killed copy left: this destination's own staging, the staging of another
        // destination, and a name that only looks like a staging folder.
        let stale = root.join(".Local Mindmap.migrating-4242");
        std::fs::create_dir_all(stale.join("half")).expect("stale");
        let other = root.join(".Local Mind.migrating-4242");
        std::fs::create_dir_all(&other).expect("other");
        let lookalike = root.join(".Local Mindmap.migrating-not-a-pid");
        std::fs::create_dir_all(&lookalike).expect("lookalike");
        assert_eq!(migrate_dir_with(&old, &new, cross_volume_once()), Outcome::Copied);
        assert!(!stale.exists(), "the interrupted copy's staging folder is gone");
        assert!(other.is_dir(), "another destination's staging folder is left alone");
        assert!(lookalike.is_dir(), "a name that only looks like a staging folder is left alone");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The normal path: the copy of a previous start died, and this start's rename succeeds at
    /// once. The staging folder must not survive it (the copy path's cleanup never runs then).
    #[test]
    fn a_rename_that_succeeds_still_sweeps_the_staging_of_an_interrupted_copy() {
        let root = temp("stale-moved");
        old_folder(&root, "Local Mind");
        let new = root.join("Local Mindmap");
        let stale = root.join(".Local Mindmap.migrating-4242");
        std::fs::create_dir_all(stale.join("half")).expect("stale");
        let other = root.join(".Local Mind.migrating-4242");
        std::fs::create_dir_all(&other).expect("other");
        assert_eq!(migrate_from_old_name(&new), Outcome::Moved);
        assert!(!stale.exists(), "the interrupted copy's staging folder is gone");
        assert!(other.is_dir(), "another destination's staging folder is left alone");
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

    #[cfg(unix)]
    #[test]
    fn a_copy_keeps_the_mode_of_the_files() {
        use std::os::unix::fs::PermissionsExt;
        let root = temp("mode");
        let old = old_folder(&root, "Local Mind");
        let tool = old.join("apps").join("localmindmap");
        std::fs::create_dir_all(old.join("apps")).expect("apps");
        std::fs::write(&tool, b"#!/bin/sh\n").expect("tool");
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        copy_tree(&old, &root.join("dst"), 0).expect("copy");
        let copied = root.join("dst").join("apps").join("localmindmap");
        assert_eq!(std::fs::metadata(&copied).expect("meta").permissions().mode() & 0o111, 0o111, "the executable bit survives");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_key_is_copied_to_the_current_service_and_the_old_entry_stays() {
        let all = [connection("localmind")];
        let current = std::cell::RefCell::new(Vec::<String>::new());
        // The old side is only ever read: `copy_keys` has no way to delete an entry.
        let (copied, failed) = copy_keys(
            &all,
            |_| false,
            |_| Ok("sk-old".to_owned()),
            |c, key| {
                current.borrow_mut().push(format!("{}={key}", c.id));
                Ok(())
            },
        );
        assert_eq!((copied, failed), (1, 0));
        assert_eq!(current.borrow().as_slice(), ["localmind=sk-old"]);
    }

    #[test]
    fn a_key_under_the_current_service_is_never_replaced() {
        let all = [connection("localmindmap")];
        let current = std::cell::RefCell::new(Vec::<String>::new());
        let (copied, failed) = copy_keys(
            &all,
            |_| true,
            |_| Ok("sk-old".to_owned()),
            |c, key| {
                current.borrow_mut().push(format!("{}={key}", c.id));
                Ok(())
            },
        );
        assert_eq!((copied, failed), (0, 0));
        assert!(current.borrow().is_empty(), "a key already filed under the new service wins");
    }

    #[test]
    fn a_connection_without_an_old_key_copies_nothing() {
        let all = [connection("localmind")];
        assert_eq!(copy_keys(&all, |_| false, |_| Ok(String::new()), |_, _| panic!("nothing to copy")), (0, 0));
    }

    #[test]
    fn a_keychain_that_cannot_be_read_is_skipped_not_fatal() {
        let all = [connection("a"), connection("b")];
        let current = std::cell::RefCell::new(Vec::<String>::new());
        // `a` cannot be read; `b` can, and is copied: one broken entry does not stop the rest.
        let (copied, failed) = copy_keys(
            &all,
            |_| false,
            |c| if c.id == "a" { Err("keychain locked".to_owned()) } else { Ok("sk-b".to_owned()) },
            |c, key| {
                current.borrow_mut().push(format!("{}={key}", c.id));
                Ok(())
            },
        );
        assert_eq!((copied, failed), (1, 1));
        assert_eq!(current.borrow().as_slice(), ["b=sk-b"]);
    }

    #[test]
    fn a_write_that_fails_is_counted_as_nothing_and_is_not_fatal() {
        let all = [connection("localmind")];
        assert_eq!(copy_keys(&all, |_| false, |_| Ok("sk".to_owned()), |_, _| Err("locked".to_owned())), (0, 1));
    }

    /// The blocker the review found: the entry in the new service was empty *on purpose* (the user
    /// saved a connection that needs no key), and a second run put the old provider's key back,
    /// which the app would then send as `Authorization: Bearer` to the new base URL.
    #[test]
    fn the_keys_are_copied_once_and_an_entry_emptied_on_purpose_is_not_refilled() {
        let root = temp("once");
        let marker = root.join(KEYS_MARKER);
        let all = [connection("localmind")];
        let entry = std::cell::RefCell::new(None::<String>);
        let has_current = |_: &Connection| entry.borrow().is_some();
        let legacy_key = |_: &Connection| Ok("sk-old".to_owned());
        let set_current = |_: &Connection, key: &str| {
            *entry.borrow_mut() = Some(key.to_owned());
            Ok(())
        };

        assert_eq!(copy_keys_once(&marker, &all, has_current, legacy_key, set_current), 1);
        assert_eq!(entry.borrow().as_deref(), Some("sk-old"));

        // The app saves a connection that needs no key: the entry becomes empty, on purpose.
        *entry.borrow_mut() = None;
        assert_eq!(copy_keys_once(&marker, &all, has_current, legacy_key, set_current), 0);
        assert_eq!(*entry.borrow(), None, "the old key must not come back over an entry emptied on purpose");
        assert!(marker.is_file(), "the copy is marked");
        assert!(std::fs::read(&marker).expect("marker").is_empty(), "the marker is written once, not appended to");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_failed_copy_is_marked_on_the_first_attempt_and_the_second_pass_copies_nothing() {
        let root = temp("retry");
        let marker = root.join(KEYS_MARKER);
        let all = [connection("a")];
        let entry = std::cell::RefCell::new(None::<String>);
        let has_current = |_: &Connection| entry.borrow().is_some();
        let locked = |_: &Connection| Err("keychain locked".to_owned());
        let set_current = |_: &Connection, key: &str| {
            *entry.borrow_mut() = Some(key.to_owned());
            Ok(())
        };
        assert_eq!(copy_keys_once(&marker, &all, has_current, locked, set_current), 0);
        assert!(marker.is_file(), "the first attempt is marked even when it could not read the key");

        // The keychain answers this time, but the attempt is spent: nothing is copied.
        assert_eq!(copy_keys_once(&marker, &all, has_current, |_| Ok("sk".to_owned()), set_current), 0);
        assert_eq!(*entry.borrow(), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The sequence the review found: the first attempt fails (the keychain refuses), the user then
    /// saves a connection that needs no key, and only after that the keychain answers. The old key
    /// must not come back as `Authorization: Bearer` for the new base URL.
    #[test]
    fn a_keychain_that_answers_late_cannot_refill_an_entry_emptied_on_purpose() {
        let root = temp("late");
        let marker = root.join(KEYS_MARKER);
        let all = [connection("localmind")];
        let entry = std::cell::RefCell::new(None::<String>);
        let has_current = |_: &Connection| entry.borrow().is_some();
        let set_current = |_: &Connection, key: &str| {
            *entry.borrow_mut() = Some(key.to_owned());
            Ok(())
        };
        // Run 1: the keychain refuses.
        assert_eq!(copy_keys_once(&marker, &all, has_current, |_| Err("keychain locked".to_owned()), set_current), 0);
        // Run 2: the user saves a connection with no key of its own (a local provider).
        assert_eq!(copy_keys_once(&marker, &all, has_current, |_| Ok("sk-old".to_owned()), set_current), 0);
        assert_eq!(*entry.borrow(), None);
        // Run 3: the keychain would answer now.
        assert_eq!(copy_keys_once(&marker, &all, has_current, |_| Ok("sk-old".to_owned()), set_current), 0);
        assert_eq!(*entry.borrow(), None, "the old key must never be sent to the provider the user set up");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_marker_never_creates_the_folder_whose_old_twin_is_still_waiting_to_move() {
        let root = temp("marker");
        let new = root.join("Local Mindmap");
        let old = root.join("Local Mind");
        std::fs::create_dir_all(&old).expect("old");
        let marker = new.join(KEYS_MARKER);
        mark_keys_copied(&marker);
        assert!(!new.exists(), "creating the preferences folder would make the old one unmovable for good");

        // Nothing pending: the folder and the marker are made.
        std::fs::remove_dir_all(&old).expect("rm");
        mark_keys_copied(&marker);
        assert!(marker.is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_empty_first_attempt_is_marked_too_so_nothing_stays_armed() {
        let root = temp("empty");
        let marker = root.join("Nowhere").join(KEYS_MARKER);
        assert_eq!(copy_keys_once(&marker, &[], |_| false, |_| Ok("sk".to_owned()), |_, _| Ok(())), 0);
        // Nothing to copy is a first attempt too: the connections the user saves later are theirs,
        // and an old key must never appear over them.
        assert!(marker.is_file());
        let _ = std::fs::remove_dir_all(&root);
    }
}
