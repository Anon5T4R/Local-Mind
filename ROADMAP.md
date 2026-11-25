# Local Mindmap roadmap

The honest state of the app: what is missing or weak, and what is planned. The README says what the app
does; the release notes say what changed in each version.

## Known gaps and deliberate limits

- **Re-parenting by dragging is not there.** The Ctrl+drag tried before 1.0 was removed in 1.1.0: it passed the
  UI tests but was not reliable in real use. "Move to…" (new parent, plus the direction when the parent is the
  root) and the `+` buttons around the root cover the same flow.
- **Only the two theme palettes remain.** The extra named palettes of the Electron app (Nature, Dark blue,
  Calm green, Pastel pink, PunkPrincess) were dropped; a preference saved with one of them now opens in the
  closer of Light and Dark. Colours saved on the cards are untouched.
- **AI is remote only.** No local model ships: the suite is Rust and llama.cpp is C/C++. A local
  OpenAI-compatible server (llama.cpp, Ollama) works through a Custom connection.
- **Stop in the chat** cancels the display and discards the answer, but the HTTP stream is read to the end;
  it does not abort the request.
- **One window per file.** Opening a map from the file manager starts a new window instead of reusing the one
  already open; the Electron app did the opposite.
- **Anthropic** is reachable only through the OpenRouter preset; there is no native adapter.
- **A very large map** is refused as a single PNG export (the image is capped at 64 million pixels).
- **The update check** runs at every start with no setting to turn it off.

## Planned

Items worth doing if they start to matter in use:

- A reliable drag gesture for re-parenting, keeping "Move to…" as the default.
- Reusing the running window when a second map is opened.
