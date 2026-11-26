//! The window as a whole, headless (egui_kittest): the top bar, the palette, Settings, About and every dialog of
//! the app, with a map open (several nodes, some with long names), at the window's minimum width and at a
//! medium one. The kittest guards stay on: a field without a name or a glyph the fonts lack fails the test.
//! Nothing here touches the real preferences, the keychain or the network.

use std::path::Path;

use craft_icons::ThemeMode;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use crate::app::{LocalApp, Shell};
use crate::doc::Document;
use crate::prefs::Prefs;

const FIXTURE: &str = include_str!("../../../crates/io/tests/fixtures/sepse.tmind");

/// The window's minimum size (`main.rs`: `with_min_inner_size`) and the medium one the reviews also use.
const NARROW: egui::Vec2 = egui::vec2(900.0, 600.0);
const MEDIUM: egui::Vec2 = egui::vec2(1100.0, 760.0);

/// The fixture map plus a branch of long names (the case that breaks lists, cards and dialogs).
fn document() -> Document {
    let mut map = localmindmap_io::parse(FIXTURE).expect("the fixture opens");
    let root = map.root_id().map(str::to_string).expect("the fixture has a root");
    let long = map.add_child(&root, "Reconhecimento precoce da disfunção orgânica associada à infecção em pacientes internados", None);
    map.add_child(&long, "Pontuação SOFA calculada a cada oito horas durante toda a internação na unidade de terapia intensiva", None);
    map.add_child(&long, "qSOFA", None);
    map.add_child(&root, "Antibioticoterapia empírica de amplo espectro iniciada na primeira hora após o reconhecimento", None);
    Document { map, path: Some("C:\\Users\\qa\\Documentos\\mapas\\protocolo-de-sepse-hospital-universitario.tmind".into()), dirty: false }
}

fn prefs(language: &str, mode: ThemeMode) -> Prefs {
    Prefs { language: language.to_string(), theme: mode.id().to_string() }
}

fn app(frame: craft_window::Frame, language: &str, mode: ThemeMode) -> LocalApp {
    LocalApp::for_test(frame, prefs(language, mode), document(), mode)
}

fn harness(app: LocalApp, size: egui::Vec2) -> Harness<'static, LocalApp> {
    harness_with(app, size, true)
}

/// Like [`harness`], for a test that opens a list with names in scripts the test fonts lack (the
/// language dropdown).
fn harness_any_script(app: LocalApp, size: egui::Vec2) -> Harness<'static, LocalApp> {
    Harness::builder().with_accessibility_check(true).allow_missing_glyphs().with_size(size).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut LocalApp| {
            egui_extras::install_image_loaders(ui.ctx());
            crate::i18n::set_language(&app.language());
            app.theme().apply(ui.ctx());
            app.draw_for_test(ui);
        },
        app,
    )
}

/// `named_fields` false turns the unnamed-widget check off (see the connections dialog in
/// `every_dialog_fits_the_narrow_window`).
fn harness_with(app: LocalApp, size: egui::Vec2, named_fields: bool) -> Harness<'static, LocalApp> {
    Harness::builder().with_accessibility_check(named_fields).with_size(size).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut LocalApp| {
            egui_extras::install_image_loaders(ui.ctx());
            // The language is per thread (the UI runs on one): each test pins its own every frame.
            crate::i18n::set_language(&app.language());
            app.theme().apply(ui.ctx());
            app.draw_for_test(ui);
        },
        app,
    )
}

fn frames() -> [(&'static str, craft_window::Frame); 2] {
    [("desktop", craft_window::Frame::Desktop), ("app", craft_window::Frame::App(craft_window::Layout::default()))]
}

fn shot(harness: &mut Harness<'_, LocalApp>, name: &str) {
    if crate::legacy::env_var_os("QA_RENDER").is_none() {
        return;
    }
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/qa-guardas");
    if std::fs::create_dir_all(&directory).is_err() {
        return;
    }
    if let Ok(image) = harness.render() {
        let _ = image.save(directory.join(format!("{name}.png")));
    }
}

/// Nothing the screen reader can see leaves the window (the map's own cards are painted, not nodes).
fn assert_inside(harness: &Harness<'_, LocalApp>, size: egui::Vec2, what: &str) {
    for node in harness.root().children_recursive() {
        let Some(rect) = node.accesskit_node().bounding_box() else { continue };
        if rect.width() > 0.0 && rect.height() > 0.0 && !scrolled_row(&node, size.y) {
            assert!(
                rect.x0 >= -0.5 && rect.x1 <= f64::from(size.x) + 0.5 && rect.y0 >= -0.5 && rect.y1 <= f64::from(size.y) + 0.5,
                "{what}: {:?} {:?} sai da janela de {size:?}: {rect:?}",
                node.accesskit_node().role(),
                node.accesskit_node().label(),
            );
        }
    }
}

/// A list row (the palette) scrolled out of its scroll area keeps its node, but the area clips it: the user does
/// not see it cut. The area itself and the rows in view are still checked; only rows past the window's height are
/// skipped.
fn scrolled_row(node: &egui_kittest::Node<'_>, height: f32) -> bool {
    node.accesskit_node().role() == Role::ListBoxOption && node.accesskit_node().bounding_box().is_some_and(|r| r.y1 > f64::from(height))
}

/// The buttons of the top bar (icon only: search, theme, gear) are all on screen and do not overlap each other.
fn assert_bar_buttons(harness: &Harness<'_, LocalApp>, size: egui::Vec2, what: &str) {
    let mut rects: Vec<egui::Rect> =
        harness.get_all_by_role(Role::Button).map(|node| node.rect()).filter(|r| r.min.y < 40.0 && r.width() < 40.0).collect();
    rects.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
    assert!(rects.len() >= 3, "{what}: a barra tem lupa, tema e engrenagem: {rects:?}");
    for pair in rects.windows(2) {
        assert!(pair[0].max.x <= pair[1].min.x + 0.5, "{what}: botões da barra se sobrepõem: {pair:?}");
    }
    assert!(rects.iter().all(|r| r.max.x <= size.x), "{what}: botão da barra fora da janela: {rects:?}");
}

/// The keys and clicks of `ui_tests`, for the window as a whole.
fn key(harness: &mut Harness<'_, LocalApp>, key: egui::Key, modifiers: egui::Modifiers) {
    for pressed in [true, false] {
        harness.event(egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers });
    }
    harness.run_steps(2);
}

fn type_text(harness: &mut Harness<'_, LocalApp>, text: &str) {
    harness.event(egui::Event::Text(text.to_owned()));
    harness.run_steps(2);
}

/// Clicks the card of the node whose text is `text`.
fn click_card(harness: &mut Harness<'_, LocalApp>, text: &str) {
    let pos = {
        let editor = &harness.state().editor;
        let node = editor.doc.map.nodes.iter().find(|n| n.text == text).expect("the node exists");
        let layout = localmindmap_layout::layout_tree(&editor.doc.map.nodes);
        let at = *layout.positions.get(&node.id).expect("the node is placed");
        let size = localmindmap_layout::node_size(node);
        crate::canvas::card_rect(&editor.view, editor.rect, at, (size.w, size.h)).center()
    };
    click_at(harness, pos);
}

/// A right click of the mouse on the card of the node whose text is `text`.
fn right_click_card(harness: &mut Harness<'_, LocalApp>, text: &str) {
    let pos = {
        let editor = &harness.state().editor;
        let node = editor.doc.map.nodes.iter().find(|n| n.text == text).expect("the node exists");
        let layout = localmindmap_layout::layout_tree(&editor.doc.map.nodes);
        let at = *layout.positions.get(&node.id).expect("the node is placed");
        let size = localmindmap_layout::node_size(node);
        crate::canvas::card_rect(&editor.view, editor.rect, at, (size.w, size.h)).center()
    };
    for event in [
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed: true, modifiers: egui::Modifiers::default() },
        egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed: false, modifiers: egui::Modifiers::default() },
    ] {
        harness.event(event);
    }
    harness.run_steps(3);
}

/// A click of the primary button at `pos` (client coordinates).
fn click_at(harness: &mut Harness<'_, LocalApp>, pos: egui::Pos2) {
    harness.event(egui::Event::PointerMoved(pos));
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::default() });
    }
    harness.run_steps(2);
}

/// Opens the chat and types in its composer.
fn chat_with_text(harness: &mut Harness<'_, LocalApp>, text: &str) {
    harness.state_mut().editor_mut().ai.chat_open = true;
    harness.run_steps(3);
    harness.get_by_role(Role::MultilineTextInput).click();
    harness.run_steps(2);
    type_text(harness, text);
    assert!(harness.ctx.text_edit_focused(), "the composer has the keyboard");
}

/// Ctrl+Z and Ctrl+Y typed in the chat composer never undo or redo the map.
#[test]
fn undo_keys_in_the_chat_composer_leave_the_map_alone() {
    let mut harness = harness(app(craft_window::Frame::Desktop, "pt-br", ThemeMode::Light), MEDIUM);
    harness.run_steps(3);
    // One edit to undo: a card deleted from the map.
    click_card(&mut harness, "qSOFA");
    key(&mut harness, egui::Key::Delete, egui::Modifiers::NONE);
    let after_delete = harness.state().editor.doc.map.nodes.len();
    assert!(harness.state().editor.can_undo(), "the deletion is an undo step");

    chat_with_text(&mut harness, "uma pergunta");
    key(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    let composer = harness.get_by_role(Role::MultilineTextInput).value();
    assert_ne!(composer.as_deref(), Some("uma pergunta"), "Ctrl+Z is the composer's own undo: it takes back what was typed (composer: {composer:?})");
    assert_eq!(harness.state().editor.doc.map.nodes.len(), after_delete, "Ctrl+Z in the composer undid the map (composer: {composer:?})");
    assert!(harness.state().editor.can_undo(), "the map's undo step is still there");

    key(&mut harness, egui::Key::Y, egui::Modifiers::COMMAND);
    assert!(harness.state().editor.can_undo(), "Ctrl+Y in the composer redid the map");
    assert_eq!(harness.state().editor.doc.map.nodes.len(), after_delete, "Ctrl+Y in the composer changed the map");
}

/// With the command palette open, the map takes no key: the palette is drawn after the map, so
/// a Tab that takes the focus out of its box must not hand the next key to the map behind it.
#[test]
fn the_map_takes_no_key_while_the_palette_is_open() {
    let mut harness = harness(app(craft_window::Frame::Desktop, "pt-br", ThemeMode::Light), MEDIUM);
    harness.run_steps(3);
    click_card(&mut harness, "qSOFA");
    let before = harness.state().editor.doc.map.nodes.len();
    harness.state_mut().show_for_test(Shell::Palette);
    harness.run_steps(2);
    assert!(harness.state().palette_open());
    assert!(harness.ctx.text_edit_focused(), "the palette focuses its box when it opens");

    key(&mut harness, egui::Key::Tab, egui::Modifiers::NONE);
    assert!(!harness.ctx.text_edit_focused(), "Tab took the focus out of the palette's box");
    assert!(harness.state().palette_open(), "the palette is still open");

    key(&mut harness, egui::Key::Backspace, egui::Modifiers::NONE);

    assert!(harness.state().palette_open(), "the palette is still open");
    assert_eq!(harness.state().editor.doc.map.nodes.len(), before, "Backspace deleted a card behind the palette");
    assert!(harness.state().editor.editing.is_none(), "a key opened a text field behind the palette");

    // Esc still closes the palette (it is the palette's own key), and the map has its keys back.
    key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!harness.state().palette_open(), "Esc closes the palette");
    key(&mut harness, egui::Key::Backspace, egui::Modifiers::NONE);
    assert_eq!(harness.state().editor.doc.map.nodes.len(), before - 1, "with the palette closed, Backspace deletes the selected card");
}

/// Checks that a key changed nothing on the map: same cards, same selection, no text field.
fn assert_map_untouched(harness: &Harness<'_, LocalApp>, before: usize, selected: &Option<String>, what: &str) {
    let editor = &harness.state().editor;
    assert_eq!(editor.doc.map.nodes.len(), before, "{what}: the map changed behind what is open");
    assert_eq!(&editor.selected, selected, "{what}: the selection moved behind what is open");
    assert!(editor.editing.is_none(), "{what}: a text field opened behind what is open");
}

/// The keys that act on the map, with what each does when it gets there.
const MAP_KEYS: [(&str, egui::Key); 3] = [("Tab", egui::Key::Tab), ("Down", egui::Key::ArrowDown), ("Backspace", egui::Key::Backspace)];

/// The unsaved-changes prompt (a modal): no key reaches the map behind it.
#[test]
fn the_map_takes_no_key_while_the_unsaved_prompt_is_up() {
    let mut harness = harness(app(craft_window::Frame::Desktop, "pt-br", ThemeMode::Light), MEDIUM);
    harness.run_steps(3);
    click_card(&mut harness, "qSOFA");
    let selected = harness.state().editor.selected.clone();
    let before = harness.state().editor.doc.map.nodes.len();
    harness.state_mut().show_for_test(Shell::Unsaved);
    harness.run_steps(3);
    assert!(harness.state().close_confirming(), "the prompt is up");

    for (name, pressed) in MAP_KEYS {
        key(&mut harness, pressed, egui::Modifiers::NONE);
        assert!(harness.state().close_confirming(), "{name} closed the prompt");
        assert_map_untouched(&harness, before, &selected, name);
    }
}

/// The help window: no key reaches the map behind it.
#[test]
fn the_map_takes_no_key_while_the_help_window_is_open() {
    let mut harness = harness(app(craft_window::Frame::Desktop, "pt-br", ThemeMode::Light), MEDIUM);
    harness.run_steps(3);
    click_card(&mut harness, "qSOFA");
    let selected = harness.state().editor.selected.clone();
    let before = harness.state().editor.doc.map.nodes.len();
    harness.state_mut().editor_mut().help_open = true;
    harness.run_steps(3);
    assert!(harness.state().editor.help_open, "the help window is open");

    for (name, pressed) in MAP_KEYS {
        key(&mut harness, pressed, egui::Modifiers::NONE);
        assert!(harness.state().editor.help_open, "{name} closed the help window");
        assert_map_untouched(&harness, before, &selected, name);
    }
}

/// A menu of the bar open over the map: no key reaches the map behind it.
#[test]
fn the_map_takes_no_key_while_a_menu_is_open() {
    let mut harness = harness(app(craft_window::Frame::Desktop, "en", ThemeMode::Light), MEDIUM);
    harness.run_steps(3);
    click_card(&mut harness, "qSOFA");
    let selected = harness.state().editor.selected.clone();
    let before = harness.state().editor.doc.map.nodes.len();
    harness.get_by_label("Edit").click();
    harness.run_steps(3);
    assert!(egui::Popup::is_any_open(&harness.ctx), "the menu is an open popup");

    for (name, pressed) in MAP_KEYS {
        key(&mut harness, pressed, egui::Modifiers::NONE);
        assert!(egui::Popup::is_any_open(&harness.ctx), "{name} closed the menu");
        assert_map_untouched(&harness, before, &selected, name);
    }

    // Esc closes the menu (it is the menu's own key), and the map has its keys again.
    key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!egui::Popup::is_any_open(&harness.ctx), "Esc closes the menu");
    harness.run_steps(2);
    key(&mut harness, egui::Key::Backspace, egui::Modifiers::NONE);
    assert_eq!(harness.state().editor.doc.map.nodes.len(), before - 1, "with the menu closed, Backspace deletes the selected card");
}

/// In Settings, Esc closes the open language dropdown first and the window only on the next Esc.
#[test]
fn esc_closes_the_dropdown_first_and_settings_second() {
    let mut harness = harness_any_script(app(craft_window::Frame::Desktop, "pt-br", ThemeMode::Light), MEDIUM);
    harness.run_steps(3);
    harness.state_mut().show_for_test(Shell::Settings);
    harness.run_steps(3);
    assert!(harness.state().settings_or_about_open());
    harness.get_by_role(Role::ComboBox).click();
    harness.run_steps(3);
    assert!(egui::Popup::is_any_open(&harness.ctx), "the language dropdown is open");

    key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!egui::Popup::is_any_open(&harness.ctx), "Esc closes the dropdown");
    assert!(harness.state().settings_or_about_open(), "Esc that closed the dropdown also closed Settings");

    key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!harness.state().settings_or_about_open(), "the next Esc closes Settings");
}

/// With the keyboard on a button of the menu bar, Tab moves the focus and does not also create a
/// card. With the keyboard on the map's side - the canvas after a click on a card, or nothing
/// focused at all - Tab is the map's and creates the child.
#[test]
fn tab_leaves_a_focused_widget_without_creating_a_card() {
    let mut on_button = harness(app(craft_window::Frame::Desktop, "en", ThemeMode::Light), MEDIUM);
    on_button.run_steps(3);
    click_card(&mut on_button, "qSOFA");
    let before = on_button.state().editor.doc.map.nodes.len();
    let selected = on_button.state().editor.selected.clone();
    on_button.get_by_label("File").focus();
    on_button.run_steps(2);
    assert!(on_button.ctx.memory(|m| m.focused()).is_some(), "a button of the menu bar has the keyboard");

    key(&mut on_button, egui::Key::Tab, egui::Modifiers::NONE);
    assert_map_untouched(&on_button, before, &selected, "Tab from a menu bar button");

    // The canvas holds the keyboard (the click on the card gave it): Tab is the map's.
    let mut on_canvas = harness(app(craft_window::Frame::Desktop, "en", ThemeMode::Light), MEDIUM);
    on_canvas.run_steps(3);
    click_card(&mut on_canvas, "qSOFA");
    assert!(on_canvas.ctx.memory(|m| m.focused()).is_some(), "the click left the keyboard on the canvas");
    let before = on_canvas.state().editor.doc.map.nodes.len();
    key(&mut on_canvas, egui::Key::Tab, egui::Modifiers::NONE);
    assert_eq!(on_canvas.state().editor.doc.map.nodes.len(), before + 1, "Tab with the canvas holding the keyboard creates the child");
    assert!(on_canvas.state().editor.editing.is_some(), "and opens it for typing");

    // Nothing focused at all (no click yet): the same Tab is the map's.
    let mut nothing = harness(app(craft_window::Frame::Desktop, "en", ThemeMode::Light), MEDIUM);
    nothing.run_steps(3);
    assert!(nothing.ctx.memory(|m| m.focused()).is_none(), "nothing has the keyboard");
    let before = nothing.state().editor.doc.map.nodes.len();
    key(&mut nothing, egui::Key::Tab, egui::Modifiers::NONE);
    assert_eq!(nothing.state().editor.doc.map.nodes.len(), before + 1, "Tab with nothing focused creates the child");
    assert!(nothing.state().editor.editing.is_some(), "and opens it for typing");
}

/// The update prompt (found at start, or by a check): no key reaches the map behind it.
#[test]
fn the_map_takes_no_key_while_the_update_prompt_is_up() {
    let mut app = app(craft_window::Frame::Desktop, "pt-br", ThemeMode::Light);
    app.updater_hermetic();
    let mut harness = harness(app, MEDIUM);
    harness.run_steps(3);
    click_card(&mut harness, "qSOFA");
    let selected = harness.state().editor.selected.clone();
    let before = harness.state().editor.doc.map.nodes.len();
    harness.state_mut().offer_update_for_test();
    harness.run_steps(3);
    assert!(harness.state().update_prompt_open(), "the update prompt is up");

    for (name, pressed) in MAP_KEYS {
        key(&mut harness, pressed, egui::Modifiers::NONE);
        assert!(harness.state().update_prompt_open(), "{name} closed the update prompt");
        assert_map_untouched(&harness, before, &selected, name);
    }
}

/// A click on a card, or on an empty stretch of the map, asks for the canvas to hold the keyboard,
/// whatever had it before (a button of the bar): Tab right after is the map's and creates the
/// child. (The arrow is no indicator here: from this card of the fixture it has nowhere to go.)
#[test]
fn a_click_on_the_map_makes_the_canvas_hold_the_keyboard() {
    let mut harness = harness(app(craft_window::Frame::Desktop, "en", ThemeMode::Light), MEDIUM);
    harness.run_steps(3);
    harness.get_by_label("File").focus();
    harness.run_steps(2);
    assert!(harness.ctx.memory(|m| m.focused()).is_some(), "a button of the menu bar has the keyboard");

    click_card(&mut harness, "qSOFA");
    let canvas = harness.state().editor.map_widgets()[0];
    assert_eq!(harness.ctx.memory(|m| m.focused()), Some(canvas), "the click on the card did not give the canvas the keyboard");
    let before = harness.state().editor.doc.map.nodes.len();

    key(&mut harness, egui::Key::Tab, egui::Modifiers::NONE);
    assert_eq!(harness.state().editor.doc.map.nodes.len(), before + 1, "Tab after the click did not create a child");

    // The bar again, then an empty stretch of the map: the canvas takes the keyboard too.
    harness.get_by_label("File").focus();
    harness.run_steps(2);
    let empty = {
        let editor = &harness.state().editor;
        editor.rect.left_top() + egui::vec2(editor.rect.width() * 0.5, editor.rect.height() * 0.12)
    };
    click_at(&mut harness, empty);
    assert_eq!(harness.ctx.memory(|m| m.focused()), Some(canvas), "the click on the empty map did not give the canvas the keyboard");
    assert!(harness.state().editor.selected.is_none(), "a click on the empty map clears the selection");

    // Esc still clears the focus (the canvas's focus filter leaves Escape to egui).
    key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(harness.ctx.memory(|m| m.focused()).is_none(), "Esc did not clear the keyboard");

    // The minimap too: a click on it is the user pointing at the map.
    harness.get_by_label("File").focus();
    harness.run_steps(2);
    let minimap = {
        let editor = &harness.state().editor;
        crate::canvas::minimap_rect(editor.rect).expect("the window is big enough for a minimap")
    };
    click_at(&mut harness, minimap.center());
    assert_eq!(harness.ctx.memory(|m| m.focused()), Some(canvas), "a click on the minimap did not give the canvas the keyboard");
}

/// Settings or About open: no key reaches the map behind the window (not Tab, not the arrows, not
/// Backspace), and Esc closes the window (egui leaves Esc to `Modal`; these are `Window`s).
#[test]
fn the_map_takes_no_key_while_settings_or_about_is_open() {
    for (name, what) in [("Settings", Shell::Settings), ("About", Shell::About)] {
        let mut harness = harness(app(craft_window::Frame::Desktop, "pt-br", ThemeMode::Light), MEDIUM);
        harness.run_steps(3);
        click_card(&mut harness, "qSOFA");
        let selected = harness.state().editor.selected.clone();
        let before = harness.state().editor.doc.map.nodes.len();
        harness.state_mut().show_for_test(what);
        harness.run_steps(3);
        assert!(harness.state().settings_or_about_open(), "{name} is open");

        for (key_name, pressed) in [("Tab", egui::Key::Tab), ("Down", egui::Key::ArrowDown), ("Backspace", egui::Key::Backspace)] {
            key(&mut harness, pressed, egui::Modifiers::NONE);
            assert!(harness.state().settings_or_about_open(), "{name}: {key_name} closed the window");
            let editor = &harness.state().editor;
            assert_eq!(editor.doc.map.nodes.len(), before, "{name}: {key_name} changed the map behind the window");
            assert_eq!(editor.selected, selected, "{name}: {key_name} moved the selection behind the window");
            assert!(editor.editing.is_none(), "{name}: {key_name} opened a text field behind the window");
        }

        key(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
        assert!(!harness.state().settings_or_about_open(), "{name}: Esc does not close the window");
        assert!(harness.state().editor.editing.is_none(), "{name}: Esc reached the map");
        // The window gone, the map has its keys again.
        key(&mut harness, egui::Key::Backspace, egui::Modifiers::NONE);
        assert_eq!(harness.state().editor.doc.map.nodes.len(), before - 1, "{name} closed: Backspace deletes the selected card again");
    }
}

/// A right click on a card, like a left one, is the user pointing at the map: the canvas takes
/// the keyboard (the card's menu opens on top of it).
#[test]
fn a_right_click_on_a_card_makes_the_canvas_hold_the_keyboard() {
    let mut harness = harness(app(craft_window::Frame::Desktop, "en", ThemeMode::Light), MEDIUM);
    harness.run_steps(3);
    harness.get_by_label("File").focus();
    harness.run_steps(2);
    assert!(harness.ctx.memory(|m| m.focused()).is_some(), "a button of the menu bar has the keyboard");

    right_click_card(&mut harness, "qSOFA");

    let canvas = harness.state().editor.map_widgets()[0];
    assert!(harness.state().editor.menu.is_some(), "the right click opens the card's menu");
    assert_eq!(harness.ctx.memory(|m| m.focused()), Some(canvas), "the right click did not give the canvas the keyboard");
}

/// The widget with the keyboard outside the map area does not take it away from the map: with the
/// focus on a button of the menu bar, Delete still acts on the selected card. (A click on a
/// toolbar button leaves nothing focused; it is here to keep it so.)
#[test]
fn a_widget_outside_the_map_area_does_not_mute_the_map() {
    let mut harness = harness(app(craft_window::Frame::Desktop, "en", ThemeMode::Light), MEDIUM);
    harness.run_steps(3);
    click_card(&mut harness, "qSOFA");
    let before = harness.state().editor.doc.map.nodes.len();
    // The window is in English: the toolbar's label is looked up by its text.
    harness.get_by_label("Fit to window").click();
    harness.run_steps(3);
    assert!(harness.ctx.memory(|m| m.focused()).is_none(), "a toolbar click leaves the keyboard free");

    harness.get_by_label("File").focus();
    harness.run_steps(2);
    assert!(harness.ctx.memory(|m| m.focused()).is_some(), "a button of the menu bar has the keyboard");

    key(&mut harness, egui::Key::Delete, egui::Modifiers::NONE);

    assert_eq!(harness.state().editor.doc.map.nodes.len(), before - 1, "Delete with the menu bar focused did not reach the map");
}

/// The shell's own windows and the palette, over the open map.
#[test]
fn the_shell_windows_fit_the_narrow_window() {
    for (frame_name, frame) in frames() {
        for what in [Shell::About, Shell::Settings, Shell::Palette, Shell::Unsaved] {
            for (language, mode, theme) in [("en", ThemeMode::Dark, "dark"), ("pt-br", ThemeMode::Light, "light")] {
                for size in [NARROW, MEDIUM] {
                    let mut app = app(frame.clone(), language, mode);
                    app.show_for_test(what);
                    let mut harness = harness(app, size);
                    harness.run_steps(4);
                    shot(&mut harness, &format!("{frame_name}-{what:?}-{language}-{theme}-{}", size.x).to_lowercase());
                    assert_inside(&harness, size, &format!("{frame_name} {what:?} {language}"));
                }
            }
        }
    }
}

/// The top bar over the map, both frames, both widths, both themes.
#[test]
fn the_top_bar_fits_the_narrow_window() {
    for (frame_name, frame) in frames() {
        for size in [NARROW, MEDIUM] {
            for (mode, theme) in [(ThemeMode::Light, "light"), (ThemeMode::Dark, "dark")] {
                let mut harness = harness(app(frame.clone(), "pt-br", mode), size);
                harness.run_steps(4);
                shot(&mut harness, &format!("{frame_name}-map-{theme}-{}", size.x));
                assert_inside(&harness, size, &format!("{frame_name} {theme}"));
                assert_bar_buttons(&harness, size, &format!("{frame_name} {theme} {}", size.x));
            }
        }
    }
}

/// The editor's own windows (note, move to, help, generate, rewrite, connections, chat, the card menu), opened
/// over the map at the minimum width.
#[test]
fn every_dialog_fits_the_narrow_window() {
    type Open = fn(&mut crate::edit::Editor, &str);
    let dialogs: [(&str, Open); 8] = [
        ("help", |e, _| e.help_open = true),
        ("note", |e, id| e.open_note(id)),
        ("move", |e, id| e.open_move_to(id)),
        ("generate", |e, _| e.ai.generate_topic = Some("Sepse".into())),
        ("rewrite", |e, id| e.ai.rewrite = Some((id.into(), String::new()))),
        ("connections", |e, _| e.ai.settings_open = true),
        ("chat", |e, _| e.ai.chat_open = true),
        ("card-menu", |e, id| e.menu = Some(crate::edit::MenuState { node: id.into(), at: egui::pos2(400.0, 300.0) })),
    ];
    for (frame_name, frame) in frames() {
        for (name, open) in dialogs {
            for (language, mode, theme) in [("en", ThemeMode::Dark, "dark"), ("pt-br", ThemeMode::Light, "light")] {
                let mut app = app(frame.clone(), language, mode);
                let sofa = app.editor_mut().doc.map.nodes.iter().find(|n| n.text.starts_with("Pontuação SOFA")).map(|n| n.id.clone());
                // The long-named node is the interesting subject of the dialogs that name one.
                let id = sofa.expect("the long node exists");
                open(app.editor_mut(), &id);
                let mut harness = harness_with(app, NARROW, true);
                harness.run_steps(4);
                shot(&mut harness, &format!("{frame_name}-{name}-{language}-{theme}-900"));
                assert_inside(&harness, NARROW, &format!("{frame_name} {name} {language}"));
            }
        }
    }
}
