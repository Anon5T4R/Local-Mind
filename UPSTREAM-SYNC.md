# Upstream sync

There is no upstream project to fork here: the app is ours. This document records where its
behaviour comes from, which differences from that source are deliberate, what each release
carried, and which code is vendored and from where. Facts that a tag, a commit or a `path:line` in
this repository can prove are named with it.

## Base

- **Spec:** LocalMind, the Electron/React/TypeScript app, **v0.5.9**, read-only. Its code is gone:
  not on the machine (not even in `C:\Dev\legado\Local\`) and not on GitHub as a repository. What
  it left is the behaviour the port recorded in the first commit (`ab5f124`), which names the spec
  and its version.
- The port copied behaviour and geometry, not code: the layout of `layout.ts` (same constants),
  the tree rules of `store.ts`, the `.tmind` of `persistence.ts`, the text of the exports, and the
  traps its tests pinned. Its suite became this app's suite in `crates/` (the packages are named
  `localmindmap-*` since the rename, `3976116`).
- Local Mindmap is a Rust rewrite on the suite's egui fork: no web engine, one runtime with the
  other Local apps, the same shell, updater and release process. The cost is no local AI engine:
  `llama.cpp` is C/C++ and this suite is Rust only (`README.md` §Where it comes from).
- Nothing from `C:\Dev\storytold` is read at build, test or release. That folder is only a local
  reference for the `sync-upstream` skill. The mentions of `storytold/*` in code, comments and
  NOTICE are historical reference and licence obligation.
- The app was born from the suite's mould, `local-app-template`: the first commit is the rename
  and identity work its `scripts/nascer-app.ps1` does (`ab5f124`).

## Releases

The CI builds and publishes each tag from the tagged commit (`.github/workflows/release.yml`).
F1-F6.2 were the port. The "What shipped" column cites the commits of the work, not the tags' own
release commits.

| Tag | What shipped |
|---|---|
| v0.1.0 (F1) | birth from the mould; tree model, four-way layout, `.tmind` open/save/save-as, canvas with pan/zoom/fit/selection, file association, i18n, icon (`ab5f124`) |
| v0.2.0 (F2) | full editing: inline text with the IME guard, child/sibling, delete, collapse, undo/redo, indent/outdent/move, the card menu, drag width, editable note; UI tests on kittest (`20b71a2`) |
| v0.3.0 (F3) | per-node style (colour/ink, font, side), the old app's six named palettes, minimap, Markdown/HTML/SVG/PNG export (`0c1beaa`) |
| v0.4.0 (F4) | remote AI with the user's key: connections with the key in the OS keychain, five actions, streaming chat, cancel and errors (`97a3521`) |
| v1.0.0 (F5) | parity and polish: the suite's light palette, Help ("Keyboard and mouse"), the tables below, a real `.tmind` fixture (`5c5f539`) |
| v1.1.0 (F6) | hot corners (the `+` on the four sides of the root), "Move to…" with a side, AI robustness (visible error strip, the provider's reason, timeouts, a localhost e2e), the OpenCode Go preset's base fixed (Zen → Go); **Ctrl+drag removed** (`862f59f`) |
| v1.1.1 (F6.1) | the provider error uses the real reason (`error.metadata.raw` + `provider_name`); **save before close** (Save / Don't save / Cancel) (`182f528`) |
| v1.1.2 (F6.2) | Gemini fetches models from the native `/v1beta/models`; the OpenCode Go recipe completed (`x-opencode-session` = UUID per request, JSON content type, old Zen connections migrated) (`3886a25`) |
| v1.1.3 | vendored craft-update (MSI copy detection, honest prompt) and local-ai (tolerant JSON extractor) (`7ae6a39`, `ac729e8`) |
| v1.2.0 | branch colours, `Ctrl+Enter` expands the selected card, the AI actions in the card menu (`b5d3bee`) |
| v1.2.1 | the chat moves to the suite's `craft-ai-ui` and reads the map (the outline as context) (`eef237c`) |
| v1.2.2 | System theme mode (follows the OS) as the default (`a51259b`) |
| v1.2.3 | `.tmind` in "Open with": Windows Capabilities and Linux `MimeType`/`mime.xml`/metainfo (`d8d2b98`) |
| v1.2.4 | the System mode on Linux through `craft-desktop-theme` (`509c09b`) |
| v1.2.5 | `cargo xtask ci`, `cargo-deny`, `publish = false`, fonts through `CRAFT_FONTS_DIR`, CI jobs for deny and for the mirrors (`586c76c`, `cb7bbf9`, `65adda6`, `753d4f1`) |
| v1.2.6 | command palette (`Ctrl+K`, `craft-palette`), suite icons, **two palettes and three modes** (the five extra palettes dropped), Settings window with language and theme (`dc3196b`, `f97ca78`, `dc4b95c`) |
| v1.2.7 | width and accessibility guards, ink on the accent computed for contrast, `craft-ai-ui` mirrored (`2ed7038`, `9d77dd1`, `3d32e7a`) |
| v1.2.8 | the palette's row outside the window measures its height instead of a fixed 600 (`3707293`) |
| v1.2.9 | agent-native: MCP over stdio, `run --script`, `commands --search`, the live control channel; craft-update with `jiff` (RUSTSEC-2026-0543) (`f8c5834`, `374daa3`) |
| v1.2.10 | the suite's night-style icon (`b4d99b8`) |
| v1.3.0 | **Local Mind becomes Local Mindmap** (names in Rust, catalogue, tests, icons), packaging/env/links, one-time migration of the old folders and keychain keys (`3976116`, `5f6cbb0`, `0668edf`, `a553716`, `fd51d5e`) |

The workspace version is the last release (`Cargo.toml`).

## The rename (1.3.0)

- Up to 1.2.10 (tag `v1.2.10`) the product was **Local Mind**; from 1.3.0 it is **Local Mindmap**
  (`3976116`). The maps did not change: a `.tmind` file keeps its format, and the rename touched
  names, packaging and identities only (`apps/localmindmap/src/legacy.rs:16`).
- On start the app migrates what the old name left, once and quietly: the preferences folder, the
  AI connections folder, the updater's data and cache, and the portable data folder beside the
  executable. It never overwrites — when the new folder exists the old one is left alone, and when
  the move has to be a copy the old folder stays (`legacy.rs:7-13`, `legacy.rs:39-40`).
- The API keys in the OS keychain are copied to the new service name **once**, marked by a file;
  the old entries are kept (`legacy.rs:12-13`, `legacy.rs:45-46`).
- `LOCALMIND_*` still works as a fallback for `LOCALMINDMAP_*` (`legacy.rs:14`,
  `apps/localmindmap/src/main.rs:159-162`, `packaging/env.sh:24`, `env.sh:35`).

## Declared divergences (not accidents)

- **AI without a local GGUF engine.** The old app loads `.gguf` with `node-llama-cpp` (llama.cpp
  is C/C++). The suite is Rust only, with no C dependency: AI talks to remote providers through
  the `local-ai` crate (OpenAI-compatible: Gemini, OpenAI, Groq, OpenRouter, OpenCode Zen/Go,
  custom), and
  a local OpenAI-compatible server (Ollama, llama.cpp) fits as a Custom connection. The key lives
  in the OS keychain, as in the other apps. A future offline path would be an external process
  (`llama-server`) or a pure binding; neither enters without an explicit decision.
- **Anthropic** comes in through the OpenRouter preset, not a native adapter: the Anthropic API
  does not answer on the `/chat/completions` route the suite uses. A small adapter of our own is
  the only other path in.
- **The node's note is editable.** The old app had the `note` field but no way to write into it.
  Here it is editable — a requested improvement, not parity.
- **Re-parenting goes through a dialog, not a drag.** The old app had no way to move a node under
  another parent except undoing and rebuilding the tree. Here the card menu and the **Edit** menu
  open "Move to…", which picks the parent from a list and, when the parent is the root, the
  **direction** (left, right, up, down) in the same step (`edit.rs:831`). Ctrl+drag was tried in F5
  and **removed in v1.1.0**: it passed the kittest but the real gesture was not reliable, and a
  gesture that works 95% of the time is worse than none. The `+` buttons around the root (hot
  corners) create a branch already on that side (`canvas.rs:155`, `canvas.rs:596`), and the "Side"
  submenu changes the direction of a root child.
- **Save before close.** The old app closed and lost what was not saved (only a generic browser
  warning, with no control). Here closing a dirty map **cancels the close** and opens a modal
  **Save / Don't save / Cancel** (the Word pattern): Save writes — or asks for a path first — and
  closes only if the write worked; Don't save closes; Cancel keeps the window. A cancelled save-as
  leaves no pending close (`apps/localmindmap/src/close.rs`). An addition, not parity.
- **Dropping a file on the window** opens the map (the old app only opened from a dialog or argv).
- **Themes: two palettes, three modes.** The old app had six CSS themes. The port carried them and added the suite's
  light palette; v1.2.6 dropped the extras (`dc4b95c`). The palettes are the old app's dark one, translated one to one (`--bg`, `--panel`,
  `--accent`, `--canvas-dots`, `--minimap-*` become `theme::Colors`), and the suite's neutral light
  one (`apps/localmindmap/src/theme.rs:1-19`). Three modes — Automatic (follows the OS), Light,
  Dark — pick between them; a preference saved with one of the old names falls to the closer side
  (`theme.rs:47-55`, `ROADMAP.md` §Known gaps and deliberate limits). The choice lives in the top
  bar's theme button and in Settings (Edit → Settings…), saved in `prefs.json`
  (`app.rs:131-142`, `app.rs:596-623`); the old app hid it in its settings window.
- **Export:** Markdown, HTML, **SVG** and **PNG**. The SVG is ours (vectors, no rasteriser). The
  PNG is rendered from the **same drawing as the window** (an off-screen egui context → epaint
  meshes → a software fill here, with the font atlas), at 2× and compressed with `miniz_oxide`
  (pure Rust, already in the graph through `zip`/`flate2`; level 6, ~65 KB for a typical map). No
  rasterisation library is involved.
- **Minimap** in the bottom-right corner: the whole map, the viewport rectangle, and click/drag to
  move the view (the old app had the same, from ReactFlow).
- **Remote AI with the user's key:** connections (provider preset + base URL + model + key in the
  keychain), **five actions** (generate a map from a topic or a whole text, with depth and breadth;
  expand; expand deep; explain; rewrite with an instruction and four presets) and a **chat** with
  streaming, "as a node"/"as a branch" and clear (`crates/ai/src/actions.rs:15-22`,
  `apps/localmindmap/src/ai.rs:434-439`, `ai.rs:628-632`). The client is the shared `local-ai`
  (`chat_stream`/SSE), the parser is the old app's tolerant extractor (reasoning blocks, fences,
  prose, trailing comma), and the chat panel is the suite's `craft-ai-ui`, with the map sent as its
  context (`crates/ai/src/outline.rs`). **No local GGUF engine** (llama.cpp is C/C++): a declared
  limit.
- **Cancel** in the chat cancels the display and discards the answer; it does not abort the HTTP
  request (the shared client reads the stream to the end). A declared limit, with the abort hook as
  a follow-up in the source (`ROADMAP.md` §Known gaps and deliberate limits).
- **Single instance was not brought over:** today each file opened through the association starts
  its own window. The old app reused the existing window (`second-instance`); it comes in if it
  gets in the way (`ROADMAP.md` §Known gaps and deliberate limits).

## Agents: palette, command line, MCP

Every menu command is also a command of the app (`apps/localmindmap/src/app/commands.rs`), listed
by the command palette (`Ctrl+K` or the search icon, on the suite's `craft-palette`) and reachable
from outside the window:

- `localmindmap commands [--search TEXT]` prints the catalogue as JSON (the palette's ids);
  `--search` keeps the ids, labels or menus containing the text (`automation/cli.rs:35-36`).
- `localmindmap run [--in FILE] [--script FILE|-]` runs the commands headless and prints one JSON
  result per step; a script is a JSON array (or one object per line) and a parameter that is exactly
  `"$N.path"` takes that value from step N's result (`cli.rs:30-34`).
- `localmindmap mcp` is the MCP server on stdio, over the suite's `craft-mcp`
  (`libs/craft-libs/crates/mcp`, source in `local-libs` — a mirror, never edited here). By default
  it drives a running app at `127.0.0.1:7979` and falls back to a headless session; `--connect
  PORT|HOST:PORT` points it at an app started with `--control PORT` (a host that is not loopback is
  refused). Three tools: `command_list` (the catalogue: id, label, menu, `appOnly` and the parameter
  schema), `command_run` (one id with `params`) and `command_batch` (steps in order, with `"$N.path"`
  pulling a previous step's result). The catalogue is the palette's commands plus `doc.inspect`, the
  reading side of the same map: no new action (`automation/catalogue.rs:3-5`,
  `automation/mcp.rs:1-10`, `cli.rs:24-29`).

The map is the document: the headless mode opens, edits, saves and exports **through the app's own
editor** (`automation/session.rs`, with undo history and the `.tmind` atomic write), and the window
uses the same functions (`live.rs`). What needs the window (viewport, dialogs, AI panel, updater,
quit) says `appOnly` and the headless mode refuses it, pointing at live mode.

Live mode: `localmindmap --control PORT` opens the JSON channel on `127.0.0.1`; with an app running,
the MCP server connects to it and an agent's `edit.child` adds a card the user sees (`live.rs`).
`--automation-read-root DIR`/`--automation-write-root DIR` (repeatable, `--flag=DIR`) confine the
files the commands open, save and export: a kind left out grants none of that access, and without
either flag there is no confinement (`automation/session.rs:9-10`). The routes have a test each in
`apps/localmindmap/tests/agent_native.rs` (`main.rs:66-69`, `automation/control.rs:7`).

## Item-by-item parity

LocalMind Electron is the spec. Where the Here column says **divergence**, the difference is
deliberate and detailed above.

### Layout and canvas

| Old (Electron) | Here | Where |
|---|---|---|
| layout on the four sides, balanced, one band per subtree | same | `crates/layout` (tests in `crates/layout/src/lib.rs`) |
| root = first node without a parent; a duplicated root does not hang | same | layout + io |
| height from the text; `width`/`fontSize` zero use the default | same | `node_size` |
| collapse hides the subtree and keeps the side | same | `hidden_ids`/`resolve_sides` |
| `side` inherited by the subtree; `up`/`down` only when explicit | same | `compute_sides` (`crates/layout/src/lib.rs:204`) |
| pan/zoom (wheel/pinch at the pointer)/"fit to window"/selection by click | same | `apps/localmindmap/src/canvas.rs` |
| minimap with the viewport rectangle, click and drag | same | `canvas.rs` + `minimap_rect` (`canvas.rs:505`) |
| width handle on the card (a drag is one undo step) | same | `canvas.rs` (`resizing`) |

### Editing the tree

| Old (Electron) | Here | Where |
|---|---|---|
| edit text inline (double click / F2), IME guard | same | `edit.rs` + the IME test |
| new child (Tab), new sibling (Enter) | same | `edit.rs` |
| delete node and subtree (Delete), root protected, parent selected | same | `edit.rs` + model |
| collapse/expand a branch (Space) | same | `edit.rs` |
| undo/redo (Ctrl+Z / Ctrl+Shift+Z or Ctrl+Y) | same | `model::History` |
| indent/outdent and reorder (Alt+arrows) | same | `edit.rs` (`indent`/`outdent`/`moveUp`/`moveDown`) |
| card menu: edit, child, sibling, AI, collapse, side, font, width, colour, delete | same | `edit.rs` (`card_menu`) |
| **hot corners: `+` on the four sides of the root** | **parity**: a dashed circle in the accent; a click creates the child already on that side, selects it and opens the text; drawn outside `paint`, so never in the PNG | `canvas.rs` (`corner_at`/`hot_corners`) |
| **move a node to another parent / another direction** | **addition**: "Move to…" dialog (parent, plus the side when it is the root); the F5 Ctrl+drag was removed in v1.1.0 | `edit.rs` (`move_node_to`) |
| move/resize the card itself | canvas pan + width handle | canvas |

### Notes, style and themes

| Old (Electron) | Here | Where |
|---|---|---|
| `color`, `fontSize`, `width`, `side` per node | same (card menu/toolbar) | `edit.rs` + `doc.rs` |
| node note (field exists, **the UI never wrote it**) | **divergence**: editable note (menu → Note…), dot on the card, included in the exports | `edit.rs` (`note_window`) |
| named palettes by CSS variables | **divergence**: two palettes — the old app's dark, translated one to one, and the suite's neutral light one — and three modes (Automatic/Light/Dark) in the top bar and Settings, saved in `prefs.json`; an old palette id falls to the closer side | `theme.rs`, `app.rs:131-142`, `app.rs:596-623` |

### Export

| Old (Electron) | Here | Where |
|---|---|---|
| Markdown (headings by level, the root's note) | same | `export.rs` |
| HTML (with the theme's colours) | same | `export.rs` |
| PNG (ReactFlow canvas → dataURL) | same concept: the **same drawing as the window** through epaint tessellation + a software rasteriser + `miniz_oxide`, at 2× | `png.rs` |
| — | **addition**: SVG (vectors) | `export.rs` |

### AI

| Old (Electron) | Here | Where |
|---|---|---|
| generate a map from a topic (depth/breadth) | same | `crates/ai` + `ai.rs` |
| expand a node, expand deep, explain | same | idem |
| rewrite with an instruction + four presets (shorter, more technical, as a question, fix the wording) | same | idem |
| — | **addition**: `Ctrl+Enter` expands the selected card with AI | `edit.rs:630-633` |
| chat with streaming, insert as node/branch, copy, clear, stop | send, stop, retry, clear and insert as node/branch in the suite's `craft-ai-ui` (the panel has no copy action); the chat also reads the map (outline as context), which the old one did not; stop = discards the display, does not abort the HTTP (declared divergence) | `ai.rs`, `crates/ai/src/outline.rs`, `libs/craft-libs/crates/ai-ui/src/chat.rs:20-34` |
| **local GGUF engine** (`node-llama-cpp`) | **divergence**: remote only (llama.cpp is C/C++) | `libs/local-ai` |
| API key in the OS keychain per provider, temperature, model | same (OS keychain) | `ai.rs` + `local-ai` |

### Files, window and i18n

| Old (Electron) | Here | Where |
|---|---|---|
| open/save/save as `.tmind` (pretty 2 spaces) | same | `crates/io` |
| unknown fields kept in the round trip | same | `MindNode::extra`/`MindMap::extra` |
| atomic write | same | `crates/io` |
| open by file association and by argument | same | `main.rs` + `suite_entry.rs` |
| — | **addition**: dropping a file on the window opens the map | `main.rs` |
| reuse the existing window (`second-instance`) | **divergence**: every file opens its own window | — |
| pt/en/es (LocalePicker) | pt-br/es/ja (English is the source); follows the system, then the saved choice (`i18n/locales/`) | `i18n.rs` |
| About + update check | same (`craft-update`) + Help "Keyboard and mouse" | `app.rs` |

## AI: what the real tests exposed

The real test (the user's key) exposed defects the in-process mock did not see. All of them are
fixed and still covered:

- **The error banner was invisible**: the status/error strip was drawn **after** the canvas, which
  takes the whole space, so the error was set and the user saw nothing. Now the strip is drawn
  **before** the canvas, with the provider's reason and a Dismiss button; the `ai-quota-error` test
  pins "free usage reached" in the render.
- **Provider error without the reason**: `ureq` treated non-2xx as a transport error and dropped
  the body with the reason ("invalid API key", "insufficient balance", "free usage limit"). The
  client now uses `http_status_as_error(false)` and reads the body; `provider_error` prefers
  `error.metadata.raw` (where OpenRouter puts the upstream provider's message), prefixes it
  with `metadata.provider_name` and only falls back to the generic `error.message`. A 429
  rate-limit shows the reason and the provider, not "Provider returned error".
- **No timeout**: the client had no read deadline; a stream that never sends `[DONE]` and never
  closes hung forever. There are now deadlines for DNS/connect/send/headers/body (10/10/30/60/120 s
  in `libs/local-ai/src/lib.rs:25-33`), and a stream that expires **after** delivering text
  returns that text instead of losing it.
- **Permanent e2e on localhost**: `crates/ai/tests/sse_e2e.rs` starts a `TcpListener` that answers
  OpenAI-shaped SSE and covers success (text → map), a provider error, a quota error inside a stream,
  429 with a provider message and with raw metadata, the Gemini native models route and the
  `Authorization` header. It is the test the mock was not.
- **OpenCode Go**: the validated recipe is base `https://opencode.ai/inference/go/openai/v1`,
  `x-opencode-session` = a **random UUID per request**, `Content-Type: application/json` and
  Bearer. Without the session the gateway rejects the request; the old connection (Zen base) is
  migrated to the Go base on load.
  The Zen base is `https://opencode.ai/zen/v1`, with no Go header and the `…-free` ids
  (`libs/local-ai/src/lib.rs:120-129`, `libs/local-ai/src/lib.rs:280`).
- **Gemini**: the `/v1beta/openai/models` compat route is **404**; "Fetch models" uses the native
  `/v1beta/models` with `x-goog-api-key`, filters `generateContent` and strips the `models/` prefix
  (`libs/local-ai/src/lib.rs:338`).

**OpenCode keys:** use a **personal** Go key or another provider (OpenRouter/Groq/Gemini/etc.).

## Vendored forks (`libs/`)

| Tree | Origin | Revision | Sync |
|---|---|---|---|
| `libs/egui` | storytold/egui (fork of egui) | `6d518fc799681571963842f95b55a6b68431b15a` | skill `sync-upstream`; the suite's patches (bidi, scripting, viewport hooks) |
| `libs/winit` | storytold/winit (fork of winit) | `a517b3846ee8457d84ee4975110bfda6797c44dc` | idem |
| `libs/craft-libs` | storytold/craft-libs | base `1e6f1278` (docking: a drop beside a too-narrow frame…) | skill `sync-upstream`; the crates below are mirrored from `local-libs` instead |
| the crates of `libs/craft-libs/` mirrored from `local-libs` | **`Styx-Studios/local-libs`** | checked against the source tree by `scripts/check-mirror.sh`, which lists them (CI and `xtask ci`) | edit the source and replicate with `sync.ps1 -Mode push`; the copy here is a mirror |
| `libs/craft-libs/crates/update` | **`Styx-Studios/local-libs`** | same hash check | **outside the sync-upstream**: conscious permanent fork (below) |
| `libs/local-ai` | `Styx-Studios/local-ai` | mirror; the same bytes the sibling apps vendor | its TLS is the suite's `rustls-rustcrypto` fork, wired by `[patch.crates-io]` (`Cargo.toml:44-47`) |
| `libs/winprint.rs` | storytold/winprint.rs (BSD-3), the suite's maintenance branch of ArcticLampyrid/winprint v0.2.1 | based on `406ed73` | vendored for `craft-print`; this app does not depend on it, so nothing here compiles it |
| `libs/rustls-rustcrypto` | fork of RustCrypto/rustls-rustcrypto, kept at `0.0.2-alpha` | — | `craft-update` pins this fork; the upstream alpha pulls rustls-webpki 0.102 for `alg_id` alone |
| `assets/craft-fonts` | storytold/craft-fonts | `b076417b5880bd4154bd22f00695b1d28c6bea83` | build input (`CRAFT_FONTS_DIR`) |

`libs/craft-libs/crates/update` is a **conscious permanent fork**: bugfix and feature go straight
into the single source (`Styx-Studios/local-libs`, crate `crates/update`) and from there are replicated
to the apps by the sync script (with a hash guard; the copy vendored here is a mirror). Never edit
the local copy: the next sync overwrites it. Upstream contributions only through a manual
Issue/PR, in English, when the problem also exists there. The rest of `craft-libs` follows the
normal flow of this document.
