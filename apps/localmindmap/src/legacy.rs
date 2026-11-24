//! What the rename from Local Mind to Local Mindmap left behind.
//!
//! The product was called Local Mind, and its environment variables with it. [`env_var_os`] reads
//! `LOCALMINDMAP_<NAME>` and falls back to the old `LOCALMIND_<NAME>`, so a user who set the old
//! variable keeps the behaviour. Nothing here touches the user's files.

use std::ffi::OsString;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

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
}
