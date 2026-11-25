# AGENTS.md — Local Mindmap

App desktop da suíte Local: **Rust puro + egui forkado**, sem webview/React/TS. Mapas mentais com
layout equilibrado nas quatro direções; o formato é o `.tmind` do **LocalMind** (Electron), que é a
spec de comportamento deste port. O código Electron não existe mais na máquina (nem em
`C:\Dev\legado\Local\`) nem como repo no GitHub; a spec ficou só no histórico do git.

Estrutura e regras vêm do padrão craft (`docs/planos/padrao-craft.md` no dev-notes).

## Regras
- **Dependências:** toda dep que entra/sai/atualiza exige `Cargo.lock` commitado + linha no
  inventário do repo `Styx-Studios/local-deps` (regra completa em `local-deps/WORKFLOW.md`).
- **Sem fontes git:** nenhuma dependência pode apontar para `git+` (o CI valida; vale para o
  `Cargo.lock` da raiz e o de `libs/craft-libs`). Tudo vem do crates.io (pinado pelo lock) ou de
  `libs/` (vendored). Dep nova também precisa ser pure Rust (sem C).
- **egui/winit/craft-libs do fork vendored**, nunca do crates.io; nunca `lab-ui`.
- **craft-update tem fonte única** (`Styx-Studios/local-libs`): a cópia em `libs/craft-libs/crates/update`
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
- **Disco:** no workspace do João vale a política de `target/` do dev-notes (`MEMORY.md` §4 — limpa o que
  está frio ou feito, mantém durante iteração ativa); fora dele, `cargo clean` ao terminar. Builds pesados são do CI.

## Estrutura
- `apps/localmindmap` — o app: `main.rs` (args, ícone, janela), `app.rs` (menus, toolbar, About com
  update), `canvas.rs` (desenho e interação do mapa, minimapa, alça de largura, "+" nos 4 lados da
  raiz), `edit.rs` (seleção, texto inline, menu do cartão, nota, "Mover para…" com pai/lado, Ajuda,
  teclado e undo), `ai.rs` (as 5 ações, o chat e o strip de status/erro, desenhado antes do canvas;
  o chat e o formulário de conexões são o `craft-ai-ui` da suíte, e o mapa vai como contexto do
  painel), `close.rs` (o guardar-antes-de-fechar: a máquina de estado do close e o modal
  Salvar/Não salvar/Cancelar), `export.rs`
  (Markdown/HTML/SVG), `png.rs` (o PNG pela tesselação do epaint), `theme.rs` (os 7 temas, com o
  claro craft), `doc.rs` (o mapa aberto e o arquivo), `prefs.rs` (idioma e tema), `legacy.rs` (os
  nomes do "Local Mind": pastas de prefs/dados/cache/portátil e chaves do chaveiro que a migração
  traz uma vez, sem sobrescrever nem apagar; e as env vars novas com fallback nas antigas), `i18n.rs`,
  `pickers.rs`, `fonts.rs`, `suite_entry.rs`, `build.rs`
- `crates/localmindmap-ai` — prompts localizados, o extrator JSON tolerante, as 5 ações + o chat, o
  `outline` (o mapa como contexto do chat, com orçamento) e o `Backend` (o seam para a rede; os
  testes unitários usam um mock, o e2e em `tests/sse_e2e.rs` sobe um servidor SSE em `127.0.0.1` e
  o `tests/real_e2e.rs` (ignorado) fala com um provedor real pelo `ai-test.env`)
- `crates/localmindmap-model` — nós, a árvore dos `parentId`, as edições e o histórico (L1,
  `Class::Standalone`: não depende de crate do workspace)
- `crates/localmindmap-layout` — onde cada cartão fica (port do `layout.ts` do app antigo, mesmas
  constantes; L2)
- `crates/localmindmap-io` — leitura/escrita do `.tmind`, gravação atômica (L2)
- `libs/` — forks vendored: `craft-libs` (com `crates/update` espelhado do `local-libs`),
  `egui` 6d518fc, `winit` a517b38, `winprint.rs`, `rustls-rustcrypto`
- `assets/` — `app-icon/` (dois mestres: normal e small), `craft-fonts/` vendored,
  `ATTRIBUTION.md`
- `i18n/locales/` — catálogos TSV `context⇥fonte EN⇥tradução`, embutidos pelo `build.rs`
- `packaging/` — Windows (WiX/MSI), Linux (AppImage/deb/rpm/tar), Arch (PKGBUILD), minisign
- `xtask/` — `ci`, `fmt`, `assets`, `layers`, `version`, `ico`
- `scripts/check-mirror.sh` — confere as crates e os assets (`craft-fonts`) espelhados do `local-libs` contra a fonte (CI e `xtask ci`)

## Build
```
cargo build --locked          # debug
cargo test --locked           # as fontes vendored entram sempre (.cargo/config.toml), igual ao CI
cargo xtask ci                # o gate local: o mesmo que o CI (fmt, clippy, build, assets, layers,
                              # testes, cargo-deny, espelhos do local-libs)
packaging/windows/package.ps1 # MSI + portable (Windows)
packaging/linux/package.sh    # AppImage/deb/rpm/tar (Linux)
```

## Assets
- Nenhum asset de produto proprietário. Só trabalho próprio, domínio público/CC0, CC-BY com
  atribuição, OFL ou permissivo (MIT/Apache).
- Todo asset tem linha em `assets/ATTRIBUTION.md` no mesmo commit (`cargo xtask assets` valida;
  `libs/` é árvore vendored e carrega a atribuição de origem).
- Fontes fora do que o egui embute vivem no craft-fonts (`assets/craft-fonts`).

## Agent-native (MCP)
Todo comando do app é alcançável por um agente. O servidor é `localmindmap mcp` (stdio), sobre o
`craft-mcp` da suíte (`libs/craft-libs/crates/mcp`, fonte `local-libs` — espelho, não editar), com
três ferramentas: `command_list` (o catálogo: id, rótulo, menu, `appOnly` e o schema dos parâmetros),
`command_run` (um id com `params`) e `command_batch` (passos em ordem, com `"$N.path"` puxando o
resultado de um passo anterior). Sem cliente MCP: `localmindmap commands [--search TEXT]` imprime o
catálogo e `localmindmap run --script job.json` roda os mesmos passos. O catálogo é
`src/automation/catalogue.rs`: os comandos da paleta (`src/app/commands.rs`) mais `doc.inspect`, o
lado de leitura do mesmo mapa (os nós e os ids que os comandos de edição recebem). Nada de ação nova.

O mapa é o documento: o headless abre, edita, salva e exporta **pelo próprio editor do app**
(`src/automation/session.rs` roda sobre `Editor`, com histórico de undo e gravação atômica do
`.tmind`), e a janela usa as mesmas funções em `src/live.rs`. O que precisa da janela (viewport,
diálogos, painel de IA, updater, sair) diz `appOnly` e o headless o recusa apontando o modo live.

Modo live: `localmindmap --control PORT` abre o canal JSON em `127.0.0.1` e
`localmindmap mcp --connect PORT` liga o MCP nele (porta nua = loopback; host que não é loopback é
recusado antes de conectar). `--automation-read-root DIR`/`--automation-write-root DIR`
(repetíveis, `--flag=DIR`) confinam os arquivos que os comandos abrem, salvam e exportam; uma espécie
sem raiz não concede nada daquele acesso e, sem nenhuma flag, não há confinamento. Com um app
rodando, as flags vão para o app. As três vias têm teste em `tests/agent_native.rs`.

## Upstream e a pasta `C:\Dev\storytold`

Menções a `storytold/*` em código, comentários, docs e atribuições são referência histórica e
obrigação de licença. **Nada** é lido de `C:\Dev\storytold` em build, teste ou release. A spec
funcional deste port é o **LocalMind Electron**, que não existe mais na máquina (só no histórico do
git; sem repo no GitHub). Este repositório não o altera.
