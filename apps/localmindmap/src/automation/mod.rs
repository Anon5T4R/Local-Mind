//! Agent-native Local Mindmap: the commands the app offers an agent, a headless session that runs
//! them, the MCP server on top of it and the control channel of the running window.
//!
//! The plumbing is the suite's `craft-mcp` (`libs/craft-libs/crates/mcp`, source `local-libs`):
//! the stdio server, the `run --script` steps with `"$N"` references, the
//! `--automation-read-root`/`--automation-write-root` flags and `--connect PORT`. What this module
//! adds is Local Mindmap's own: the command catalogue ([`catalogue`]), the session ([`session`]) over
//! the editor the app already has, the MCP handler ([`mcp`]) and the window's loopback control
//! channel ([`control`]).
//!
//! A mind map is the document: the headless session opens, edits, saves and exports one exactly
//! through the editor's own commands, so an agent and the user share one implementation. What needs
//! the window (the viewport, the dialogs, the AI panel, the updater) says `appOnly`.

pub mod catalogue;
pub mod cli;
pub mod control;
pub mod launch;
pub mod mcp;
pub mod session;
