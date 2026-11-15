# UPSTREAM-SYNC — Local Mind

O que neste repositório vem de fora, e o que este app faz de diferente da spec.

## De onde vem o comportamento

**Spec:** `C:\Dev\Local\LocalMind` — o **LocalMind** Electron/React/TypeScript, v0.5.9 (repo
`Anon5T4R/LocalMind`), **somente leitura**: nada aqui o altera, e nada de `storytold/*` é lido em
build, teste ou release.

O que a porta copia é o comportamento e a geometria, não o código: o layout do `layout.ts` (mesmas
constantes), as regras da árvore do `store.ts`, o formato do `.tmind` do `persistence.ts`, o texto do
export e as armadilhas que os testes dele fixam. A suíte de testes dele (37 casos em
`layout.test.ts`/`export.test.ts`, mais os comportamentos do `store.ts` que ele **não** testava)
virou a suíte deste app em `crates/localmind-*`.

## Porta por fases (estado)

| Fase | Escopo | Estado |
|---|---|---|
| F1 (v0.1.0) | nascimento, modelo, layout, `.tmind` (abrir/salvar/salvar como), canvas com pan/zoom/fit/seleção, associação de arquivo, i18n, ícone | **concluída e publicada** (8 assets assinados) |
| F2 (v0.2.0) | edição: texto inline (guarda de IME), filho/irmão, excluir, colapsar, desfazer/refazer, indentar/desindentar/mover, menu de contexto, largura por arrasto, nota editável | **concluída e publicada** (16 testes de UI no kittest) |
| F3 (v0.3.0) | estilos por nó (cor/tinta, fonte, lado), 6 temas nomeados, minimapa, export Markdown/HTML/SVG/PNG | **concluída e publicada** |
| F4 (v0.4.0) | IA remota (BYOK via `local-ai`): conexões com chave no keychain, 5 ações, chat com streaming, cancelar/erros | **concluída e publicada** |
| F5 (v1.0.0) | paridade e polimento: re-parent por Ctrl+arrasto + diálogo "Mover para…", tema claro craft, Ajuda ("Teclado e mouse"), tabela final, fixture de `.tmind` real, docs | **concluída e publicada** |

## Divergências declaradas (não são acidente)

- **IA sem motor local GGUF.** O app antigo carrega `.gguf` com `node-llama-cpp` (llama.cpp é C/C++).
  A suíte é Rust puro, sem dep C: a IA fala só com provedores remotos, pela crate `local-ai`
  (OpenAI-compatible: Gemini, OpenAI, Groq, OpenRouter, OpenCode, custom). Chave no keychain do SO,
  como nos outros apps. Caminho futuro, se fizer falta IA offline: um processo externo
  (`llama-server`) ou um binding puro — nenhum dos dois entra sem decisão explícita.
- **Anthropic** entra pelo preset OpenRouter, não por um adaptador nativo: a API da Anthropic não
  responde na rota `/chat/completions` que a suíte usa. Um adaptador próprio só se for pequeno.
- **Nota do nó é editável.** O app antigo tem o campo `note` e desenha o ponto, mas nenhuma forma de
  escrever nele (`setNote` nunca é chamado pela UI). Aqui a nota é editável — melhoria pedida, não
  paridade.
- **Re-parent por arrasto.** O app antigo não tinha como mover um nó para outro pai a não ser
  desfazer e refazer a árvore. Aqui **Ctrl+arrasto** move o nó (com a subárvore) para baixo de outro
  cartão, com alvo destacado antes do drop e a recusa de ciclos no modelo; sem mouse, o menu do cartão
  e o menu **Edit** abrem "Mover para…", que escolhe o pai numa lista. Acréscimo pedido no backlog,
  não paridade.
- **Arrastar um arquivo para a janela** abre o mapa (o antigo só abria por diálogo/argv).
- **Temas:** o antigo tem 6 temas por variáveis CSS; aqui as **paletas são as mesmas** (`--bg`, `--panel`, `--accent`, `--canvas-dots`, `--minimap-*` viram os campos de `theme::Colors`) e mudam a janela inteira, o canvas e o minimapa. Além dos seis herdados, um **tema claro** no padrão craft (os cinzas neutros e o azul da suíte). Diferente do antigo: a escolha fica no `prefs.json` e no menu **View → Theme** (o antigo escondia nas Configurações).
- **Export:** Markdown, HTML, **SVG** e **PNG**. O SVG é acréscimo nosso (vetor, sem rasterizador). O PNG é renderizado pelo **mesmo desenho da tela** (contexto egui off-screen → malhas do epaint → preenchimento em software aqui, com o atlas de fonte), em 2× e comprimido com `miniz_oxide` (Rust puro, já no grafo via `zip`/`flate2`; compressão com nível 6, ~65 KB para um mapa típico). Não há lib de rasterização envolvida.
- **Minimapa** no canto inferior direito: o mapa inteiro, o retângulo do viewport e clique/arrasto que move a vista (o antigo tinha o mesmo, do ReactFlow).
- **IA remota com a chave do usuário**: conexões (preset de provedor + base URL + modelo + chave no keychain), **5 ações** (gerar mapa com profundidade/amplitude, expandir, expandir fundo, explicar, reescrever com instrução) e **chat** com streaming, "como nó"/"como ramo" e limpar. O cliente é o `local-ai` compartilhado (`chat_stream`/SSE), o parser é o extrator tolerante do app antigo (blocos de raciocínio, cercas, prosa, vírgula final). **Sem motor local GGUF** (llama.cpp é C/C++): limitação declarada.
- **Cancelar** para a exibição e descarta a resposta; não aborta o HTTP (o cliente compartilhado lê o stream até o fim) — limitação declarada, com o hook de aborto como follow-up na fonte.
- **Single-instance** não foi trazido: hoje cada arquivo aberto pela associação abre uma janela
  própria. O antigo reaproveitava a janela existente (`second-instance`); entra se incomodar.

## Paridade item a item (F5)

O LocalMind Electron como spec; onde a coluna "Aqui" diz **divergência**, a diferença é
deliberada e está detalhada na seção acima.

### Leiaute e canvas

| Antigo (Electron) | Aqui | Onde |
|---|---|---|
| leiaute nas 4 direções, balanceado, faixa por subárvore | igual | `crates/localmind-layout` (21 testes) |
| raiz = primeiro nó sem pai; raiz duplicada não trava | igual | layout + io |
| altura pelo texto; `width`/`fontSize` zero usam o padrão | igual | `node_size` |
| colapsar esconde a subárvore e mantém o lado | igual | `hidden_ids`/`resolve_sides` |
| `side` herdado pela subárvore; `up`/`down` só explícitos | igual | `compute_sides` |
| pan/zoom (roda/pinça no cursor)/"ajustar à janela"/seleção por clique | igual | `apps/localmind/src/canvas.rs` |
| minimapa com retângulo do viewport, clique e arrasto | igual | `canvas.rs` + `minimap_rect` |
| alça de largura no cartão (arrasto = um passo de desfazer) | igual | `canvas.rs` (`resizing`) |

### Edição da árvore

| Antigo (Electron) | Aqui | Onde |
|---|---|---|
| editar texto inline (duplo clique / F2), guarda de IME | igual | `edit.rs` + teste de IME |
| novo filho (Tab), novo irmão (Enter) | igual | `edit.rs` |
| excluir nó e subárvore (Delete), raiz protegida, seleciona o pai | igual | `edit.rs` + model |
| colapsar/expandir ramo (Espaço) | igual | `edit.rs` |
| desfazer/refazer (Ctrl+Z / Ctrl+Y) | igual | `model::History` |
| indentar/desindentar e reordenar (Alt+setas) | igual | `edit.rs` (`indent`/`outdent`/`moveUp`/`moveDown`) |
| menu do cartão: editar, filho, irmão, IA, colapsar, lado, fonte, largura, cor, excluir | igual | `edit.rs` (`card_menu`) |
| **mover nó para outro pai** | **acréscimo**: Ctrl+arrasto (drop validado, recusa ciclo) **e** diálogo "Mover para…" | `canvas.rs` + `edit.rs` (`reparent`) |
| mover/redimensionar o próprio cartão | pan do canvas + alça de largura | canvas |

### Notas, estilo e temas

| Antigo (Electron) | Aqui | Onde |
|---|---|---|
| `color`, `fontSize`, `width`, `side` por nó | igual (menu do cartão/toolbar) | `edit.rs` + `doc.rs` |
| nota do nó (campo existe, **UI nunca escrevia**) | **divergência**: nota editável (menu → Nota…), ponto no cartão, incluída nos exports | `edit.rs` (`note_window`) |
| 6 temas por variáveis CSS | 6 paletas idênticas + **tema claro craft** (7), em View → Theme, salvos no `prefs.json` | `theme.rs` |

### Exportação

| Antigo (Electron) | Aqui | Onde |
|---|---|---|
| Markdown (títulos por nível, nota da raiz) | igual | `export.rs` |
| HTML (com as cores do tema) | igual | `export.rs` |
| PNG (canvas do ReactFlow → dataURL) | igual conceito: o **mesmo desenho da tela** via tesselação do epaint + rasterizador em software + `miniz_oxide`, em 2× | `png.rs` |
| — | **acréscimo**: SVG (vetor) | `export.rs` |

### IA

| Antigo (Electron) | Aqui | Onde |
|---|---|---|
| gerar mapa de um tema (profundidade/amplitude) | igual | `crates/localmind-ai` + `ai.rs` |
| expandir nó, expandir fundo, explicar | igual | idem |
| reescrever com instrução + 4 presets (curto, técnico, pergunta, corrigir) | igual | idem |
| chat com streaming, inserir como nó/ramo, copiar, limpar, parar | igual (parar = descarta a exibição; não aborta o HTTP — divergência declarada) | `ai.rs` |
| motor **local GGUF** (`node-llama-cpp`) | **divergência**: só remoto (llama.cpp é C/C++) | `libs/local-ai` |
| chave da API no cofre do SO por provedor, temperatura, modelo | igual (keychain do SO) | `ai.rs` + `local-ai` |

### Arquivos, janela e i18n

| Antigo (Electron) | Aqui | Onde |
|---|---|---|
| abrir/salvar/salvar como `.tmind` (pretty 2 espaços) | igual | `crates/localmind-io` |
| campos desconhecidos preservados no round-trip | igual | `MindNode::extra`/`MindMap::extra` |
| gravação atômica | igual | `localmind-io` |
| abrir por associação de arquivo e por argumento | igual | `main.rs` + `suite_entry.rs` |
| — | **acréscimo**: arrastar arquivo para a janela abre o mapa | `main.rs` |
| reusar a janela existente (`second-instance`) | **divergência**: cada arquivo abre a própria janela | — |
| pt/en/es (LocalePicker) | pt-br/es/ja (inglês é a fonte); segue o sistema, salvo | `i18n/` |
| About + verificação de atualização | igual (`craft-update`) + Ajuda "Teclado e mouse" | `app.rs` |

## Forks vendored (`libs/`)

| Árvore | Origem | Revisão | Sync |
|---|---|---|---|
| `libs/egui` | storytold/egui (fork do egui) | `6d518fc799681571963842f95b55a6b68431b15a` | skill `sync-upstream`; patches da suíte (bidi, scripting, viewport hooks) |
| `libs/winit` | storytold/winit (fork do winit) | `a517b3846ee8457d84ee4975110bfda6797c44dc` | idem |
| `libs/craft-libs` | storytold/craft-libs | base `1e6f1278` | idem, **exceto** `crates/update` (abaixo) |
| `libs/craft-libs/crates/update` | **`Anon5T4R/local-libs`** | `HASHES.update` da fonte | **fora do sync-upstream**: fork permanente consciente. Mudança vai na fonte e replica com `scripts/sync.ps1 -Mode push`; a cópia aqui é espelho e nunca se edita in place |
| `libs/winprint.rs` | alampy/winprint.rs (BSD-3) | `417ac3c` | usado por `craft-libs/crates/print` |
| `libs/rustls-rustcrypto` | fork da suíte | — | o `craft-update` pina este fork (o upstream alpha puxa rustls-webpki 0.102 só por `alg_id`) |
| `assets/craft-fonts` | storytold/craft-fonts | `b076417b5880bd4154bd22f00695b1d28c6bea83` | build input (`CRAFT_FONTS_DIR`) |

O app nasceu do molde `Anon5T4R/local-app-template` (`nascer-app.ps1`): o script foi usado uma vez e
**removido** do repo, como manda o README do molde.
