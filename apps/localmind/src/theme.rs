//! The two palettes: the dark one the original app shipped as its default and the suite's neutral light one,
//! mapped onto egui's style and onto the canvas's own colours. Which one shows is the suite's three-state theme
//! (`craft_icons::ThemeMode`; Automatic follows the OS).
//!
//! The dark values are the original app's `styles.css` variables, translated one to one (`--bg` is the canvas,
//! `--panel` the cards, `--accent` the selection and the root's bar, `--accent-2` the other cards' bars,
//! `--canvas-dots` the background grid, `--minimap-*` the minimap).

use craft_icons::ThemeMode;
use egui::Color32;

/// One of the app's two palettes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// The dark palette the app ships with.
    #[default]
    Dark,
    /// The suite's neutral light palette (craft's greys and blue).
    Light,
}

/// The colours one theme gives to the window, the canvas and the minimap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colors {
    /// The canvas behind the cards (`--bg`).
    pub canvas: Color32,
    /// The toolbar/menu surface (`--bg-2`).
    pub surface: Color32,
    /// The cards and controls (`--panel`).
    pub panel: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent_second: Color32,
    /// The selection fill (`--sel-bg`), used behind selected text and menus.
    pub selection_bg: Color32,
    /// The background grid.
    pub dots: Color32,
    pub minimap_node: Color32,
    pub minimap_stroke: Color32,
    pub minimap_mask: Color32,
    /// Whether egui should start from its dark palette (light shapes need dark text and vice versa).
    pub dark: bool,
}

/// The mode a preferences file names. Ids of the palettes older builds offered fall to the side they were
/// on (a saved choice never breaks the app; the suite has only Automatic, Light and Dark).
pub fn mode_from_pref(id: &str) -> ThemeMode {
    match id {
        "light" | "nature" | "calmgreen" | "pastelpink" => ThemeMode::Light,
        "dark" | "default" | "darkblue" | "punkprincess" => ThemeMode::Dark,
        _ => ThemeMode::System,
    }
}

impl Theme {
    #[cfg(test)]
    pub const ALL: [Theme; 2] = [Theme::Dark, Theme::Light];

    /// A stable id (the QA renders' file names).
    #[cfg(test)]
    pub fn id(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }

    /// The palette `mode` shows, given whether the OS is dark (or says nothing).
    pub fn resolve(mode: ThemeMode, system_dark: bool) -> Theme {
        match mode {
            ThemeMode::System if system_dark => Theme::Dark,
            ThemeMode::System | ThemeMode::Light => Theme::Light,
            ThemeMode::Dark => Theme::Dark,
        }
    }

    pub fn colors(self) -> Colors {
        match self {
            Theme::Dark => Colors {
                canvas: rgb(0x1a, 0x1c, 0x1f),
                surface: rgb(0x20, 0x24, 0x2d),
                panel: rgb(0x25, 0x2a, 0x34),
                border: rgb(0x35, 0x3c, 0x4c),
                text: rgb(0xe6, 0xe9, 0xef),
                muted: rgb(0x8a, 0x93, 0xa6),
                accent: rgb(0x7c, 0x5c, 0xff),
                accent_second: rgb(0x3a, 0x8d, 0xde),
                selection_bg: rgb(0x23, 0x20, 0x38),
                dots: rgb(0x2d, 0x32, 0x3e),
                minimap_node: rgb(0x36, 0x40, 0x5c),
                minimap_stroke: rgb(0x46, 0x50, 0x62),
                minimap_mask: rgba(0x1a, 0x1c, 0x1f, 0.6),
                dark: true,
            },
            Theme::Light => Colors {
                // The suite's neutral light: the same grey ramp and blue accent the craft apps use.
                canvas: rgb(0xf2, 0xf4, 0xf7),
                surface: rgb(0xf7, 0xf9, 0xfb),
                panel: rgb(0xff, 0xff, 0xff),
                border: rgb(0xd5, 0xdb, 0xe3),
                text: rgb(0x1f, 0x29, 0x33),
                muted: rgb(0x5c, 0x6b, 0x7a),
                accent: rgb(0x25, 0x63, 0xeb),
                accent_second: rgb(0x0e, 0xa5, 0xe9),
                selection_bg: rgb(0xe3, 0xec, 0xfb),
                dots: rgb(0xd9, 0xde, 0xe6),
                minimap_node: rgb(0xdb, 0xe2, 0xea),
                minimap_stroke: rgb(0xb8, 0xc2, 0xcf),
                minimap_mask: rgba(0xf2, 0xf4, 0xf7, 0.6),
                dark: false,
            },
        }
    }

    /// Puts the theme on the egui context: every menu, dialog and text field follows it. The canvas
    /// reads [`Self::colors`] directly (it paints its own cards).
    pub fn apply(self, ctx: &egui::Context) {
        let colors = self.colors();
        let mut visuals = if colors.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        visuals.override_text_color = Some(colors.text);
        visuals.panel_fill = colors.surface;
        visuals.window_fill = colors.panel;
        visuals.extreme_bg_color = colors.canvas;
        visuals.faint_bg_color = colors.selection_bg;
        visuals.window_stroke = egui::Stroke::new(1.0, colors.border);
        visuals.widgets.noninteractive.bg_fill = colors.surface;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, colors.border);
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, colors.text);
        visuals.widgets.inactive.bg_fill = colors.panel;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, colors.border);
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, colors.text);
        visuals.widgets.hovered.bg_fill = colors.selection_bg;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, colors.accent);
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, colors.text);
        visuals.widgets.active.bg_fill = colors.selection_bg;
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, colors.accent);
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, colors.text);
        visuals.selection.bg_fill = colors.accent;
        visuals.selection.stroke = egui::Stroke::new(1.0, colors.text);
        visuals.hyperlink_color = colors.accent_second;
        visuals.warn_fg_color = colors.accent_second;
        visuals.error_fg_color = rgb(0xe7, 0x4c, 0x3c);
        ctx.set_visuals(visuals);
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, (a * 255.0).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_palettes_differ() {
        assert_ne!(Theme::Dark.colors(), Theme::Light.colors());
        assert!(Theme::Dark.colors().dark);
        assert!(!Theme::Light.colors().dark);
    }

    /// A preference written by an older build (one of the five extra palettes) keeps its side, and an
    /// unknown id follows the system instead of failing.
    #[test]
    fn old_and_unknown_theme_ids_map_onto_the_three_modes() {
        assert_eq!(mode_from_pref("system"), ThemeMode::System);
        assert_eq!(mode_from_pref("default"), ThemeMode::Dark);
        for dark in ["dark", "darkblue", "punkprincess"] {
            assert_eq!(mode_from_pref(dark), ThemeMode::Dark, "{dark}");
        }
        for light in ["light", "nature", "calmgreen", "pastelpink"] {
            assert_eq!(mode_from_pref(light), ThemeMode::Light, "{light}");
        }
        assert_eq!(mode_from_pref("nao-existe"), ThemeMode::System);
        assert_eq!(mode_from_pref(""), ThemeMode::System);
    }

    #[test]
    fn automatic_follows_the_system_and_the_others_do_not() {
        assert_eq!(Theme::resolve(ThemeMode::System, true), Theme::Dark);
        assert_eq!(Theme::resolve(ThemeMode::System, false), Theme::Light);
        assert_eq!(Theme::resolve(ThemeMode::Light, true), Theme::Light);
        assert_eq!(Theme::resolve(ThemeMode::Dark, false), Theme::Dark);
    }

    /// The cards must stand out from the canvas in every theme (the original app's pass of "the
    /// background is not the highest contrast on screen").
    #[test]
    fn the_cards_differ_from_the_canvas_in_every_theme() {
        for theme in Theme::ALL {
            let colors = theme.colors();
            assert_ne!(colors.canvas, colors.panel, "{}: card and canvas are the same colour", theme.id());
            assert_ne!(colors.text, colors.panel, "{}: text and card are the same colour", theme.id());
        }
    }
}
