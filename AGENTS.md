# AGENTS.md — Local Mind

App desktop da suíte Local: **Rust puro + egui forkado**, sem webview/React/TS. Mapas mentais com
layout equilibrado nas quatro direções; o formato é o `.tmind` do **LocalMind** (Electron), que é a
spec de comportamento deste port e vive em `C:\Dev\Local\LocalMind` — **somente leitura**.

Estrutura e regras vêm do padrão craft (`docs/planos/padrao-craft.md` no dev-notes).

## Regras
- **Dependências:** toda dep que entra/sai/atualiza exige `Cargo.lock` commitado + linha no
  inventário do repo `Anon5T4R/local-deps` (regra completa em `local-deps/WORKFLOW.md`).
- **Sem fontes git:** nenhuma dependência pode apontar para `git+` (o CI valida; vale para o
  `Cargo.lock` da raiz e o de `libs/craft-libs`). Tudo vem do crates.io (pinado pelo lock) ou de
  `libs/` (vendored). Dep nova também precisa ser pure Rust (sem C).
- **egui/winit/craft-libs do fork vendored**, nunca do crates.io; nunca `lab-ui`.
- **craft-update tem fonte única** (`Anon5T4R/local-libs`): a cópia em `libs/craft-libs/crates/update`
  é espelho — mudança se faz na fonte e replica com `scripts/sync.ps1 -Mode push`. **Toda** mudança na
  fonte passa `rustfmt --edition 2024` na raiz do `local-libs` antes do commit (a config
  `max_width=150`/`use_small_heuristics="Max"` está lá); sem isso o rustfmt do CI reformata o crate
  inteiro no estilo errado e o estrago se replica aos 13 apps.
- **Never-crash:** nada de `unwrap`/`expect`/`panic!`/`unreachable!`/`todo!` fora de testes (o clippy
  barra), `unsafe` proibido. Toda entrada (caminho, arquivo, argumento) é hostil até prova em
  contrário.
- **O mapa é do usuário:** nada de campo perdido no round-trip do `.tmind`, nada de gravação não
  atômica. Formato proprietário? Só o `.tmind`, que é aberto e já existia antes deste app.
- **Commit:** mensagem em português, sem Co-Authored-By, paths explícitos.
- **Release:** `cargo xtask version set X.Y.Z` **antes** de toda tag → tag `vX.Y.Z` → o Actions
  builda e publica. Mínimo local.
- **Disco local:** nunca deixe `target/` para trás — `cargo clean` após uso local; builds pesados
  são do CI.

## Estrutura
- `apps/localmind` — o app: `main.rs` (args, ícone, janela), `app.rs` (menus, toolbar, About com
  update), `canvas.rs` (desenho e interação do mapa, minimapa, alça de largura, "+" nos 4 lados da
  raiz), `edit.rs` (seleção, texto inline, menu do cartão, nota, "Mover para…" com pai/lado, Ajuda,
  teclado e undo), `ai.rs` (conexões, ações, chat e o strip de status/erro, desenhado antes do
  canvas), `export.rs`
  (Markdown/HTML/SVG), `png.rs` (o PNG pela tesselação do epaint), `theme.rs` (os 7 temas, com o
  claro craft), `doc.rs` (o mapa aberto e o arquivo), `prefs.rs` (idioma e tema), `i18n.rs`,
  `pickers.rs`, `fonts.rs`, `suite_entry.rs`, `build.rs`
- `crates/localmind-ai` — prompts localizados, o extrator JSON tolerante, as 5 ações + o chat e o
  `Backend` (o seam para a rede; os testes unitários usam um mock e o e2e em
  `tests/sse_e2e.rs` sobe um servidor SSE em `127.0.0.1` para exercitar o caminho real)
- `crates/localmind-model` — nós, a árvore dos `parentId`, as edições e o histórico (L1,
  `Class::Standalone`: não depende de crate do workspace)
- `crates/localmind-layout` — onde cada cartão fica (port do `layout.ts` do app antigo, mesmas
  constantes; L2)
- `crates/localmind-io` — leitura/escrita do `.tmind`, gravação atômica (L2)
- `libs/` — forks vendored: `craft-libs` (com `crates/update` espelhado do `local-libs`),
  `egui` 6d518fc, `winit` a517b38, `winprint.rs`, `rustls-rustcrypto`
- `assets/` — `app-icon/` (dois mestres: normal e small), `craft-fonts/` vendored,
  `ATTRIBUTION.md`
- `i18n/locales/` — catálogos TSV `context⇥fonte EN⇥tradução`, embutidos pelo `build.rs`
- `packaging/` — Windows (WiX/MSI), Linux (AppImage/deb/rpm/tar), Arch (PKGBUILD), minisign
- `xtask/` — `fmt`, `assets`, `layers`, `version`, `ico`

## Build
```
cargo build --locked          # debug
CRAFT_FONTS_DIR=assets/craft-fonts cargo build --locked   # com os CJK embutidos (como no CI)
cargo test --locked
cargo xtask fmt && cargo xtask assets && cargo xtask layers
packaging/windows/package.ps1 # MSI + portable (Windows)
packaging/linux/package.sh    # AppImage/deb/rpm/tar (Linux)
```

## Assets
- Nenhum asset de produto proprietário. Só trabalho próprio, domínio público/CC0, CC-BY com
  atribuição, OFL ou permissivo (MIT/Apache).
- Todo asset tem linha em `assets/ATTRIBUTION.md` no mesmo commit (`cargo xtask assets` valida;
  `libs/` é árvore vendored e carrega a atribuição de origem).
- Fontes fora do que o egui embute vivem no craft-fonts (`assets/craft-fonts`).

## Upstream e a pasta `C:\Dev\storytold`

Menções a `storytold/*` em código, comentários, docs e atribuições são referência histórica e
obrigação de licença. **Nada** é lido de `C:\Dev\storytold` em build, teste ou release. A spec
funcional deste port é o **LocalMind Electron** em `C:\Dev\Local\LocalMind` — leitura apenas: ele
não é alterado por este repositório.
