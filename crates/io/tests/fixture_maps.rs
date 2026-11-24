//! The compatibility suite: the maps a user already has, opened by this build.
//!
//! The fixtures are shaped like the files the Electron LocalMind wrote (`sepse.tmind` has notes,
//! colours, an explicit side, a custom width and font size, a collapsed branch, a node whose parent
//! no longer exists, and one field this build does not know). The point is not that the parser is
//! lenient - it is that opening and saving must give the same map back.

use std::path::{Path, PathBuf};

use localmindmap_io as map_io;
use localmindmap_layout::{ResolvedSide, layout_tree};
use localmindmap_model::Side;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name)
}

#[test]
fn a_map_written_by_the_other_app_opens_with_every_field() {
    let map = map_io::load(&fixture("sepse.tmind")).expect("the fixture opens");
    assert_eq!(map.title, "Protocolo de sepse");
    assert_eq!(map.nodes.len(), 13);
    assert_eq!(map.root_id(), Some("r00t4a1c"));

    let root = map.root().expect("root");
    assert_eq!(root.note.as_deref(), Some("Triagem na admissão"));
    assert!(root.has_note());

    let lactato = map.node("l4ct4t03").expect("node");
    assert_eq!(lactato.parent_id.as_deref(), Some("h3m0d1n4"));
    assert_eq!(lactato.note.as_deref(), Some("Repetir em 2h se elevado"));

    let culturas = map.node("c0ltur4s").expect("node");
    assert_eq!(culturas.width, Some(260.0));

    let volume = map.node("v0lum3r3").expect("node");
    assert_eq!(volume.font_size, Some(15.0));

    let reavaliacao = map.node("r3av4l14").expect("node");
    assert_eq!(reavaliacao.side, Some(Side::Down));
    // A field this build does not know is carried, not dropped.
    assert_eq!(reavaliacao.extra.get("sticker").and_then(|v| v.as_str()), Some("star"));

    let antibiotico = map.node("4nt1b10t").expect("node");
    assert_eq!(antibiotico.collapsed, Some(true));
}

#[test]
fn the_layout_places_every_reachable_node_and_hides_the_collapsed_branch() {
    let map = map_io::load(&fixture("sepse.tmind")).expect("the fixture opens");
    let layout = layout_tree(&map.nodes);

    let hidden = map.hidden_ids();
    for node in &map.nodes {
        if node.id == "0rfa0sem4" {
            // The orphan points at a parent nobody has: no side, no position.
            assert!(!layout.positions.contains_key(&node.id));
            assert!(!layout.sides.contains_key(&node.id));
            continue;
        }
        assert!(layout.sides.contains_key(&node.id), "{} has no side", node.id);
        if hidden.contains(&node.id) {
            assert!(!layout.positions.contains_key(&node.id), "{} is hidden but placed", node.id);
        } else {
            assert!(layout.positions.contains_key(&node.id), "{} is not placed", node.id);
        }
    }

    // The collapsed branch stays folded: its child is the one hidden node, and the collapsed node
    // itself is still drawn (that is what lets it be expanded again).
    assert!(hidden.contains("4just4r"));
    assert!(!hidden.contains("4nt1b10t"));
    assert!(layout.positions.contains_key("4nt1b10t"));
    // The explicitly vertical root child keeps its own axis.
    assert_eq!(layout.sides.get("r3av4l14"), Some(&ResolvedSide::Left));
    assert_eq!(layout.sides.get("m0n1t0r4"), Some(&ResolvedSide::Left));
    assert_eq!(layout.sides.get("c0ndut4a"), Some(&ResolvedSide::Right));
    // `side` only means something on the root's children: a stray one further down is inherited
    // over (and kept in the file, so saving does not rewrite the user's data).
    assert_eq!(map.node("r3av4l14").and_then(|n| n.side), Some(Side::Down));
}

#[test]
fn opening_and_saving_gives_the_same_map_back() {
    let map = map_io::load(&fixture("sepse.tmind")).expect("the fixture opens");
    let text = map_io::to_json(&map).expect("serialize");
    let again = map_io::parse(&text).expect("parse");
    assert_eq!(map, again);
    // Saving twice is stable: the bytes do not drift between saves.
    assert_eq!(text, map_io::to_json(&again).expect("serialize"));
}

#[test]
fn a_duplicated_root_uses_the_first_one_everywhere() {
    let map = map_io::load(&fixture("raiz-duplicada.tmind")).expect("the fixture opens");
    assert_eq!(map.root_id(), Some("R1"));
    let layout = layout_tree(&map.nodes);
    assert!(layout.positions.contains_key("R1"));
    assert!(layout.positions.contains_key("F1"));
    assert!(!layout.positions.contains_key("R2"));
}

/// The file the original app wrote, opened as it is: the fixture is a real `.tmind` saved by the
/// Electron app (a neutral test map, from `C:\Dev\test-files\LocalMind\`), not a hand-made copy.
#[test]
fn a_real_file_from_the_original_app_opens_and_lays_out() {
    let map = map_io::load(&fixture("real-app.tmind")).expect("the real file opens");
    assert_eq!(map.title, "Novo mapa");
    assert_eq!(map.nodes.len(), 17);
    assert_eq!(map.root().map(|n| n.text.as_str()), Some("Mapa Mental"));
    // The root carries a custom width: the resize handle was used on it in the original app.
    assert_eq!(map.root().and_then(|n| n.width), Some(120.0));

    let layout = layout_tree(&map.nodes);
    assert_eq!(layout.positions.len(), 17, "every node is placed");
    assert!(layout.bounds(&map.nodes).is_some());

    // Saving it back keeps every field the app wrote.
    let text = map_io::to_json(&map).expect("serialize");
    let again = map_io::parse(&text).expect("parse");
    assert_eq!(map, again);
}

/// Opening a file that is not a map says why instead of showing an empty canvas.
#[test]
fn a_file_that_is_not_a_map_is_an_error() {
    let path = std::env::temp_dir().join(format!("localmindmap-not-a-map-{}.tmind", std::process::id()));
    std::fs::write(&path, "<html>não é um mapa</html>").expect("write");
    let error = map_io::load(&path).expect_err("must not parse");
    assert!(matches!(error, map_io::Error::Format(_)), "{error:?}");
    let _ = std::fs::remove_file(&path);
}
