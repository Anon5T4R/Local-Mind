use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use localmindmap_layout::{layout_tree, node_size};
use localmindmap_model::{MindMap, Side};

use crate::canvas;
use crate::doc::Document;
use crate::edit::{self, Editor};

const FIXTURE: &str = include_str!("../../../crates/io/tests/fixtures/sepse.tmind");

fn editor() -> Editor {
    let doc = Document { map: localmindmap_io::parse(FIXTURE).expect("the fixture opens"), path: None, dirty: false };
    Editor::new(doc)
}

fn harness() -> Harness<'static, Editor> {
    harness_with(editor())
}

fn harness_with(editor: Editor) -> Harness<'static, Editor> {
    Harness::builder()
        .with_size(egui::vec2(1100.0, 760.0))
        // The default step is a quarter of a second, which is longer than egui's double-click
        // window: a realistic frame keeps clicks, drags and double clicks behaving like they do.
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, editor: &mut Editor| {
                // The same menus the window uses (they live in the editor).
                egui::MenuBar::new().ui(ui, |ui| {
                    let _ = edit::menus(ui, editor);
                    crate::ai::menu(ui, editor);
                });
                editor.theme.apply(ui.ctx());
                editor.ui(ui);
            },
            editor,
        )
}

/// A backend that answers from a canned string, with no network at all.
struct MockAi {
    answer: Result<String, String>,
    /// The last request's messages, when the test wants to see what the model was told.
    seen: Option<std::sync::Arc<std::sync::Mutex<Vec<local_ai::Msg>>>>,
}

impl localmindmap_ai::Backend for MockAi {
    fn stream(&self, _connection: &local_ai::Connection, history: &[local_ai::Msg], on_delta: &mut dyn FnMut(&str)) -> Result<String, String> {
        if let Some(seen) = &self.seen
            && let Ok(mut seen) = seen.lock()
        {
            *seen = history.to_vec();
        }
        match &self.answer {
            Ok(answer) => {
                // Two deltas, so the streaming path (accumulating partial text) is exercised.
                let (head, tail) = answer.split_at(answer.len() / 2);
                on_delta(head);
                on_delta(tail);
                Ok(answer.clone())
            }
            Err(reason) => Err(reason.clone()),
        }
    }

    fn models(&self, _connection: &local_ai::Connection) -> Result<Vec<String>, String> {
        Ok(vec!["mock-model".to_string()])
    }
}

/// An editor with a mock model and one saved connection, so the AI actions run without a network.
fn ai_editor(answer: Result<&str, &str>) -> Editor {
    let mut editor = editor();
    editor.ai.backend = std::sync::Arc::new(MockAi { answer: answer.map(str::to_string).map_err(str::to_string), seen: None });
    // Hermetic: the connection is handed in, so the store and the keychain are never read.
    editor.ai.use_connection(Some(local_ai::Connection {
        id: "test".into(),
        provider: "custom".into(),
        label: "Test".into(),
        base_url: "http://example.invalid/v1".into(),
        model: "mock-model".into(),
    }));
    editor
}

/// [`ai_editor`] whose backend also records the messages the model was sent.
fn ai_editor_seen(answer: &str) -> (Editor, std::sync::Arc<std::sync::Mutex<Vec<local_ai::Msg>>>) {
    let mut editor = ai_editor(Ok(answer));
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    editor.ai.backend = std::sync::Arc::new(MockAi { answer: Ok(answer.to_string()), seen: Some(seen.clone()) });
    (editor, seen)
}

/// Runs frames (with a little wall time) until `done` holds: the AI answers on a worker thread.
fn wait_for(harness: &mut Harness<'_, Editor>, done: impl Fn(&Editor) -> bool) {
    for _ in 0..400 {
        if done(harness.state()) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
        harness.run_steps(1);
    }
    panic!("the AI worker did not finish in time");
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
    if crate::legacy::env_var_os("QA_RENDER").is_none() {
        return;
    }
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/qa-f3");
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
    let before = localmindmap_layout::width_of(node);

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
    let restored = harness.state().doc.map.node(root).and_then(|n| n.width).unwrap_or(localmindmap_layout::NODE_W);
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

/// The View menu changes the theme: a real click on View -> Theme -> Light, Dark and Automatic (the suite's three).
#[test]
fn the_view_menu_changes_the_theme() {
    let mut harness = harness();
    harness.run_steps(3);
    assert_eq!(harness.state().theme, crate::theme::Theme::Dark);

    let pick = |harness: &mut Harness<'static, Editor>, label: &str| {
        harness.get_by_label("View").click();
        harness.run_steps(2);
        harness.get_by_label("Theme").click();
        harness.run_steps(2);
        harness.get_by_label(label).click();
        harness.run_steps(3);
    };

    pick(&mut harness, "Light");
    assert_eq!(harness.state().mode, craft_icons::ThemeMode::Light);
    assert_eq!(harness.state().theme, crate::theme::Theme::Light);
    // The canvas paints with the theme it now reports.
    assert_eq!(harness.state().colors().canvas, crate::theme::Theme::Light.colors().canvas);
    assert!(!harness.state().colors().dark);
    shot(&mut harness, "theme-light");

    pick(&mut harness, "Dark");
    assert_eq!(harness.state().mode, craft_icons::ThemeMode::Dark);
    assert_eq!(harness.state().theme, crate::theme::Theme::Dark);
    shot(&mut harness, "theme-dark");

    // Automatic follows the OS; the headless context reports none, which reads as dark.
    pick(&mut harness, "Automatic");
    assert_eq!(harness.state().mode, craft_icons::ThemeMode::System);
    assert_eq!(harness.state().theme, crate::theme::Theme::Dark);
}

/// Edit > Settings… asks the app for its Settings window (the gear does the same in the window's top bar).
#[test]
fn the_edit_menu_asks_for_settings() {
    let mut harness = harness();
    harness.run_steps(3);
    assert!(!harness.state().settings_requested);
    harness.get_by_label("Edit").click();
    harness.run_steps(2);
    harness.get_by_label("Settings…").click();
    harness.run_steps(2);
    assert!(harness.state().settings_requested);
}

/// The root's "+": a click adds a child on that side, selects it and opens its text field.
#[test]
fn the_root_hot_corners_add_a_branch_on_each_side() {
    for side in Side::ALL {
        let mut harness = harness();
        harness.run_steps(3);
        let root = harness.state().doc.root_id().expect("the fixture has a root");
        let before = harness.state().doc.map.children(&root).len();
        let pos = corner_pos(&harness, side);
        click(&mut harness, pos);

        let created = harness.state().editing.as_ref().map(|e| e.node.clone()).expect("the new branch opens its text field");
        let node = harness.state().doc.map.node(&created).expect("the branch exists");
        assert_eq!(node.parent_id.as_deref(), Some(root.as_str()));
        assert_eq!(node.side, Some(side), "the branch lands on the side of the button");
        assert_eq!(harness.state().doc.map.children(&root).len(), before + 1, "one branch per click");
        shot(&mut harness, &format!("hot-corner-{side:?}").to_lowercase());
    }
}

/// The "move to…" dialog (the keyboard-free re-parent and direction change): it opens from the card
/// menu and lists the parents a node may be moved under.
#[test]
fn the_move_to_dialog_offers_the_valid_parents() {
    let mut harness = harness();
    harness.run_steps(3);
    right_click_card(&mut harness, "c0ltur4s");
    harness.run_steps(2);
    harness.get_by_label("Move to…").click();
    harness.run_steps(3);
    let state = harness.state();
    let dialog = state.move_to.as_ref().expect("the dialog is open");
    assert_eq!(dialog.node, "c0ltur4s");
    let targets = state.doc.map.reparent_targets("c0ltur4s");
    assert!(targets.contains(&"d3b1t0s3".to_string()));
    assert!(!targets.contains(&"c0ltur4s".to_string()), "a card is not a parent of itself");
    shot(&mut harness, "move-to-dialog");

    // The move itself is the same model operation the dialog's button runs (the button is a Window
    // row, which the synthetic click cannot reach - see the lesson in licoes-de-porta).
    let mut harness2 = harness_with(editor());
    harness2.run_steps(2);
    harness2.state_mut().move_node_to("c0ltur4s", "d3b1t0s3", None);
    assert_eq!(harness2.state().doc.map.node("c0ltur4s").and_then(|n| n.parent_id.as_deref()), Some("d3b1t0s3"));
}

/// With the root picked as the parent, the dialog offers a direction; the move lands the branch on it
/// (outdent + side in one step).
#[test]
fn the_move_to_dialog_can_send_a_branch_to_a_side() {
    let mut harness = harness();
    harness.run_steps(3);
    let root = harness.state().doc.root_id().expect("the fixture has a root");
    harness.state_mut().move_to = Some(edit::MoveTo { node: "c0ltur4s".into(), choice: Some(root.clone()), side: Some(Side::Left) });
    harness.run_steps(3);
    shot(&mut harness, "move-to-dialog-side");

    // Apply the same operation the Move button runs.
    harness.state_mut().move_node_to("c0ltur4s", &root, Some(Side::Left));
    let node = harness.state().doc.map.node("c0ltur4s").expect("still there");
    assert_eq!(node.parent_id.as_deref(), Some(root.as_str()), "it is a root child now");
    assert_eq!(node.side, Some(Side::Left), "on the chosen side");
}

/// The centre of a root "+" on screen.
fn corner_pos(harness: &Harness<'_, Editor>, side: Side) -> egui::Pos2 {
    let state = harness.state();
    let layout = layout_tree(&state.doc.map.nodes);
    let root = state.doc.map.root().expect("the fixture has a root");
    let pos = *layout.positions.get(&root.id).expect("the root is placed");
    let size = node_size(root);
    let card = canvas::card_rect(&state.view, state.rect, pos, (size.w, size.h));
    canvas::corner_center(&state.view, card, side)
}

/// A click on the minimap moves the view.
#[test]
fn clicking_the_minimap_moves_the_view() {
    let mut harness = harness();
    harness.run_steps(3);
    let before = harness.state().view.pan;
    let minimap = canvas::minimap_rect(harness.state().rect).expect("the canvas is big enough for a minimap");
    // Right of the centre: the view has to pan left.
    let target = minimap.center() + egui::vec2(minimap.width() * 0.3, 0.0);
    click(&mut harness, target);
    let after = harness.state().view.pan;
    assert_ne!(before, after, "the click moves the canvas");
    assert!(after.x < before.x, "clicking right of the minimap centre pans the map left: {before:?} -> {after:?}");
    shot(&mut harness, "minimap");
}

/// The AI: "Generate map…" sends the topic, the mock answers a tree, and the map is replaced with
/// it - as one undo step, with the view refitted.
#[test]
fn the_generate_dialog_replaces_the_map_with_the_answer() {
    let answer = r#"{"text":"Sepse","children":[{"text":"Reconhecimento"},{"text":"Conduta"}]}"#;
    let mut harness = harness_with(ai_editor(Ok(answer)));
    harness.run_steps(3);
    harness.state_mut().ai.generate_topic = Some("Sepse".into());
    harness.run_steps(2);
    shot(&mut harness, "ai-generate-dialog");

    harness.get_by_label("Generate").click();
    wait_for(&mut harness, |editor| editor.doc.map.title == "Sepse");

    let map = &harness.state().doc.map;
    assert_eq!(map.title, "Sepse");
    let texts: Vec<&str> = map.nodes.iter().map(|n| n.text.as_str()).collect();
    assert_eq!(texts, ["Sepse", "Reconhecimento", "Conduta"]);
    assert_eq!(map.root().map(|n| n.text.as_str()), Some("Sepse"));
    assert!(harness.state().doc.dirty);
    assert!(harness.state().can_undo(), "the generation is one undo step");
    shot(&mut harness, "ai-generated-map");
}

/// The AI actions live on the card's own menu, not only in the menubar, and "Color branches" is
/// there for a card that has children.
#[test]
fn the_card_menu_offers_the_ai_actions_and_colour_branches() {
    let mut harness = harness_with(ai_editor(Ok(r#"{"items":["Um"]}"#)));
    harness.run_steps(3);
    let pos = card_center(&harness, "l4ct4t03");
    right_click(&mut harness, pos);
    for label in ["Expand with AI", "Expand deep", "Explain", "Rewrite.", "Color branches"] {
        assert!(harness.query_by_label(label).is_some(), "{label} is missing from the card menu");
    }
}

/// Ctrl+Enter on the selected card runs Expand with the AI: the most used action never needs a menu.
#[test]
fn ctrl_enter_expands_the_selected_node() {
    let mut harness = harness_with(ai_editor(Ok(r#"{"items":["Primeiro","Segundo"]}"#)));
    harness.run_steps(3);
    click_card(&mut harness, "l4ct4t03");
    key(&mut harness, egui::Key::Enter, egui::Modifiers::COMMAND);
    wait_for(&mut harness, |editor| editor.doc.map.children("l4ct4t03").len() == 2);
    let children: Vec<&str> = harness.state().doc.map.children("l4ct4t03").iter().map(|n| n.text.as_str()).collect();
    assert_eq!(children, ["Primeiro", "Segundo"]);
}

/// "Expand" on the selected card adds the model's items as children.
#[test]
fn expand_from_the_ai_menu_adds_children() {
    let mut harness = harness_with(ai_editor(Ok(r#"{"items":["Gasometria","Hemoculturas"]}"#)));
    harness.run_steps(3);
    click_card(&mut harness, "l4ct4t03");
    harness.get_by_label("AI").click();
    harness.run_steps(2);
    harness.get_by_label("Expand").click();
    wait_for(&mut harness, |editor| editor.doc.map.children("l4ct4t03").len() == 2);

    let map = &harness.state().doc.map;
    let children: Vec<&str> = map.children("l4ct4t03").iter().map(|n| n.text.as_str()).collect();
    assert_eq!(children, ["Gasometria", "Hemoculturas"]);
    assert_eq!(harness.state().selected.as_deref(), Some(children.first().map(|_| map.children("l4ct4t03")[0].id.as_str()).unwrap_or("")));
}

/// The chat streams into a bubble, and the answer can be inserted into the map.
#[test]
fn the_chat_answers_and_inserts_a_node() {
    let mut harness = harness_with(ai_editor(Ok("Uma ideia: revisar o protocolo.")));
    harness.run_steps(3);
    // The card is selected before the chat opens: the panel takes width from the map, and a click
    // meant for a card under it would land on the panel.
    click_card(&mut harness, "h3m0d1n4");
    harness.get_by_label("AI").click();
    harness.run_steps(2);
    harness.get_by_label("Chat").click();
    harness.run_steps(2);
    assert!(harness.state().ai.chat_open);

    let ctx = harness.ctx.clone();
    harness.state_mut().ai_ask(&ctx, "me dê uma ideia");
    wait_for(&mut harness, |editor| !editor.ai.busy());
    // The answer lands inside a frame; the bubble (and its buttons) is drawn on the next one.
    harness.run_steps(2);

    harness.get_by_label("Add the last answer as a node").click();
    harness.run_steps(2);
    let children = harness.state().doc.map.children("h3m0d1n4").len();
    assert_eq!(children, 3, "the fixture's two children plus the inserted node");
    shot(&mut harness, "ai-chat");
}

/// The chat is not blind anymore: the request carries the map's outline, with the chip's label.
#[test]
fn the_chat_sends_the_map_as_context() {
    let (editor, seen) = ai_editor_seen("Ok.");
    let mut harness = harness_with(editor);
    harness.run_steps(3);
    let ctx = harness.ctx.clone();
    harness.state_mut().ai_ask(&ctx, "do que fala o mapa?");
    wait_for(&mut harness, |editor| !editor.ai.busy());

    let seen = seen.lock().expect("the recorded request");
    let joined: String = seen
        .iter()
        .map(|message| match message {
            local_ai::Msg::System(text) | local_ai::Msg::User(text) | local_ai::Msg::Assistant(text) => text.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("the map · nodes:"), "the chip's label travels: {joined}");
    assert!(joined.contains("- Sepse"), "the map's outline travels: {joined}");
    assert!(joined.contains("do que fala o mapa?"), "the question travels: {joined}");
}

/// A chat with no connection configured says why, inside the panel, with Retry beside it.
#[test]
fn the_chat_without_a_connection_says_why() {
    let mut editor = editor();
    // Hermetic: no store, no keychain.
    editor.ai.use_connection(None);
    let mut harness = harness_with(editor);
    harness.run_steps(2);
    let ctx = harness.ctx.clone();
    harness.state_mut().ai_ask(&ctx, "alguém aí?");
    harness.run_steps(3);
    // The reason and the way back are drawn by the panel itself.
    harness.get_by_label("Retry");
}

/// The connections form is the suite's: the fields a provider needs, and nothing written to the
/// keychain (the test never presses Save).
#[test]
fn the_connections_form_opens_with_the_suite_fields() {
    let mut harness = harness();
    harness.run_steps(3);
    harness.get_by_label("AI").click();
    harness.run_steps(2);
    harness.get_by_label("Connections…").click();
    harness.run_steps(2);
    assert!(harness.state().ai.settings_open);
    for label in ["Provider", "Base URL", "Model", "API key", "Save", "Cancel"] {
        harness.get_by_label(label);
    }
    shot(&mut harness, "ai-connections");
}

/// Visual QA (ignored): the chat panel in the dark default, the craft light theme and a coloured
/// one, with an answer in the bubble. Run with
/// `cargo test -p localmindmap --bin localmindmap ui_tests::ai_chat_renders_for_visual_qa -- --ignored --nocapture`.
#[test]
#[ignore = "visual QA: renders the chat panel in both palettes"]
fn ai_chat_renders_for_visual_qa() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/qa-ai");
    if std::fs::create_dir_all(&directory).is_err() {
        return;
    }
    for (name, theme) in [("dark", crate::theme::Theme::Dark), ("light", crate::theme::Theme::Light)] {
        let (editor, _seen) = ai_editor_seen("O mapa trata do protocolo de sepse: reconhecimento, conduta e o ajuste pela cultura.");
        let mut harness = harness_with(editor);
        harness.run_steps(3);
        click_card(&mut harness, "h3m0d1n4");
        harness.get_by_label("AI").click();
        harness.run_steps(2);
        harness.get_by_label("Chat").click();
        harness.run_steps(2);
        harness.state_mut().theme = theme;
        let ctx = harness.ctx.clone();
        harness.state_mut().ai_ask(&ctx, "resuma o mapa");
        wait_for(&mut harness, |editor| !editor.ai.busy());
        harness.run_steps(4);
        if let Ok(image) = harness.render() {
            let _ = image.save(directory.join(format!("ai-chat-{name}.png")));
        }
    }
}

/// A model that fails (no key, provider down) surfaces the reason instead of an empty map.
#[test]
fn a_failing_model_shows_the_reason() {
    let mut harness = harness_with(ai_editor(Err("invalid API key")));
    harness.run_steps(3);
    harness.state_mut().ai.generate_topic = Some("Sepse".into());
    harness.run_steps(2);
    harness.get_by_label("Generate").click();
    wait_for(&mut harness, |editor| editor.ai.error.is_some());
    // The worker result is applied after the strip is drawn (the strip comes before the canvas,
    // `ai_tick` after it), so the next frame is the one that shows the banner.
    harness.run_steps(2);

    assert_eq!(harness.state().ai.error.as_deref(), Some("invalid API key"));
    // The map is untouched.
    assert_eq!(harness.state().doc.map.title, "Protocolo de sepse");
    shot(&mut harness, "ai-error");
}

/// A quota error (what a free tier sends, as a 200-with-error-body or a 429) is shown with the
/// provider's reason, not swallowed: the strip is drawn before the canvas, so it is visible.
#[test]
fn a_quota_error_is_shown_with_its_reason() {
    let reason = "Free usage limit reached. Free models work in OpenCode; subscribe to OpenCode Go to keep going.";
    let mut harness = harness_with(ai_editor(Err(reason)));
    harness.run_steps(3);
    harness.state_mut().ai.generate_topic = Some("Sepse".into());
    harness.run_steps(2);
    harness.get_by_label("Generate").click();
    wait_for(&mut harness, |editor| editor.ai.error.is_some());
    harness.run_steps(2);

    assert!(harness.state().ai.error.as_deref().unwrap_or_default().contains("Free usage limit"));
    shot(&mut harness, "ai-quota-error");
}

/// With no connection at all, the action explains what to do instead of failing silently.
#[test]
fn no_connection_says_how_to_add_one() {
    let mut harness = harness();
    harness.run_steps(3);
    click_card(&mut harness, "l4ct4t03");
    harness.get_by_label("AI").click();
    harness.run_steps(2);
    harness.get_by_label("Expand").click();
    harness.run_steps(2);
    let error = harness.state().ai.error.clone().unwrap_or_default();
    assert!(error.contains("Connections"), "{error}");
}

/// Opens the chat, focuses its composer and types a question (no connection: the map's own AI
/// actions would leave their error in the strip).
fn chat_with_a_typed_question(harness: &mut Harness<'_, Editor>) {
    harness.run_steps(3);
    click_card(harness, "l4ct4t03");
    harness.get_by_label("AI").click();
    harness.run_steps(2);
    harness.get_by_label("Chat").click();
    harness.run_steps(2);
    let field = harness.get_by_role(egui::accesskit::Role::MultilineTextInput);
    field.click();
    harness.run_steps(2);
    text(harness, "uma pergunta");
    assert!(harness.ctx.text_edit_focused(), "the composer has the keyboard");
}

/// With the composer focused, Ctrl+Enter is the composer's: the question goes out and the map's
/// own Ctrl+Enter ("Expand" the selected card) doesn't run under it.
///
/// The harness has no AI connection on purpose: that is what makes "Expand" observable (its error
/// lands in `ai.error`). With a connection the chat's `busy()` swallows the expand, and this test
/// would pass without the guard.
#[test]
fn ctrl_enter_in_the_chat_composer_does_not_also_run_the_map_shortcut() {
    let mut harness = harness();
    chat_with_a_typed_question(&mut harness);
    assert_eq!(harness.state().selected.as_deref(), Some("l4ct4t03"), "a card is selected");

    key(&mut harness, egui::Key::Enter, egui::Modifiers::COMMAND);
    harness.run_steps(2);

    let turns = harness.state().ai.chat_turns();
    assert_eq!(turns.iter().map(|(_, text)| text.as_str()).collect::<Vec<_>>(), ["uma pergunta"], "the composer sends with Ctrl+Enter");
    assert!(harness.state().ai.error.is_none(), "the map's Expand ran under the chat: {:?}", harness.state().ai.error);
}

/// Backspace typed in the composer erases a character of the question, never the selected card:
/// with the keyboard in a field, the whole map family (Ctrl+Enter, Backspace/Delete, Enter, the
/// arrows, Tab) stays off.
#[test]
fn backspace_in_the_chat_composer_does_not_delete_the_selected_card() {
    let mut harness = harness();
    chat_with_a_typed_question(&mut harness);
    let before = harness.state().doc.map.nodes.len();

    key(&mut harness, egui::Key::Backspace, egui::Modifiers::NONE);
    harness.run_steps(2);

    let field = harness.get_by_role(egui::accesskit::Role::MultilineTextInput);
    assert_eq!(field.value().as_deref(), Some("uma pergunt"), "Backspace erased one character of the question");
    assert_eq!(harness.state().doc.map.nodes.len(), before, "Backspace in the composer deleted a card");
}

/// Tab (and Shift+Tab) typed in the composer moves the keyboard out of the field, but is not the
/// map's Tab: no card is created and no text field opens. The keyboard lands on a button of the
/// panel, so it is still not the map's.
#[test]
fn tab_in_the_chat_composer_does_not_create_a_card() {
    for (tab, modifiers) in [("Tab", egui::Modifiers::NONE), ("Shift+Tab", egui::Modifiers::SHIFT)] {
        let mut harness = harness();
        chat_with_a_typed_question(&mut harness);
        let before = harness.state().doc.map.nodes.len();

        key(&mut harness, egui::Key::Tab, modifiers);
        harness.run_steps(2);

        assert_eq!(harness.state().doc.map.nodes.len(), before, "{tab} in the composer created a card");
        assert!(harness.state().editing.is_none(), "{tab} in the composer opened a text field");
        assert!(!harness.ctx.text_edit_focused(), "{tab} leaves the composer (egui's own Tab)");
        // No tab character went in: egui keeps Tab for the focus (`EventFilter { tab: false }`).
        assert_eq!(
            harness.get_by_role(egui::accesskit::Role::MultilineTextInput).value().as_deref(),
            Some("uma pergunta"),
            "{tab} typed a tab into the question"
        );
        // Where the focus went: to the panel's buttons (none is a text field).
        let focused = harness.root().query_by(|n| n.is_focused()).map(|n| n.accesskit_node().role());
        assert_eq!(focused, Some(egui::accesskit::Role::Button), "{tab} moved the focus to a button of the panel");

        // The keyboard is on a button of the panel now, so it is still not the map's: Backspace
        // here changes nothing. A click on a card is what hands the keyboard back (the next test).
        key(&mut harness, egui::Key::Backspace, egui::Modifiers::NONE);
        assert_eq!(
            harness.state().doc.map.nodes.len(),
            before,
            "after {tab} in the composer, the map took a key while a panel button has the keyboard"
        );
    }
}

/// After Tab out of the composer (the keyboard on a panel button), a click on a card hands the
/// keyboard back to the map.
#[test]
fn a_click_on_a_card_hands_the_keyboard_back_to_the_map() {
    let mut harness = harness();
    chat_with_a_typed_question(&mut harness);
    let before = harness.state().doc.map.nodes.len();
    key(&mut harness, egui::Key::Tab, egui::Modifiers::NONE);

    click_card(&mut harness, "l4ct4t03");
    assert!(!harness.ctx.text_edit_focused(), "the click on the card took the keyboard out of the field");
    key(&mut harness, egui::Key::Backspace, egui::Modifiers::NONE);

    assert_eq!(harness.state().doc.map.nodes.len(), before - 1, "after the click, the map did not take the key");
}

/// Tab, Shift+Tab and the arrows typed while a button of the chat panel has the keyboard (Send,
/// Settings, Clear conversation) are not the map's: no card is created and the selection stays.
/// The map only has the keys when no widget has them.
#[test]
fn keys_on_the_chat_panel_buttons_do_not_reach_the_map() {
    let mut harness = harness();
    chat_with_a_typed_question(&mut harness);
    let before = harness.state().doc.map.nodes.len();
    // Tab out of the composer: the focus lands on the panel's "Send" button.
    key(&mut harness, egui::Key::Tab, egui::Modifiers::NONE);
    let focused = harness.root().query_by(|n| n.is_focused()).and_then(|n| n.accesskit_node().label());
    assert_eq!(focused.as_deref(), Some("Send"), "the focus is on a button of the panel");
    assert!(!harness.ctx.text_edit_focused());

    for (name, pressed, modifiers) in [
        ("Shift+Tab", egui::Key::Tab, egui::Modifiers::SHIFT),
        ("Tab", egui::Key::Tab, egui::Modifiers::NONE),
        ("Down", egui::Key::ArrowDown, egui::Modifiers::NONE),
        ("Right", egui::Key::ArrowRight, egui::Modifiers::NONE),
        ("Backspace", egui::Key::Backspace, egui::Modifiers::NONE),
    ] {
        key(&mut harness, pressed, modifiers);
        assert!(harness.state().editing.is_none(), "{name} on a panel button opened a text field on the map");
        assert_eq!(harness.state().doc.map.nodes.len(), before, "{name} on a panel button changed the map");
        assert_eq!(harness.state().selected.as_deref(), Some("l4ct4t03"), "{name} on a panel button moved the selection");
    }
}

/// The map's Tab still creates a child with the chat open, once the keyboard is back on the map.
#[test]
fn tab_on_the_map_still_creates_a_child_with_the_chat_open() {
    let mut harness = harness();
    chat_with_a_typed_question(&mut harness);
    click_card(&mut harness, "l4ct4t03");
    assert!(!harness.ctx.text_edit_focused(), "the click on the card took the keyboard back");
    let before = harness.state().doc.map.nodes.len();

    key(&mut harness, egui::Key::Tab, egui::Modifiers::NONE);

    let created = harness.state().editing.as_ref().map(|e| e.node.clone()).expect("Tab on the map opens a new child for typing");
    assert_eq!(harness.state().doc.map.nodes.len(), before + 1);
    assert_eq!(harness.state().doc.map.node(&created).and_then(|n| n.parent_id.clone()).as_deref(), Some("l4ct4t03"));
}

/// The map's Ctrl+Enter still expands once the keyboard is back on the map (a click on a card).
#[test]
fn ctrl_enter_on_the_map_still_runs_with_the_chat_open() {
    let mut harness = harness_with(ai_editor(Ok(r#"{"items":["Primeiro","Segundo"]}"#)));
    chat_with_a_typed_question(&mut harness);
    click_card(&mut harness, "l4ct4t03");
    assert!(!harness.ctx.text_edit_focused(), "the click on the card took the keyboard back");

    key(&mut harness, egui::Key::Enter, egui::Modifiers::COMMAND);
    wait_for(&mut harness, |editor| editor.doc.map.children("l4ct4t03").len() == 2);

    let children: Vec<&str> = harness.state().doc.map.children("l4ct4t03").iter().map(|n| n.text.as_str()).collect();
    assert_eq!(children, ["Primeiro", "Segundo"]);
    assert!(harness.state().ai.chat_turns().is_empty(), "the composer's text was not sent");
}
