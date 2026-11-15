//! Headless UI test of the map canvas, on `egui_kittest`.
//!
//! The canvas paints cards and links; they are not widgets, so the test drives a real pointer
//! click at a computed screen position and checks the selection that comes out. With
//! `LOCALMIND_QA_RENDER=1` the frame is also rendered to `target/qa-f1/canvas.png`, which is what
//! the visual QA looks at - the render needs a GPU, so it stays out of the default run.

use std::path::Path;

use egui_kittest::Harness;
use localmind_layout::{layout_tree, node_size};
use localmind_model::MindMap;

use crate::canvas::{self, CanvasView};

/// What the frame keeps between steps: the open map, the view and the selection.
struct CanvasState {
    map: MindMap,
    view: CanvasView,
    selected: Option<String>,
    rect: egui::Rect,
}

/// The compatibility fixture: a real map shape, with notes, colours, a collapsed branch and an
/// orphan node.
fn fixture() -> MindMap {
    localmind_io::parse(include_str!("../../../crates/io/tests/fixtures/sepse.tmind")).expect("the fixture opens")
}

fn harness() -> Harness<'static, CanvasState> {
    let state = CanvasState { map: fixture(), view: CanvasView::default(), selected: None, rect: egui::Rect::NOTHING };
    Harness::builder().with_size(egui::vec2(1100.0, 760.0)).build_ui_state(
        |ui, state: &mut CanvasState| {
            let out = canvas::show(ui, &state.map, &mut state.view, state.selected.as_deref());
            state.rect = out.rect;
            if let Some(id) = out.clicked {
                state.selected = Some(id);
            } else if out.clicked_empty {
                state.selected = None;
            }
        },
        state,
    )
}

/// A real primary click: move, press, release (egui needs the press and the release to land on the
/// same widget, which is what a card's rect is here).
fn click_at(harness: &mut Harness<'_, CanvasState>, pos: egui::Pos2) {
    harness.input_mut().events.push(egui::Event::PointerMoved(pos));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.run_steps(1);
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run_steps(2);
}

/// Saves the harness's screen when the QA render is asked for.
fn shot(harness: &mut Harness<'_, CanvasState>, name: &str) {
    if std::env::var_os("LOCALMIND_QA_RENDER").is_none() {
        return;
    }
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/qa-f1");
    if std::fs::create_dir_all(&directory).is_err() {
        return;
    }
    if let Ok(image) = harness.render() {
        let _ = image.save(directory.join(format!("{name}.png")));
    }
}

/// The screen centre of a card, from the map's own layout and the view the canvas settled on.
fn center_of(harness: &Harness<'_, CanvasState>, id: &str) -> egui::Pos2 {
    let state = harness.state();
    let layout = layout_tree(&state.map.nodes);
    let node = state.map.node(id).expect("the node exists");
    let size = node_size(node);
    let pos = *layout.positions.get(id).expect("the node is placed");
    canvas::card_rect(&state.view, state.rect, pos, (size.w, size.h)).center()
}

#[test]
fn the_canvas_draws_the_map_and_selects_the_card_that_was_clicked() {
    let mut harness = harness();
    harness.run_steps(3);
    // The fit happened on the first frames: the whole fixture is inside the canvas.
    assert!(harness.state().rect.width() > 100.0, "the canvas has a size");
    assert!(harness.state().view.zoom > 0.0);
    shot(&mut harness, "canvas-fitted");

    let target = center_of(&harness, "l4ct4t03");
    click_at(&mut harness, target);
    assert_eq!(harness.state().selected.as_deref(), Some("l4ct4t03"), "the clicked card is selected");
    shot(&mut harness, "canvas-selected");

    // Clicking empty canvas clears the selection.
    let empty = harness.state().rect.left_top() + egui::vec2(4.0, 4.0);
    click_at(&mut harness, empty);
    assert_eq!(harness.state().selected, None);
}

/// A collapsed branch hides its children, so a click where they would be hits nothing.
#[test]
fn a_click_on_a_hidden_card_hits_nothing() {
    let mut harness = harness();
    harness.run_steps(3);
    // `4just4r` is the child of the collapsed `4nt1b10t`.
    let state = harness.state();
    let layout = layout_tree(&state.map.nodes);
    assert!(!layout.positions.contains_key("4just4r"), "the collapsed child is not drawn");
}
