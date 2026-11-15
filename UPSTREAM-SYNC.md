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
| F1 (v0.1.0) | nascimento, modelo, layout, `.tmind` (abrir/salvar/salvar como), canvas com pan/zoom/fit/seleção, associação de arquivo, i18n, ícone | **concluída** |
| F2 | edição: texto inline, filho/irmão, excluir, colapsar, desfazer/refazer, indentar/desindentar/mover, menu de contexto, largura por arrasto, nota editável | pendente |
| F3 | estilos (cor, fonte, lado), temas nomeados, minimapa, export Markdown/HTML/PNG | pendente |
| F4 | IA remota (BYOK via `local-ai`): gerar mapa, expandir, expandir fundo, explicar, reescrever, chat | pendente |
| F5 (v1.0.0) | paridade e polimento: tabela final, fixture de `.tmind` real, Ajuda, docs | pendente |

## Divergências declaradas (não são acidente)

- **IA sem motor local GGUF.** O app antigo carrega `.gguf` com `node-llama-cpp` (llama.cpp é C/C++).
  A suíte é Rust puro, sem dep C: a fase de IA fala só com provedores remotos, pela crate `local-ai`
  (OpenAI-compatible: Gemini, OpenAI, Groq, OpenRouter, OpenCode, custom). Chave no keychain do SO,
  como nos outros apps. Caminho futuro, se fizer falta IA offline: um processo externo
  (`llama-server`) ou um binding puro — nenhum dos dois entra sem decisão explícita.
- **Anthropic** entra pelo preset OpenRouter, não por um adaptador nativo: a API da Anthropic não
  responde na rota `/chat/completions` que a suíte usa. Um adaptador próprio só se for pequeno.
- **Nota do nó é editável.** O app antigo tem o campo `note` e desenha o ponto, mas nenhuma forma de
  escrever nele (`setNote` nunca é chamado pela UI). Aqui a nota é editável — melhoria pedida, não
  paridade.
- **Arrastar um arquivo para a janela** abre o mapa (o antigo só abria por diálogo/argv).
- **Temas:** o antigo tem 6 temas por variáveis CSS; a fase F3 mapeia para os tokens do craft (a
  aparência não é copiada, o conjunto de climas é).
- **Single-instance** não foi trazido: hoje cada arquivo aberto pela associação abre uma janela
  própria. O antigo reaproveitava a janela existente (`second-instance`); entra se incomodar.

## Para a lista de conferência da paridade (F5)

Funcionalidades do antigo a cobrir, com o teste que as fixa aqui:

| Antigo | Aqui | Onde |
|---|---|---|
| layout 4 direções, balanceado, faixa por subárvore | feito | `crates/localmind-layout` (21 testes) |
| raiz = primeiro nó sem pai; raiz duplicada | feito | layout + io |
| altura pelo texto, `width`/`fontSize` zero | feito | `node_size` |
| colapsar esconde descendentes e mantém o lado | feito (modelo) | `hidden_ids`/`resolve_sides` |
| `side` herdado pela subárvore | feito | `compute_sides` |
| abrir/salvar `.tmind` (pretty 2 espaços) | feito | `crates/localmind-io` (8 testes) |
| campos desconhecidos preservados | feito | `MindNode::extra`/`MindMap::extra` |
| órfão descartado, ciclos não travam | feito | layout + model |
| pan/zoom/fit, seleção por clique | feito | `apps/localmind/src/canvas.rs` |
| export Markdown/HTML (escape, nota da raiz) | F3 | — |
| export PNG | F3 | — |
| edição completa (teclado, menu, undo) | F2 | — |
| IA (5 ações + chat) | F4 | — |
| i18n pt/en/es | feito (pt-br/es/ja; o inglês é a fonte) | `i18n/locales/` |

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
