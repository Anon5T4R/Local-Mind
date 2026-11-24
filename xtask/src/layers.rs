//! Dependency layering rules.
//!
//! The suite grows every app as layers: `localmindmap-<name>` crates only ever depend downwards, so
//! the engine can be tested without the UI and no lower layer drags egui in. The rule engine works
//! on a small, metadata-independent model so it can be unit-tested; `from_metadata` builds that
//! model from `cargo metadata`.
//!
//! A born app fills [`TABLE`] and [`INTRA_LAYER_ORDER`] with its own crates as it adds them. With
//! only the binary, the table holds just the app and the tooling, and the gate passes trivially.

use serde_json::Value;

/// Where a workspace crate sits.
// A born app registers its crates in [`TABLE`] and every class becomes reachable; with only the
// binary, the table holds one `Exempt` entry each and the rest is dead code.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Regular layered crate.
    Layer(u8),
    /// Layer 0, and additionally may depend on no workspace crate at all.
    Standalone,
    /// Test tooling: may depend on anything up to L5 (it sits at L6 for rule purposes); other
    /// crates may use it only as a dev-dependency.
    Testkit,
    /// Binaries and build tooling: exempt from the rules.
    Exempt,
}

impl Class {
    fn layer(self) -> Option<u8> {
        match self {
            Class::Layer(l) => Some(l),
            Class::Standalone => Some(0),
            Class::Testkit => Some(5),
            Class::Exempt => None,
        }
    }
}

/// The layering table. Names are package names without the `localmindmap-` prefix.
pub const TABLE: &[(&str, Class)] = &[
    // L6 apps and tooling
    ("localmindmap", Class::Exempt),
    ("xtask", Class::Exempt),
    // The map itself: the nodes, the tree their parent links form, the edits and the undo history.
    // Depends on no workspace crate at all (serde only), so its rules are tested without a UI.
    ("model", Class::Standalone),
    // Where every card sits: the port of the original app's balanced four-way layout.
    ("layout", Class::Layer(2)),
    // Reading and writing the user's .tmind files, atomically.
    ("io", Class::Layer(2)),
    // The remote AI: prompts, the tolerant JSON extractor, the actions and the chat. Sits above the
    // model (it builds nodes) and reaches the network only through the shared `local-ai` client.
    ("ai", Class::Layer(3)),
];

/// Explicit orderings *within* a layer (earlier may be used by later).
///
/// `io` and `layout` share L2 (both sit above the model, neither knows about the other's insides).
/// The compatibility suite reads a real map AND lays it out, so `io`'s dev-dependency on `layout` is
/// declared here instead of inventing a layer for it.
pub const INTRA_LAYER_ORDER: &[&[&str]] = &[&["layout", "io"]];

/// The package-name prefix of the app's own crates. A born app renames it with the rest of the
/// template (`scripts/nascer-app.ps1`).
pub const PREFIX: &str = "localmindmap-";

fn intra_layer_allowed(from: &str, to: &str) -> bool {
    let (from, to) = (short_name(from), short_name(to));
    INTRA_LAYER_ORDER.iter().any(|chain| match (chain.iter().position(|n| *n == from), chain.iter().position(|n| *n == to)) {
        (Some(f), Some(t)) => t < f,
        _ => false,
    })
}

/// External crates that constitute a UI toolkit / windowing dependency. Entries ending in `*` are
/// prefixes.
pub const UI_CRATES: &[&str] = &["craft-ui", "egui", "eframe", "winit", "egui_kittest", "rfd", "bevy*"];

/// First layer allowed to use UI crates.
pub const UI_MIN_LAYER: u8 = 5;

pub fn short_name(pkg: &str) -> &str {
    pkg.strip_prefix(PREFIX).unwrap_or(pkg)
}

pub fn classify(pkg: &str) -> Option<Class> {
    classify_in(TABLE, pkg)
}

/// [`classify`] against an explicit table, so a test can extend it the way a born app does.
pub fn classify_in(table: &[(&'static str, Class)], pkg: &str) -> Option<Class> {
    let s = short_name(pkg);
    table.iter().find(|(n, _)| *n == s).map(|(_, c)| *c)
}

fn is_ui_crate(name: &str) -> bool {
    UI_CRATES.iter().any(|p| match p.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == *p,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    Normal,
    Dev,
    Build,
}

#[derive(Debug, Clone)]
pub struct Dep {
    pub name: String,
    pub kind: DepKind,
    /// `true` if the dependency is a workspace member.
    pub workspace: bool,
}

#[derive(Debug, Clone)]
pub struct Crate {
    pub name: String,
    pub deps: Vec<Dep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    Unregistered { krate: String },
    Upward { krate: String, dep: String, from: u8, to: u8, kind: DepKind },
    StandaloneHasWorkspaceDep { krate: String, dep: String },
    TestkitAsNormalDep { krate: String },
    UiBelowL5 { krate: String, dep: String, layer: u8 },
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Violation::Unregistered { krate } => {
                write!(f, "{krate}: unknown workspace crate; register it in xtask/src/layers.rs TABLE")
            }
            Violation::Upward { krate, dep, from, to, kind } => {
                write!(f, "{krate} (L{from}) -> {dep} (L{to}) [{kind:?}]: may only depend on strictly lower layers")
            }
            Violation::StandaloneHasWorkspaceDep { krate, dep } => {
                write!(f, "{krate}: standalone crate must not depend on workspace crate {dep}")
            }
            Violation::TestkitAsNormalDep { krate } => {
                write!(f, "{krate}: test tooling may only be a dev-dependency")
            }
            Violation::UiBelowL5 { krate, dep, layer } => {
                write!(f, "{krate} (L{layer}) depends on UI crate `{dep}`; UI toolkits are only allowed in L5+")
            }
        }
    }
}

/// Check all rules. Returns violations sorted for stable output.
pub fn check(crates: &[Crate]) -> Vec<Violation> {
    check_with(crates, TABLE)
}

/// [`check`] against an explicit table (a born app's table, in a test).
pub fn check_with(crates: &[Crate], table: &[(&'static str, Class)]) -> Vec<Violation> {
    let mut out = Vec::new();
    for c in crates {
        let Some(class) = classify_in(table, &c.name) else {
            out.push(Violation::Unregistered { krate: c.name.clone() });
            continue;
        };
        if class == Class::Exempt {
            continue;
        }
        let layer = class.layer().unwrap_or(0);
        for d in &c.deps {
            // Self dev-dependencies (e.g. to enable features in tests) are fine.
            if d.name == c.name {
                continue;
            }
            if d.workspace {
                if class == Class::Standalone {
                    out.push(Violation::StandaloneHasWorkspaceDep { krate: c.name.clone(), dep: d.name.clone() });
                    continue;
                }
                match classify_in(table, &d.name) {
                    // Unregistered deps are reported on their own entry.
                    None => {}
                    Some(Class::Testkit) if d.kind != DepKind::Dev => out.push(Violation::TestkitAsNormalDep { krate: c.name.clone() }),
                    Some(Class::Testkit) => {}
                    Some(dc) => {
                        let to = dc.layer().unwrap_or(u8::MAX);
                        if to >= layer && !(to == layer && intra_layer_allowed(&c.name, &d.name)) {
                            out.push(Violation::Upward {
                                krate: c.name.clone(),
                                dep: d.name.clone(),
                                from: layer,
                                to: if to == u8::MAX { 7 } else { to },
                                kind: d.kind,
                            });
                        }
                    }
                }
            } else if layer < UI_MIN_LAYER && is_ui_crate(&d.name) {
                out.push(Violation::UiBelowL5 { krate: c.name.clone(), dep: d.name.clone(), layer });
            }
        }
    }
    out.sort_by_key(|v| v.to_string());
    out.dedup();
    out
}

/// Build the model from `cargo metadata --format-version 1 --no-deps`.
pub fn from_metadata(meta: &Value) -> Result<Vec<Crate>, String> {
    let pkgs = meta["packages"].as_array().ok_or("metadata: no packages array")?;
    let members: Vec<&str> = pkgs.iter().filter_map(|p| p["name"].as_str()).collect();
    let mut out = Vec::new();
    for p in pkgs {
        let name = p["name"].as_str().ok_or("package without name")?.to_owned();
        let mut deps = Vec::new();
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let dname = d["name"].as_str().unwrap_or_default().to_owned();
            let kind = match d["kind"].as_str() {
                Some("dev") => DepKind::Dev,
                Some("build") => DepKind::Build,
                _ => DepKind::Normal,
            };
            let workspace = members.contains(&dname.as_str()) || d["path"].is_string();
            deps.push(Dep { name: dname, kind, workspace });
        }
        out.push(Crate { name, deps });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn describe(class: Option<Class>) -> String {
    match class {
        Some(Class::Layer(l)) => format!("L{l}"),
        Some(Class::Standalone) => "L0 standalone".into(),
        Some(Class::Testkit) => "testkit".into(),
        Some(Class::Exempt) => "exempt".into(),
        None => "UNREGISTERED".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DepKind::*;

    fn c(name: &str, deps: &[(&str, DepKind, bool)]) -> Crate {
        Crate { name: name.into(), deps: deps.iter().map(|(n, k, w)| Dep { name: (*n).into(), kind: *k, workspace: *w }).collect() }
    }

    /// The table this app ships with: the app, the tooling and the three layers it uses.
    #[test]
    fn the_shipped_table_is_clean() {
        let g = [c("localmindmap", &[("egui", Normal, false), ("craft-update", Normal, false)]), c("xtask", &[("serde_json", Normal, false)])];
        assert!(check(&g).is_empty(), "{:?}", check(&g));
    }

    /// The table this app actually ships: the three layers and the one same-layer edge (the
    /// compatibility suite lays a real map out, so `io`'s dev-dependency on `layout` is declared in
    /// [`INTRA_LAYER_ORDER`]).
    #[test]
    fn the_apps_own_layers_pass_the_rules() {
        let g = [
            c("localmindmap-model", &[("serde", Normal, false)]),
            c("localmindmap-layout", &[("localmindmap-model", Normal, true)]),
            c("localmindmap-io", &[("localmindmap-model", Normal, true), ("localmindmap-layout", Dev, true)]),
            c("localmindmap", &[("localmindmap-io", Normal, true), ("localmindmap-layout", Normal, true), ("localmindmap-model", Normal, true)]),
        ];
        assert!(check(&g).is_empty(), "{:?}", check(&g));

        // The order is declared for the pair, not for the dependency kind: the same edge is legal
        // as a normal dependency too, so the table says what the layers mean, not what cargo does.
        let normal = [c("localmindmap-io", &[("localmindmap-layout", Normal, true)])];
        assert!(check(&normal).is_empty(), "{:?}", check(&normal));
    }

    #[test]
    fn an_unregistered_workspace_crate_is_flagged() {
        let v = check(&[c("localmindmap-mystery", &[])]);
        assert!(matches!(&v[..], [Violation::Unregistered { krate }] if krate == "localmindmap-mystery"));
    }

    /// The rules a born app relies on, with its own crates registered.
    #[test]
    fn upward_ui_below_l5_and_testkit_rules() {
        let table: &[(&'static str, Class)] = &[
            ("geom", Class::Layer(0)),
            ("engine", Class::Layer(3)),
            ("ui", Class::Layer(5)),
            ("testkit", Class::Testkit),
            ("localmindmap", Class::Exempt),
            ("xtask", Class::Exempt),
        ];
        let g = [
            c("localmindmap-geom", &[]),
            c("localmindmap-engine", &[("localmindmap-geom", Normal, true)]),
            c("localmindmap-ui", &[("localmindmap-engine", Normal, true), ("egui", Normal, false)]),
            c("localmindmap", &[("localmindmap-ui", Normal, true), ("localmindmap-testkit", Dev, true)]),
        ];
        assert!(check_with(&g, table).is_empty(), "{:?}", check_with(&g, table));

        // engine -> ui goes up
        let up = check_with(&[c("localmindmap-engine", &[("localmindmap-ui", Normal, true)])], table);
        assert!(matches!(up[..], [Violation::Upward { from: 3, to: 5, .. }]), "{up:?}");

        // a UI toolkit below L5
        let ui = check_with(&[c("localmindmap-engine", &[("egui", Normal, false)])], table);
        assert!(matches!(ui[..], [Violation::UiBelowL5 { layer: 3, .. }]), "{ui:?}");

        // test tooling as a normal dependency
        let tk = check_with(&[c("localmindmap-engine", &[("localmindmap-testkit", Normal, true)])], table);
        assert!(matches!(tk[..], [Violation::TestkitAsNormalDep { .. }]), "{tk:?}");
    }
}
