# Local Mind

Mapas mentais da suíte Local: **Rust puro + egui forkado**, sem webview, sem React, sem conta e sem
telemetria. É o port do **LocalMind** (Electron) para o padrão craft — os mapas que você já tem
continuam abrindo, porque o formato é o mesmo arquivo `.tmind`.

## O que funciona

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

As fases seguintes (edição, estilos, export, IA) estão no `UPSTREAM-SYNC.md` com o estado de cada
uma. O que esta versão **não** faz: editar (o mapa abre só para leitura), exportar e conversar com
a IA.

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
