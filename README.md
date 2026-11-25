<p align="center">
  <img src="assets/app-icon/hicolor/128x128/apps/com.localmind.app.png" width="96" height="96" alt="Local Mindmap icon">
</p>

<h1 align="center">Local Mindmap</h1>

<p align="center">
  Mind maps with a balanced layout on all four sides of the central idea, for Windows and Linux. Local files, no account, written in Rust.<br>
  <a href="#where-it-comes-from">Where it comes from</a> ·
  <a href="https://github.com/Styx-Studios/Local-Mindmap/releases/latest">Download</a> ·
  <a href="ROADMAP.md">Roadmap and status</a> ·
  <a href="#how-it-is-made">How it is made</a>
</p>

<!-- screenshot: a real map open, root in the centre with branches on all four sides, minimap in the corner; dark and light -->

Before 1.3.0 this app was called Local Mind; 1.3.0 renamed it to Local Mindmap, and the map files did not change.

## What it does

- Edit the map in place: double-click a card or press F2 to type in it, `Tab` for a child, `Enter` for a
  sibling, `Delete` for the card and its branch (the parent is selected), `Space` to collapse or expand.
  Arrows move the selection; `Alt` plus an arrow reorders, indents or outdents the selected card.
  `Ctrl+Z` / `Ctrl+Y` undo and redo.
- The card menu (right click) has edit text, new child and sibling, collapse, note, "Move to…", the side of a
  root child, the text size, the colour, the AI actions, colour branches and delete. Dragging the handle of the
  selected card changes its width, one undo step per drag.
- **Move to…**, from the card menu or the Edit menu, picks a new parent from a list and, when that parent is
  the root, the direction (left, right, up, down) in the same step. The `+` buttons around the root create a
  branch already on that side. Re-parenting by dragging was tried and removed: it passed the UI tests but was
  not reliable in real use.
- A note per card (menu, Note…), shown as a dot on the card and written by the Markdown and HTML exports.
- AI with your own provider (menu AI): generate a map from a topic or from a text, with a depth and a breadth;
  expand a card, expand it deep, explain it, or rewrite it with an instruction (four presets); and a chat that
  reads the map as context and inserts an answer as a node or a branch. `Ctrl+Enter` expands the selected card
  without going through the menu.
- Three theme modes in View → Theme: Automatic (follows the system), Light (the suite's neutral palette) and
  Dark (the palette the old app shipped). A theme saved by an older build falls to the closer of the two.
- A minimap in the bottom-right corner with the viewport marked; click or drag it to move the view. Drag the
  canvas to pan, scroll or pinch to zoom at the pointer, and View → Fit to window frames the whole map.
- The layout is balanced on four sides: the root in the centre, its branches fanning left, right, up and
  down, and a child without an explicit side goes to the lighter of left and right.
- Open, Save and Save As for `.tmind`; the dialogs also accept `.json`, since a map is plain JSON. The file
  association, a path on the command line and a file dropped on the window all open a map.
- Closing a window with unsaved changes cancels the close and asks: Save (which writes, or asks for a path
  first, and closes only if the write worked), Don't save, or Cancel.
- A command palette (`Ctrl+K`) with every menu command, the same commands on the command line
  (`localmindmap commands`, `localmindmap run --script job.json`), and an MCP server (`localmindmap mcp`) so
  an AI agent can drive the app.
- Interface in English, Portuguese (Brazil), Spanish and Japanese; it follows the system language and the
  choice is kept.
- An About window that shows the version and checks for updates, and Help → Keyboard and mouse, which lists
  the main shortcuts.

What is weak or missing is in the [roadmap](ROADMAP.md).

## What it opens and writes

A map is one `.tmind` file: JSON, with the map's `title`, `createdAt` and `updatedAt` in milliseconds, and a flat list of `nodes`. Every node names its `parentId`, and the root is the first node without one. A node may also carry `collapsed`, `color` (`#rrggbb`), `note`, `width`, `fontSize`, and a `side` (`left`, `right`, `up`, `down`) on the root's direct children.

That is the format LocalMind (the Electron app) wrote, so the maps you already have open as they are. A save writes a sibling file and renames it over the target, so an interrupted save leaves the previous file intact, and fields this build does not know are written back untouched.

- **Markdown**: a nested bullet outline, with the root's note.
- **HTML**: one self-contained page with the theme's colours.
- **SVG**: the map as vectors.
- **PNG**: the same drawing as the window, rendered off-screen at 2×.

## Network and privacy

- Your maps never leave the computer.
- On start the app asks GitHub whether a newer version exists, and installs it only if you accept. There is
  no setting to turn the check off yet.
- AI does nothing until you add a connection: a provider preset (Gemini, OpenAI, Groq, OpenRouter, OpenCode)
  with your key, which the operating system's keychain holds, or any OpenAI-compatible base URL, which is how
  a local server such as Ollama or llama.cpp fits in; a base URL on this computer needs no key. Only what you
  send goes to that provider. No model ships with the app, on purpose: the suite is Rust only, and the usual
  local engine (llama.cpp) is C++.
- No account, analytics or crash reporting.

Security notes, checkable in the repository:

- The app and its four crates contain no `unsafe` code (the workspace denies it). The layers below do, as in
  any desktop app: the window system (winit) and the GPU (wgpu).
- TLS for the update check and for the AI calls is pure Rust (rustls with RustCrypto); `ring`, `aws-lc` and
  OpenSSL are not in the program.
- Releases are built by GitHub Actions from a tagged commit with locked dependencies. Every dependency passes
  `cargo-deny` (advisories, licenses, bans), and none comes from a git URL.

## Where it comes from

Local Mindmap is a Rust rewrite of **LocalMind**, an earlier mind-map app built with Electron, React and
TypeScript. The Electron code is gone; what it left is the behaviour: the balanced four-way layout and its
constants, the `.tmind` format, the text of the exports, and the test cases that pinned them. The differences
found at the port are recorded in [UPSTREAM-SYNC.md](UPSTREAM-SYNC.md).

Why rewrite instead of keeping Electron: one runtime for the whole suite, a smaller app that starts fast, no
web engine between your map and the screen, and the same shell, updater and release process as the other Local
apps. What it cost: no local AI engine (the old app had one, built on llama.cpp, which is C/C++).

There is no upstream project to fork here; the app itself is ours. What comes from elsewhere is the shared
foundation, vendored in `libs/`: the suite's forks of storytold's egui (commit `6d518fc`), winit (`a517b38`)
and craft-libs, with its updater (the suite's permanent fork of the ArtCraft updater), and
[craft-fonts](https://github.com/storytold/craft-fonts), whose CJK faces release builds embed; the other
vendored pieces (`winprint.rs`, `rustls-rustcrypto`) are credited in [NOTICE](NOTICE). Nothing from
`storytold/` is read at build, test or release.

## Download

Windows (x64): `.msi` installer or portable `.zip`. Linux (x86_64): `.AppImage`, `.deb`, `.rpm`, Arch
`.pkg.tar.zst` or `.tar.gz`. All in
[Releases](https://github.com/Styx-Studios/Local-Mindmap/releases/latest). macOS is not published.

Each file is signed with [minisign](https://jedisct1.github.io/minisign/), the same key as every Local app,
when the release pipeline has the signing key; check that the `.minisig` file is in the release. Each release
also has a `SHA256SUMS.txt`. To check a file:

```sh
minisign -Vm <file> -P RWTqN+hTTCFbDCGCA1cEJRLp2FSluxIUiJfqdoAYwkwNNVH5MTYH8qSN
```

The public key is also in [packaging/minisign.pub](packaging/minisign.pub).

## How the app is built

| Part | What it holds |
|---|---|
| `crates/model` | nodes, the tree their `parentId` links form, the edits and the undo history. No workspace dependency |
| `crates/layout` | where every card sits: the balanced four-way layout, a port of the old app's geometry with the same constants |
| `crates/io` | reading and writing `.tmind`, atomic saves, unknown fields kept |
| `crates/ai` | the prompts, the tolerant JSON extractor, the five actions and the chat |
| `apps/localmindmap` | the egui app: canvas, editing, menus, exports, the command line and the MCP server |

Nothing below the app crate knows about egui; `cargo xtask layers` enforces the direction. The shared crates
(window frame, icons, palette, i18n, AI panel, MCP) are mirrored from the suite's source with a hash check in
CI.

## How it is made

Local Mindmap follows the same rules as every Local app: local files and no account, Rust only (including
the UI and the cryptography), one self-contained repository per app, governed dependencies, signed releases
built from a tag, optional AI with your own provider, and MCP so agents can use the app. Most of the code is
written by AI coding agents (Claude Code and OpenCode) from written plans, reviewed by the maintainer and held
to the same checks and CI. The reasons for each decision:
[How we make the Local apps](https://github.com/Styx-Studios/.github/blob/main/profile/README.md#how-we-make-the-local-apps).

## Build from source

Rust 1.95 or newer.

```sh
git clone https://github.com/Styx-Studios/Local-Mindmap
cd Local-Mindmap
cargo run --release -p localmindmap
cargo xtask ci        # all local checks
```

## Keyboard and mouse

`Ctrl+K` lists every command. The common ones:

- `Ctrl+N` new map, `Ctrl+O` open, `Ctrl+S` save, `Ctrl+Shift+S` save as.
- `F2` or double-click edits the card's text; `Tab` new child, `Enter` new sibling, `Delete` the card and its
  branch, `Space` collapses or expands it.
- Arrows move the selection; `Alt` plus an arrow reorders, indents or outdents the selected card.
- `Ctrl+Z` / `Ctrl+Y` undo and redo; `Ctrl+Enter` has the AI expand the selected card.
- Drag the canvas to pan, and scroll or pinch to zoom at the pointer. Drag the selected card's handle to change
  its width.
- The `+` buttons around the root add a branch on that side.

Help → Keyboard and mouse lists the main shortcuts inside the app.

## AI

AI does nothing until you add a connection: [Network and privacy](#network-and-privacy) says what one is and
where the key lives.

The AI menu generates a map from a topic or from a whole text, with a depth and a breadth, expands
the selected card, expands it deep, explains it, and rewrites it with an instruction (four presets,
or your own); `Ctrl+Enter` expands the card without the menu. Each action becomes one undo step.

The chat reads the map's outline as its context, and its answers stay in the panel as bubbles: you
insert one as a node or as a branch parsed from the outline. `Enter` sends the message and
`Shift+Enter` breaks the line. What you send goes only to the provider you chose.

## The Local apps

[Local Word](https://github.com/Anon5T4R/local-office) ·
[Local Sheets](https://github.com/Anon5T4R/local-sheets) ·
[Local Slides](https://github.com/Anon5T4R/local-slides) ·
[Local PDF](https://github.com/Anon5T4R/local-pdf) ·
[Local Keys](https://github.com/Anon5T4R/Local-Keys) ·
[Local Image](https://github.com/Anon5T4R/Local-Image) ·
[Local Photo](https://github.com/Anon5T4R/Local-Photo) ·
[Local Paint](https://github.com/Anon5T4R/Local-Paint) ·
[Local Vector](https://github.com/Anon5T4R/Local-Vector) ·
[Local Player](https://github.com/Anon5T4R/Local-Player) ·
[Local Notebook](https://github.com/Anon5T4R/Local-Obsidian) ·
[Local Mindmap](https://github.com/Styx-Studios/Local-Mindmap) ·
[Local Utils](https://github.com/Anon5T4R/Local-Utils)

## License and credits

MIT or Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)); third-party
notices in [NOTICE](NOTICE) and asset credits in [assets/ATTRIBUTION.md](assets/ATTRIBUTION.md). Release builds
embed the craft-fonts faces (SIL Open Font License 1.1), and the Windows installer ships their licence texts.
