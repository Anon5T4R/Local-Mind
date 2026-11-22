//! Local Mind - mind maps (Rust + egui, no account, no telemetry, no third-party servers).
//!
//! Usage: `localmind [--version] [--help] [<map>]`. A map path opens at start, which is what the
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
localmind - mind maps of the Local suite

USAGE: localmind [OPTIONS] [FILE]

OPTIONS:
  -h, --help       print this help and exit
      --version    print the version and exit

FILE is a .tmind map to open; the same format the original LocalMind (Electron) wrote, so the maps
you already have open as they are.

The interface language follows the system's; the About window (Help menu) lets you pick another
one and checks for updates. Updates install from the GitHub releases of Anon5T4R/Local-Mind.

SCRIPTS AND AI AGENTS:
  localmind commands [--search TEXT]     every command as JSON (the palette's ids)
  localmind mcp [--connect PORT] [--headless]   the MCP server on stdio; --connect drives a running
                                         app (`localmind --control PORT`)
  localmind run --script job.json        headless steps, one JSON result per line
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

    // Who draws the window frame: this is the suite's one decision (the desktop on Windows, macOS
    // and X11; on Wayland, the compositor when it offers server-side decorations, the app on GNOME's
    // Mutter). `LOCALMIND_TITLEBAR=app|system` overrides it.
    let choice = craft_window::Choice::probe(craft_window::Prefix::new("LOCALMIND"));
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
        assert_eq!(APP_ID, format!("com.{}.app", APP.id));
    }

    #[test]
    fn help_documents_the_flags_the_parser_accepts() {
        for flag in ["-h", "--help", "--version", "FILE"] {
            assert!(HELP.contains(flag), "{flag} is parsed but not documented");
        }
    }
}
