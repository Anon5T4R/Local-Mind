//! Local Mindmap - mind maps (Rust + egui, no account, no telemetry, no third-party servers).
//!
//! Usage: `localmindmap [--version] [--help] [<map>]`. A map path opens at start, which is what the
//! file association and "Open with" pass.
//!
//! On Windows, release builds are GUI-subsystem programs: launching the app opens no console
//! window. Their `--help` / `--version` output then has no console to go to; use a debug build or
//! a terminal that captures the child's output.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod ai;
mod app;
mod automation;
mod canvas;
mod close;
mod doc;
mod edit;
mod export;
mod fonts;
mod i18n;
mod legacy;
mod live;
mod pickers;
mod png;
mod prefs;
mod suite_entry;
mod theme;

#[cfg(test)]
mod ui_guards;
#[cfg(test)]
mod ui_tests;

use app::LocalApp;
use suite_entry::APP;

/// Reverse-DNS app id: the Wayland app id, the `.desktop` file and the hicolor icon name. It must
/// match `packaging/linux/com.localmind.app.desktop` and the MSI's icon/registry names.
const APP_ID: &str = "com.localmind.app";

/// The window, taskbar and Dock icon, committed by `packaging/icons.sh`. The 256 px hicolor entry
/// is the runtime icon; the ICO built from the same SVG is the executable's (see `build.rs`).
fn app_icon() -> egui::IconData {
    let png: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/256x256/apps/com.localmind.app.png");
    eframe::icon_data::from_png_bytes(png).unwrap_or_default()
}

const HELP: &str = "\
localmindmap - mind maps of the Local suite

USAGE: localmindmap [OPTIONS] [FILE]

OPTIONS:
  -h, --help       print this help and exit
      --version    print the version and exit

FILE is a .tmind map to open; the same format the original LocalMind (Electron) wrote, so the maps
you already have open as they are.

The interface language follows the system's; the About window (Help menu) lets you pick another
one and checks for updates. Updates install from the GitHub releases of Styx-Studios/Local-Mindmap.

SCRIPTS AND AI AGENTS:
  localmindmap commands [--search TEXT]     every command as JSON (the palette's ids)
  localmindmap mcp [--connect PORT] [--headless]   the MCP server on stdio; --connect drives a running
                                         app (`localmindmap --control PORT`)
  localmindmap run --script job.json        headless steps, one JSON result per line
";

fn main() -> eframe::Result {
    // Windows: the update helper is a copy of this program started with `--finish-update`. Nothing
    // else runs in that mode, and nothing at all happens off Windows.
    if let Some(result) = craft_update::self_update::finish(&APP, &std::env::args_os().skip(1).collect::<Vec<_>>()) {
        std::process::exit(i32::from(result.is_err()));
    }

    let mut initial: Option<std::path::PathBuf> = None;
    let mut unknown: Vec<String> = Vec::new();
    // Agent-native: `mcp`, `run` e `commands` saem antes da janela; o `--control PORT` e as
    // raízes de automação saem dos args antes do parser da janela (que lê o resto como sempre).
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = automation::cli::run(&args) {
        std::process::exit(code);
    }
    let launch = match automation::launch::parse(&args) {
        Ok(launch) => launch,
        Err(e) => {
            eprintln!("{}: {e}", APP.id);
            std::process::exit(2);
        }
    };
    let roots = match launch.roots.roots() {
        Ok(roots) => roots,
        Err(e) => {
            eprintln!("{}: {e}", APP.id);
            std::process::exit(2);
        }
    };
    // O canal de controle escuta em loopback antes da janela: porta ocupada falha aqui.
    let control = match launch.control.map(automation::control::Control::start) {
        None => None,
        Some(Ok(control)) => {
            eprintln!("{}: control channel on 127.0.0.1:{}", APP.id, control.port());
            Some(control)
        }
        Some(Err(e)) => {
            eprintln!("{}: {e}", APP.id);
            std::process::exit(1);
        }
    };
    for arg in &launch.args {
        match arg.as_str() {
            "--version" => {
                println!("{} {}", APP.id, env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(());
            }
            other if other.starts_with('-') => unknown.push(other.to_string()),
            // One map per window: a second path is a mistake, not something to guess about.
            other if initial.is_some() => unknown.push(other.to_string()),
            other => initial = Some(std::path::PathBuf::from(other)),
        }
    }
    if !unknown.is_empty() {
        for option in &unknown {
            eprintln!("{}: unexpected argument `{option}`", APP.id);
        }
        eprintln!("\n{HELP}");
        std::process::exit(2);
    }

    // The product was renamed (Local Mind -> Local Mindmap): move the old-named folders over once
    // and copy the keychain keys, before the window reads any of them (preferences, AI connections,
    // the updater's folders). The headless paths above read maps only, and the tests that run this
    // binary drive those paths, so a test run never moves a user's real folders.
    legacy::migrate_all();

    // Who draws the window frame: this is the suite's one decision (the desktop on Windows, macOS
    // and X11; on Wayland, the compositor when it offers server-side decorations, the app on GNOME's
    // Mutter). `LOCALMINDMAP_TITLEBAR=app|system` overrides it.
    // The old `LOCALMIND_TITLEBAR` still works when the new variable isn't set.
    let prefix = if std::env::var_os("LOCALMINDMAP_TITLEBAR").is_none() && legacy::env_var_os("TITLEBAR").is_some() {
        craft_window::Prefix::new("LOCALMIND")
    } else {
        craft_window::Prefix::new("LOCALMINDMAP")
    };
    let choice = craft_window::Choice::probe(prefix);
    let viewport = craft_window::decorate(
        egui::ViewportBuilder::default().with_title(APP.name).with_min_inner_size([900.0, 600.0]).with_icon(app_icon()).with_app_id(APP_ID),
        choice.frame(),
    );
    let frame = choice.into_frame();
    eframe::run_native(
        APP.name,
        eframe::NativeOptions { viewport, ..Default::default() },
        Box::new(move |cc| {
            let mut app = LocalApp::new(cc, frame, initial);
            app.attach_control(control, roots);
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_id_is_reverse_dns_and_matches_the_desktop_file_name() {
        assert_eq!(APP_ID, "com.localmind.app");
        // The suite catalog keeps the packaging id the product shipped with (Local Mind's), as it
        // does for the other renamed apps, so an installed copy is still the same app to the system.
        assert_eq!(APP_ID, APP.bundle_id());
    }

    #[test]
    fn help_documents_the_flags_the_parser_accepts() {
        for flag in ["-h", "--help", "--version", "FILE"] {
            assert!(HELP.contains(flag), "{flag} is parsed but not documented");
        }
    }

    /// The packaging id is spent in more than one place: the `.desktop` file has to exist, start the
    /// binary this crate builds and name the icon after `APP_ID`.
    #[test]
    fn the_desktop_file_starts_this_binary() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/linux/com.localmind.app.desktop");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(text.contains(&format!("Exec={} %F", APP.id)), "the entry does not start `{}`", APP.id);
        assert!(text.contains(&format!("Icon={APP_ID}")), "the icon is not `{APP_ID}`");
    }
}
