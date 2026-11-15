use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use localmind_layout::{layout_tree, node_size};
use localmind_model::MindMap;

use crate::canvas;
use crate::doc::Document;
use crate::edit::{self, Editor};

const FIXTURE: &str = include_str!("../../../crates/io/tests/fixtures/sepse.tmind");

fn editor() -> Editor {
    let doc = Document { map: localmind_io::parse(FIXTURE).expect("the fixture opens"), path: None, dirty: false };
    Editor::new(doc)
}

fn harness() -> Harness<'static, Editor> {
    Harness::builder()
        .with_size(egui::vec2(1100.0, 760.0))
        // The default step is a quarter of a second, which is longer than egui's double-click
        // window: a realistic frame keeps clicks, drags and double clicks behaving like they do.
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, editor: &mut Editor| editor.ui(ui), editor())
}

fn push(harness: &mut Harness<'_, Editor>, event: egui::Event) {
    harness.input_mut().events.push(event);
}

fn key_event(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
    vec![
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers },
        egui::Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers },
    ]
}

fn key(harness: &mut Harness<'_, Editor>, key: egui::Key, modifiers: egui::Modifiers) {
    for event in key_event(key, modifiers) {
        push(harness, event);
    }
    harness.run_steps(2);
}

fn text(harness: &mut Harness<'_, Editor>, text: &str) {
    push(harness, egui::Event::Text(text.to_string()));
    harness.run_steps(2);
}

fn click(harness: &mut Harness<'_, Editor>, pos: egui::Pos2) {
    push(harness, egui::Event::PointerMoved(pos));
    harness.run_steps(1);
    push(harness, egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::default() });
    harness.run_steps(1);
    push(harness, egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::default() });
    harness.run_steps(2);
}

fn right_click(harness: &mut Harness<'_, Editor>, pos: egui::Pos2) {
    push(harness, egui::Event::PointerMoved(pos));
    harness.run_steps(1);
    push(harness, egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed: true, modifiers: egui::Modifiers::default() });
    harness.run_steps(1);
    push(harness, egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed: false, modifiers: egui::Modifiers::default() });
    harness.run_steps(2);
}

fn card_center(harness: &Harness<'_, Editor>, id: &str) -> egui::Pos2 {
    let state = harness.state();
    let layout = layout_tree(&state.doc.map.nodes);
    let node = state.doc.map.node(id).expect("the node exists");
    let size = node_size(node);
    let pos = *layout.positions.get(id).expect("the node is placed");
    canvas::card_rect(&state.view, state.rect, pos, (size.w, size.h)).center()
}

fn badge_pos(harness: &Harness<'_, Editor>, id: &str) -> egui::Pos2 {
    let state = harness.state();
    let layout = layout_tree(&state.doc.map.nodes);
    let node = state.doc.map.node(id).expect("the node exists");
    let size = node_size(node);
    let pos = *layout.positions.get(id).expect("the node is placed");
    let card = canvas::card_rect(&state.view, state.rect, pos, (size.w, size.h));
    canvas::badge_center(&state.view, card)
}

fn click_card(harness: &mut Harness<'_, Editor>, id: &str) {
    let pos = card_center(harness, id);
    click(harness, pos);
}

fn right_click_card(harness: &mut Harness<'_, Editor>, id: &str) {
    let pos = card_center(harness, id);
    right_click(harness, pos);
}

fn double_click_card(harness: &mut Harness<'_, Editor>, id: &str) {
    let pos = card_center(harness, id);
    click(harness, pos);
    click(harness, pos);
}

fn click_badge(harness: &mut Harness<'_, Editor>, id: &str) {
    let pos = badge_pos(harness, id);
    click(harness, pos);
}

fn shot(harness: &mut Harness<'_, Editor>, name: &str) {
    if std::env::var_os("LOCALMIND_QA_RENDER").is_none() {
        return;
    }
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/qa-f2");
    if std::fs::create_dir_all(&directory).is_err() {
        return;
    }
    if let Ok(image) = harness.render() {
        let _ = image.save(directory.join(format!("{name}.png")));
    }
}

fn node_text(editor: &Editor, id: &str) -> Option<String> {
    editor.doc.map.node(id).map(|n| n.text.clone())
}

#[test]
fn tab_creates_a_child_and_typing_then_enter_commits_it() {
    let mut harness = harness();
    harness.run_steps(3);

    let parent = "l4ct4t03".to_string();
    click_card(&mut harness, &parent);
    assert_eq!(harness.state().selected.as_deref(), Some(parent.as_str()), "the click selects the card");

    key(&mut harness, egui::Key::Tab, egui::Modifiers::NONE);
    let editing = harness.state().editing.as_ref().map(|e| e.node.clone());
    let Some(created) = editing else { panic!("Tab opens the text field of a new child") };
    let parent_of_created = harness.state().doc.map.node(&created).and_then(|n| n.parent_id.clone());
    assert_eq!(parent_of_created.as_deref(), Some(parent.as_str()));

    text(&mut harness, "Novo ramo");
    shot(&mut harness, "editing-inline");
    key(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);

    assert!(harness.state().editing.is_none(), "Enter commits and closes the field");
    assert_eq!(node_text(harness.state(), &created).as_deref(), Some("Novo ramo"));
    assert!(harness.state().doc.dirty, "the map has unsaved changes");
    assert!(harness.state().can_undo());
}

#[test]
fn a_double_click_opens_the_text_and_escape_keeps_the_old_one() {
    let mut harness = harness();
    harness.run_steps(3);

    double_click_card(&mut harness, "h3m0d1n4");
    assert_eq!(harness.state().editing.as_ref().map(|e| e.node.as_str()), Some("h3m0d1n4"), "a double click edits the card");
    shot(&mut harness, "editing-double-click");

    text(&mut harness, "Outro texto");
    key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(harness.state().editing.is_none());
    assert_eq!(node_text(harness.state(), "h3m0d1n4").as_deref(), Some("Reconhecimento"), "Escape throws the draft away");
}

#[test]
fn enter_creates_a_sibling_of_the_selected_node() {
    let mut harness = harness();
    harness.run_steps(3);

    click_card(&mut harness, "l4ct4t03");
    key(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let created = harness.state().editing.as_ref().map(|e| e.node.clone()).expect("the sibling opens for typing");
    assert_eq!(harness.state().doc.map.node(&created).and_then(|n| n.parent_id.as_deref()), Some("h3m0d1n4"));
}

#[test]
fn delete_removes_the_subtree_and_selects_the_parent() {
    let mut harness = harness();
    harness.run_steps(3);

    click_card(&mut harness, "4nt1b10t");
    key(&mut harness, egui::Key::Delete, egui::Modifiers::NONE);

    let map = &harness.state().doc.map;
    assert!(map.node("4nt1b10t").is_none());
    assert!(map.node("4just4r").is_none(), "the child went with it");
    assert_eq!(harness.state().selected.as_deref(), Some("c0ndut4a"));
}

#[test]
fn space_collapses_and_the_badge_expands() {
    let mut harness = harness();
    harness.run_steps(3);

    click_card(&mut harness, "h3m0d1n4");
    key(&mut harness, egui::Key::Space, egui::Modifiers::NONE);
    assert_eq!(harness.state().doc.map.node("h3m0d1n4").and_then(|n| n.collapsed), Some(true));
    shot(&mut harness, "collapsed");

    click_badge(&mut harness, "h3m0d1n4");
    assert_eq!(harness.state().doc.map.node("h3m0d1n4").and_then(|n| n.collapsed), None, "the badge expands it again");
}

#[test]
fn arrow_navigation_walks_the_tree() {
    let mut harness = harness();
    harness.run_steps(3);

    click_card(&mut harness, "l4ct4t03");
    key(&mut harness, egui::Key::ArrowLeft, egui::Modifiers::NONE);
    assert_eq!(harness.state().selected.as_deref(), Some("h3m0d1n4"), "left goes to the parent");
    key(&mut harness, egui::Key::ArrowRight, egui::Modifiers::NONE);
    assert_eq!(harness.state().selected.as_deref(), Some("l4ct4t03"), "right goes to the first child");
    key(&mut harness, egui::Key::ArrowDown, egui::Modifiers::NONE);
    assert_eq!(harness.state().selected.as_deref(), Some("c0ltur4s"), "down goes to the next sibling");
    key(&mut harness, egui::Key::ArrowUp, egui::Modifiers::NONE);
    assert_eq!(harness.state().selected.as_deref(), Some("l4ct4t03"));
}

#[test]
fn alt_arrows_move_the_node_around_its_parent() {
    let mut harness = harness();
    harness.run_steps(3);

    click_card(&mut harness, "c0ltur4s");
    key(&mut harness, egui::Key::ArrowUp, egui::Modifiers::ALT);
    let order: Vec<String> = harness.state().doc.map.children("h3m0d1n4").iter().map(|n| n.id.clone()).collect();
    assert_eq!(order, ["c0ltur4s", "l4ct4t03"], "Alt+Up swaps it with the sibling above");

    // Back below the sibling (an indent needs one above it).
    key(&mut harness, egui::Key::ArrowDown, egui::Modifiers::ALT);
    key(&mut harness, egui::Key::ArrowRight, egui::Modifiers::ALT);
    assert_eq!(harness.state().doc.map.node("c0ltur4s").and_then(|n| n.parent_id.as_deref()), Some("l4ct4t03"), "Alt+Right indents");

    key(&mut harness, egui::Key::ArrowLeft, egui::Modifiers::ALT);
    assert_eq!(harness.state().doc.map.node("c0ltur4s").and_then(|n| n.parent_id.as_deref()), Some("h3m0d1n4"), "Alt+Left outdents");
}

#[test]
fn ctrl_z_undoes_the_last_edit_and_ctrl_y_redoes_it() {
    let mut harness = harness();
    harness.run_steps(3);

    click_card(&mut harness, "l4ct4t03");
    key(&mut harness, egui::Key::Delete, egui::Modifiers::NONE);
    assert!(harness.state().doc.map.node("l4ct4t03").is_none());

    key(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    assert!(harness.state().doc.map.node("l4ct4t03").is_some(), "undo brings the node back");
    key(&mut harness, egui::Key::Y, egui::Modifiers::COMMAND);
    assert!(harness.state().doc.map.node("l4ct4t03").is_none(), "redo deletes it again");
}

#[test]
fn the_card_menu_creates_a_child_by_click() {
    let mut harness = harness();
    harness.run_steps(3);

    right_click_card(&mut harness, "c0ndut4a");
    assert!(harness.state().menu.is_some(), "the right click opens the card menu");
    shot(&mut harness, "card-menu");

    harness.get_by_label("New child").click();
    harness.run_steps(3);
    assert!(harness.state().menu.is_none(), "the menu closes after an action");
    let created = harness.state().editing.as_ref().map(|e| e.node.clone()).expect("the new child opens for typing");
    assert_eq!(harness.state().doc.map.node(&created).and_then(|n| n.parent_id.as_deref()), Some("c0ndut4a"));
}

#[test]
fn the_note_window_writes_and_removes_the_note() {
    let mut harness = harness();
    harness.run_steps(3);

    right_click_card(&mut harness, "c0ltur4s");
    harness.get_by_label("Note…").click();
    harness.run_steps(2);
    assert!(harness.state().note.is_some(), "the note window opens");

    text(&mut harness, "Colher antes do antibiótico");
    shot(&mut harness, "note-window");
    harness.get_by_label("Save").click();
    harness.run_steps(2);
    assert_eq!(harness.state().doc.map.node("c0ltur4s").and_then(|n| n.note.clone()).as_deref(), Some("Colher antes do antibiótico"));
    assert!(harness.state().doc.map.node("c0ltur4s").is_some_and(|n| n.has_note()));

    right_click_card(&mut harness, "c0ltur4s");
    harness.get_by_label("Note…").click();
    harness.run_steps(2);
    harness.get_by_label("Remove note").click();
    harness.run_steps(2);
    assert!(!harness.state().doc.map.node("c0ltur4s").is_some_and(|n| n.has_note()));
}

#[test]
fn dragging_the_handle_resizes_the_selected_card() {
    let mut harness = harness();
    harness.run_steps(3);

    let root = "r00t4a1c";
    click_card(&mut harness, root);
    let state = harness.state();
    let layout = layout_tree(&state.doc.map.nodes);
    let node = state.doc.map.node(root).expect("root");
    let size = node_size(node);
    let card = canvas::card_rect(&state.view, state.rect, *layout.positions.get(root).expect("placed"), (size.w, size.h));
    let handle = canvas::handle_rect(&state.view, card).center();
    let before = localmind_layout::width_of(node);

    push(&mut harness, egui::Event::PointerMoved(handle));
    harness.run_steps(1);
    push(
        &mut harness,
        egui::Event::PointerButton { pos: handle, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::default() },
    );
    harness.run_steps(1);
    let moved = handle + egui::vec2(80.0, 0.0);
    push(&mut harness, egui::Event::PointerMoved(moved));
    harness.run_steps(2);
    push(
        &mut harness,
        egui::Event::PointerButton { pos: moved, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::default() },
    );
    harness.run_steps(2);

    let after = harness.state().doc.map.node(root).and_then(|n| n.width).unwrap_or(before);
    assert!(after > before + 40.0, "the card got wider: {before} -> {after}");
    harness.run_steps(1);
    let width_now = after;
    key(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    let restored = harness.state().doc.map.node(root).and_then(|n| n.width).unwrap_or(localmind_layout::NODE_W);
    assert!(restored < width_now, "undo puts the width back: {width_now} -> {restored}");
}

#[test]
fn enter_during_an_ime_composition_does_not_commit() {
    let mut harness = harness();
    harness.run_steps(3);

    click_card(&mut harness, "h3m0d1n4");
    key(&mut harness, egui::Key::F2, egui::Modifiers::NONE);
    assert!(harness.state().editing.is_some());

    push(&mut harness, egui::Event::Ime(egui::ImeEvent::Preedit { text: "ka".into(), active_range_chars: None }));
    for event in key_event(egui::Key::Enter, egui::Modifiers::NONE) {
        push(&mut harness, event);
    }
    harness.run_steps(2);
    assert!(harness.state().editing.is_some(), "Enter during a composition is the candidate list's, not a commit");

    push(&mut harness, egui::Event::Ime(egui::ImeEvent::Commit("ka".into())));
    harness.run_steps(2);
    key(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    assert!(harness.state().editing.is_none(), "the plain Enter commits");
    assert_eq!(node_text(harness.state(), "h3m0d1n4").as_deref(), Some("ka"));
}

#[test]
fn clicking_the_empty_canvas_clears_the_selection() {
    let mut harness = harness();
    harness.run_steps(3);

    click_card(&mut harness, "l4ct4t03");
    assert!(harness.state().selected.is_some());
    let corner = harness.state().rect.left_top() + egui::vec2(6.0, 6.0);
    click(&mut harness, corner);
    assert_eq!(harness.state().selected, None);
}

#[test]
fn an_edit_does_not_move_the_canvas() {
    let mut harness = harness();
    harness.run_steps(3);
    let before = (harness.state().view.zoom, harness.state().view.pan);

    click_card(&mut harness, "c0ndut4a");
    key(&mut harness, egui::Key::Space, egui::Modifiers::NONE);
    harness.run_steps(2);

    let after = (harness.state().view.zoom, harness.state().view.pan);
    assert_eq!(before, after);
}

#[test]
fn expanding_a_branch_places_its_children_again() {
    let mut harness = harness();
    harness.run_steps(3);
    assert!(!layout_tree(&harness.state().doc.map.nodes).positions.contains_key("4just4r"));

    click_badge(&mut harness, "4nt1b10t");
    harness.run_steps(2);
    assert_eq!(harness.state().doc.map.node("4nt1b10t").and_then(|n| n.collapsed), None);
    assert!(layout_tree(&harness.state().doc.map.nodes).positions.contains_key("4just4r"));
}

#[test]
fn the_editor_starts_fitted_on_the_whole_map() {
    let mut harness = harness();
    harness.run_steps(3);
    let state = harness.state();
    assert!(!state.view.needs_fit);
    assert!(state.rect.width() > 100.0);
    let _: &MindMap = &state.doc.map;
    let _ = edit::COLORS;
}
