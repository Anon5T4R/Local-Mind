# Local Mind

Mapas mentais da suíte Local: **Rust puro + egui forkado**, sem webview, sem React, sem conta e sem
telemetria. É o port do **LocalMind** (Electron) para o padrão craft — os mapas que você já tem
continuam abrindo, porque o formato é o mesmo arquivo `.tmind`.

## O que funciona

- **Edição completa do mapa**: duplo clique (ou F2) escreve no cartão; Tab cria um filho, Enter um
  irmão, Delete exclui a subárvore (selecionando o pai), Espaço colapsa, as setas navegam, Alt+setas
  movem/indentam, Ctrl+Z/Ctrl+Y desfazem e refazem. O menu do cartão (clique direito) tem estrutura,
  lado, tamanho do texto, cor, largura, nota e excluir; a alça no canto do cartão selecionado muda a
  largura (um passo de desfazer por arrasto).
- **Nota do nó editável** (menu → Nota…), o ponto no cartão e a nota nos exports.
- **Seis temas nomeados** (o escuro padrão, Natureza, Azul escuro, Verde calmo, Rosa pastel e
  PunkPrincess) em **View → Theme**, com as paletas do app antigo — a janela, o canvas e o minimapa
  mudam juntos, e a escolha fica salva.
- **IA com a sua chave** (menu **AI**): conexões com preset de provedor e chave no keychain do
  sistema; gerar mapa de um tema ou de um texto, expandir, expandir fundo, explicar, reescrever com
  instrução, e um chat com streaming que insere a resposta como nó ou ramo. Sem motor local: os
  modelos são os do provedor que você configurar.
- **Minimapa** no canto inferior direito, com o viewport marcado e clique/arrasto para navegar.
- **Export** em Markdown, HTML, SVG e PNG (View/Arquivo → Exportar). O HTML leva as cores do tema;
  o PNG é o mesmo desenho da tela em 2×.
- **Layout equilibrado nas quatro direções**: a raiz no centro, os ramos distribuídos à esquerda,
  à direita, para cima e para baixo, com o lado automático escolhido pelo lado mais leve.
- **Navegação**: arraste para mover o mapa, roda do mouse (ou pinça) para dar zoom no ponto sob o
  cursor, "Ajustar à janela" para enquadrar tudo.
- **Abrir, salvar e salvar como** `.tmind` — o JSON que o LocalMind (Electron) grava, lido e escrito
  campo a campo: um arquivo com campos que esta versão não conhece é preservado, não reescrito.
  Nó com `width: 0`, raiz duplicada e nó órfão (as armadilhas que o app antigo documenta) abrem sem
  perder nada. Gravação atômica: um erro no meio não deixa o mapa pela metade.
- **Abrir por associação de arquivo**, por argumento (`localmind mapa.tmind`) e arrastando o
  arquivo para a janela.
- **Interface em português, espanhol, japonês e inglês** (o inglês é a fonte; o idioma segue o
  sistema e a escolha fica salva).
- **Janela About com verificação de atualizações** (`craft-update`), MSI no Windows, AppImage/deb/
  rpm/tar e pacote Arch no Linux.

## O que ainda vem

A paridade final (tabela, Ajuda, docs) fecha na v1.0.0 — ver `UPSTREAM-SYNC.md`. No backlog do app:
mover/realocar filhos por arrasto (Ctrl+arrasto, com diálogo de "novo pai" como plano B) e um tema
claro no padrão craft, além dos seis herdados.

## Formato

`.tmind` é JSON:

```json
{
  "id": "a1b2c3d4",
  "title": "Meu mapa",
  "createdAt": 1731600000000,
  "updatedAt": 1731600000000,
  "nodes": [
    { "id": "r00t0000", "text": "Ideia central", "parentId": null },
    { "id": "br4nch00", "text": "Ramo", "parentId": "r00t0000", "side": "right" }
  ]
}
```

Cada nó aceita `collapsed`, `color` (`#rrggbb`), `note`, `width`, `fontSize` e `side`
(`left`/`right`/`up`/`down`, só nos filhos da raiz). A raiz é o **primeiro** nó com `parentId:
null` — a mesma regra que o layout e os exports usam.

## Build e verificação

```
cargo build --locked          # debug
CRAFT_FONTS_DIR=assets/craft-fonts cargo build --locked   # com os CJK embutidos (como no CI)
cargo test --locked
cargo xtask fmt && cargo xtask assets && cargo xtask layers
packaging/windows/package.ps1 # MSI + portable (Windows)
packaging/linux/package.sh    # AppImage/deb/rpm/tar (Linux)
```

Release: `cargo xtask version set X.Y.Z` **antes** de cada tag → tag `vX.Y.Z` → o Actions builda e
publica. Nada de `target/` para trás: `cargo clean` depois do uso local.

## Licença

MIT OR Apache-2.0, como o resto da suíte (`LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`).
