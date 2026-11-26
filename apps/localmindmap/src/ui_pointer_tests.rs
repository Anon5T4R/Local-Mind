//! The pointer on the map and the keyboard around it, in the whole window: the width handle, a
//! drag on the map, a click on an empty stretch, and the chat panel's buttons. These were written
//! to compare two commits during the fourth blind retest, and stay as regression tests.

use craft_icons::ThemeMode;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use crate::app::LocalApp;
use crate::doc::Document;
use crate::prefs::Prefs;

const FIXTURE: &str = include_str!("../../../crates/io/tests/fixtures/sepse.tmind");

fn app() -> LocalApp {
    let mut map = localmindmap_io::parse(FIXTURE).expect("the fixture opens");
    let root = map.root_id().map(str::to_string).expect("the fixture has a root");
    let long = map.add_child(&root, "Reconhecimento precoce da disfunção orgânica associada à infecção em pacientes internados", None);
    map.add_child(&long, "Pontuação SOFA calculada a cada oito horas durante toda a internação na unidade de terapia intensiva", None);
    map.add_child(&long, "qSOFA", None);
    map.add_child(&root, "Antibioticoterapia empírica de amplo espectro iniciada na primeira hora após o reconhecimento", None);
    let doc = Document { map, path: None, dirty: false };
    LocalApp::for_test(craft_window::Frame::Desktop, Prefs { language: "en".into(), theme: ThemeMode::Light.id().to_string() }, doc, ThemeMode::Light)
}

fn harness() -> Harness<'static, LocalApp> {
    Harness::builder().with_accessibility_check(true).with_size(egui::vec2(1100.0, 760.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut LocalApp| {
            egui_extras::install_image_loaders(ui.ctx());
            crate::i18n::set_language(&app.language());
            app.theme().apply(ui.ctx());
            app.draw_for_test(ui);
        },
        app(),
    )
}

fn key(h: &mut Harness<'_, LocalApp>, key: egui::Key, modifiers: egui::Modifiers) {
    for pressed in [true, false] {
        h.event(egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers });
    }
    h.run_steps(2);
}

fn card(h: &Harness<'_, LocalApp>, text: &str) -> egui::Rect {
    let editor = &h.state().editor;
    let node = editor.doc.map.nodes.iter().find(|n| n.text == text).expect("the node exists");
    let layout = localmindmap_layout::layout_tree(&editor.doc.map.nodes);
    let at = *layout.positions.get(&node.id).expect("the node is placed");
    let size = localmindmap_layout::node_size(node);
    crate::canvas::card_rect(&editor.view, editor.rect, at, (size.w, size.h))
}

fn pointer(h: &mut Harness<'_, LocalApp>, event: egui::Event) {
    h.event(event);
    h.run_steps(1);
}

fn press(h: &mut Harness<'_, LocalApp>, at: egui::Pos2, pressed: bool) {
    pointer(h, egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::default() });
}

/// A drag of the primary button from `from` to `to`, in small steps.
fn drag(h: &mut Harness<'_, LocalApp>, from: egui::Pos2, to: egui::Pos2) {
    pointer(h, egui::Event::PointerMoved(from));
    press(h, from, true);
    for step in 1..=8 {
        pointer(h, egui::Event::PointerMoved(from + (to - from) * step as f32 / 8.0));
    }
    press(h, to, false);
    h.run_steps(3);
}

fn selected_facts(h: &Harness<'_, LocalApp>, text: &str) -> (Option<f32>, egui::Vec2, f32) {
    let editor = &h.state().editor;
    let width = editor.doc.map.nodes.iter().find(|n| n.text == text).map(localmindmap_layout::width_of);
    (width, editor.view.pan, editor.view.zoom)
}

/// Dragging the width handle of the selected card changes its width and does not pan the map.
#[test]
fn dragging_the_width_handle_resizes_the_card() {
    let mut h = harness();
    h.run_steps(3);
    let body = card(&h, "qSOFA");
    drag(&mut h, body.center(), body.center());
    assert_eq!(
        h.state().editor.selected.as_deref(),
        h.state().editor.doc.map.nodes.iter().find(|n| n.text == "qSOFA").map(|n| n.id.as_str()),
        "the click selects qSOFA"
    );
    let (width, pan, zoom) = selected_facts(&h, "qSOFA");
    let handle = crate::canvas::handle_rect(&h.state().editor.view, card(&h, "qSOFA"));

    drag(&mut h, handle.center(), handle.center() + egui::vec2(40.0, 0.0));
    let (width_after, pan_after, zoom_after) = selected_facts(&h, "qSOFA");
    assert!(width_after.unwrap_or(0.0) > width.unwrap_or(0.0) + 10.0, "the handle drag did not widen the card: {width:?} -> {width_after:?}");
    assert_eq!(pan_after, pan, "the handle drag panned the map");
    assert_eq!(zoom_after, zoom);
}

/// Dragging the body of a card pans the map (there is no card drag in this app): the card keeps
/// its place in the layout and its width.
#[test]
fn dragging_a_card_pans_the_map() {
    let mut h = harness();
    h.run_steps(3);
    let body = card(&h, "qSOFA");
    drag(&mut h, body.center(), body.center());
    let (width, pan, _) = selected_facts(&h, "qSOFA");
    let other = card(&h, "Antibioticoterapia empírica de amplo espectro iniciada na primeira hora após o reconhecimento");

    drag(&mut h, body.center(), body.center() + egui::vec2(52.0, 0.0));
    let (width_after, pan_after, _) = selected_facts(&h, "qSOFA");
    let other_after = card(&h, "Antibioticoterapia empírica de amplo espectro iniciada na primeira hora após o reconhecimento");
    assert_ne!(pan_after, pan, "dragging a card did not pan the map");
    assert_eq!(width_after, width, "dragging a card changed its width");
    assert_eq!(other_after.min - other.min, pan_after - pan, "the cards kept their place relative to each other");
}

/// A click on an empty stretch of the map clears the selection; with nothing selected, Tab, the
/// arrows and Enter have nothing to act on, and must not reach the toolbar either: before the
/// click gave the canvas the keyboard, that Tab focused "New map" and that Enter replaced the map.
#[test]
fn a_click_on_the_empty_map_then_the_keys() {
    let mut h = harness();
    h.run_steps(3);
    let selected = h.state().editor.selected.clone();
    assert!(selected.is_some(), "a card is selected before the click (the root)");
    let before = h.state().editor.doc.map.nodes.len();
    let empty = {
        let editor = &h.state().editor;
        editor.rect.left_top() + egui::vec2(editor.rect.width() * 0.5, editor.rect.height() * 0.12)
    };
    drag(&mut h, empty, empty);
    assert!(h.state().editor.selected.is_none(), "a click on the empty map clears the selection");

    for pressed in [egui::Key::Tab, egui::Key::ArrowDown, egui::Key::Enter] {
        key(&mut h, pressed, egui::Modifiers::NONE);
    }
    assert_eq!(h.state().editor.doc.map.nodes.len(), before, "with nothing selected the keys changed the map");
    assert!(h.state().editor.editing.is_none(), "with nothing selected the keys opened a text field");
}

/// With the keyboard on the chat panel's "Send" button, Backspace does not edit the composer.
#[test]
fn backspace_with_send_focused() {
    let mut h = harness();
    h.run_steps(3);
    h.state_mut().editor_mut().ai.chat_open = true;
    h.run_steps(3);
    h.get_by_role(Role::MultilineTextInput).click();
    h.run_steps(2);
    h.event(egui::Event::Text("uma pergunta".into()));
    h.run_steps(2);
    key(&mut h, egui::Key::Tab, egui::Modifiers::NONE);
    let focused = h.root().query_by(|n| n.is_focused()).and_then(|n| n.accesskit_node().label());
    assert_eq!(focused.as_deref(), Some("Send"), "Tab out of the composer puts the keyboard on Send");

    key(&mut h, egui::Key::Backspace, egui::Modifiers::NONE);
    let value = h.get_by_role(Role::MultilineTextInput).value();
    assert_eq!(value.as_deref(), Some("uma pergunta"), "Backspace with Send focused edited the composer");
}
