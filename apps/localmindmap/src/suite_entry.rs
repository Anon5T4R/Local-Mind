//! The app's entry in the Craft App Suite, as `craft-update` knows apps.
//!
//! The catalog is the single source `Styx-Studios/local-libs` (`crates/update/src/suite.rs`) and
//! reaches this repository through `scripts/sync.ps1` as a mirror under
//! `libs/craft-libs/crates/update`: the copy in `libs/` is never edited here.

use craft_update::App;

/// This app, as its releases and installers know it.
pub const APP: App = craft_update::suite::LOCALMINDMAP;
