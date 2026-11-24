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
