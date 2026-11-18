//! The named themes: the six palettes the original app shipped, mapped onto egui's style and onto
//! the canvas's own colours.
//!
//! The values are the original app's `styles.css` variables, translated one to one (`--bg` is the
//! canvas, `--panel` the cards, `--accent` the selection and the root's bar, `--accent-2` the other
//! cards' bars, `--canvas-dots` the background grid, `--minimap-*` the minimap). The names are the
//! original ones: "default" is its dark theme, and the other five are the ones its Settings offered.

use egui::Color32;

/// One of the app's named themes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// The dark theme the app ships with.
    #[default]
    Default,
    /// The suite's neutral light theme (craft's greys and blue): the counterpart of `Default`.
    Light,
    Nature,
    DarkBlue,
    CalmGreen,
    PastelPink,
    PunkPrincess,
}

/// The id the preferences file keeps for "System" (follow the OS light/dark setting).
pub const SYSTEM_ID: &str = "system";

/// What the Theme menu picks: the OS setting, or one of the named palettes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    System,
    Named(Theme),
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

impl Theme {
    /// The six themes, in the order the original app listed them.
    pub const ALL: [Theme; 7] =
        [Theme::Default, Theme::Light, Theme::Nature, Theme::DarkBlue, Theme::CalmGreen, Theme::PastelPink, Theme::PunkPrincess];

    /// The stable id used in the preferences file and in the menu.
    pub fn id(self) -> &'static str {
        match self {
            Theme::Default => "default",
            Theme::Light => "light",
            Theme::Nature => "nature",
            Theme::DarkBlue => "darkblue",
            Theme::CalmGreen => "calmgreen",
            Theme::PastelPink => "pastelpink",
            Theme::PunkPrincess => "punkprincess",
        }
    }

    /// The theme a preferences file names; an unknown id is the default theme, not an error (a
    /// preferences file written by a newer build must not keep the app from opening).
    pub fn from_id(id: &str) -> Theme {
        Theme::ALL.into_iter().find(|theme| theme.id() == id).unwrap_or_default()
    }

    /// The theme "System" resolves to: the dark default when the OS is dark, the light theme otherwise.
    pub fn for_system(dark: bool) -> Theme {
        if dark { Theme::Default } else { Theme::Light }
    }

    /// The English source string the catalogs translate.
    pub fn label(self) -> &'static str {
        match self {
            Theme::Default => "Dark (default)",
            Theme::Light => "Light",
            Theme::Nature => "Nature",
            Theme::DarkBlue => "Dark blue",
            Theme::CalmGreen => "Calm green",
            Theme::PastelPink => "Pastel pink",
            Theme::PunkPrincess => "PunkPrincess",
        }
    }

    pub fn colors(self) -> Colors {
        match self {
            Theme::Default => Colors {
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
            Theme::Nature => Colors {
                canvas: rgb(0xee, 0xf1, 0xe9),
                surface: rgb(0xf8, 0xfa, 0xf5),
                panel: rgb(0xf8, 0xfa, 0xf5),
                border: rgb(0xdb, 0xe3, 0xd4),
                text: rgb(0x2e, 0x3a, 0x2e),
                muted: rgb(0x64, 0x70, 0x64),
                accent: rgb(0x7a, 0x9e, 0x7e),
                accent_second: rgb(0xe8, 0xb4, 0xc8),
                selection_bg: rgb(0xe6, 0xef, 0xe1),
                dots: rgb(0xcc, 0xd6, 0xc6),
                minimap_node: rgb(0xdf, 0xe7, 0xda),
                minimap_stroke: rgb(0xb9, 0xc6, 0xb2),
                minimap_mask: rgba(0xee, 0xf1, 0xe9, 0.6),
                dark: false,
            },
            Theme::DarkBlue => Colors {
                canvas: rgb(0x16, 0x20, 0x2e),
                surface: rgb(0x1d, 0x2a, 0x3c),
                panel: rgb(0x1d, 0x2a, 0x3c),
                border: rgb(0x2d, 0x3f, 0x59),
                text: rgb(0xe2, 0xe8, 0xf0),
                muted: rgb(0x85, 0x95, 0xad),
                accent: rgb(0x3b, 0x82, 0xf6),
                accent_second: rgb(0x60, 0xa5, 0xfa),
                selection_bg: rgb(0x1e, 0x3a, 0x5f),
                dots: rgb(0x1d, 0x2a, 0x3d),
                minimap_node: rgb(0x2a, 0x3f, 0x5e),
                minimap_stroke: rgb(0x3d, 0x55, 0x78),
                minimap_mask: rgba(0x16, 0x20, 0x2e, 0.6),
                dark: true,
            },
            Theme::CalmGreen => Colors {
                canvas: rgb(0xea, 0xf0, 0xe9),
                surface: rgb(0xf7, 0xfa, 0xf6),
                panel: rgb(0xf7, 0xfa, 0xf6),
                border: rgb(0xd5, 0xe0, 0xd3),
                text: rgb(0x24, 0x30, 0x2a),
                muted: rgb(0x61, 0x70, 0x66),
                accent: rgb(0x4f, 0x7d, 0x5e),
                accent_second: rgb(0x6f, 0xa0, 0x7e),
                selection_bg: rgb(0xdc, 0xeb, 0xd9),
                dots: rgb(0xc9, 0xd5, 0xc6),
                minimap_node: rgb(0xdb, 0xe4, 0xd8),
                minimap_stroke: rgb(0xb4, 0xc4, 0xb0),
                minimap_mask: rgba(0xea, 0xf0, 0xe9, 0.6),
                dark: false,
            },
            Theme::PastelPink => Colors {
                canvas: rgb(0xf8, 0xeb, 0xf0),
                surface: rgb(0xfd, 0xf6, 0xf9),
                panel: rgb(0xfd, 0xf6, 0xf9),
                border: rgb(0xec, 0xd3, 0xde),
                text: rgb(0x3a, 0x2a, 0x30),
                muted: rgb(0x79, 0x67, 0x71),
                accent: rgb(0xe8, 0x79, 0x9f),
                accent_second: rgb(0xf4, 0xa9, 0xc4),
                selection_bg: rgb(0xfb, 0xe4, 0xee),
                dots: rgb(0xec, 0xd3, 0xde),
                minimap_node: rgb(0xf3, 0xdc, 0xe6),
                minimap_stroke: rgb(0xdc, 0xb8, 0xc8),
                minimap_mask: rgba(0xf8, 0xeb, 0xf0, 0.6),
                dark: false,
            },
            Theme::PunkPrincess => Colors {
                canvas: rgb(0x1c, 0x12, 0x28),
                surface: rgb(0x25, 0x17, 0x33),
                panel: rgb(0x25, 0x17, 0x33),
                border: rgb(0x3a, 0x28, 0x50),
                text: rgb(0xf0, 0xe6, 0xff),
                muted: rgb(0x9a, 0x86, 0xb5),
                accent: rgb(0xff, 0x2d, 0x95),
                accent_second: rgb(0x2d, 0xe2, 0xff),
                selection_bg: rgb(0x33, 0x14, 0x3a),
                dots: rgb(0x25, 0x16, 0x38),
                minimap_node: rgb(0x34, 0x1f, 0x4a),
                minimap_stroke: rgb(0x4a, 0x2f, 0x66),
                minimap_mask: rgba(0x1c, 0x12, 0x28, 0.6),
                dark: true,
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
    fn every_theme_has_its_own_id_and_colours() {
        for (i, theme) in Theme::ALL.iter().enumerate() {
            assert_eq!(Theme::from_id(theme.id()), *theme);
            for other in Theme::ALL.iter().skip(i + 1) {
                assert_ne!(theme.colors(), other.colors(), "{} and {} share a palette", theme.id(), other.id());
            }
        }
    }

    /// The default is the dark theme the app ships with, and an unknown id is not an error.
    #[test]
    fn an_unknown_id_falls_back_to_the_default_theme() {
        assert_eq!(Theme::from_id("default"), Theme::Default);
        assert_eq!(Theme::from_id("nao-existe"), Theme::Default);
        assert_eq!(Theme::from_id(""), Theme::Default);
        assert!(Theme::Default.colors().dark);
        assert!(!Theme::Nature.colors().dark);
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

    #[test]
    fn every_theme_is_labelled() {
        for theme in Theme::ALL {
            assert!(!theme.label().is_empty());
        }
    }
}
