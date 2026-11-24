# App icon

The Local Mindmap icon: a central card with branches around it, on the suite's tile.

## Files

| File | What it is |
|---|---|
| `localmindmap.svg` | the master: the full-bleed 512 px tile (corner radius 100) Windows and Linux use as is |
| `localmindmap-small.svg` | a three-branch **tree** (not the hub shrunk) for 16, 20, 24 and 32 px only |
| `localmindmap-1024.png` | the master at 1024 px, for stores, README and the issue tracker |
| `hicolor/<size>x<size>/apps/com.localmind.app.png` | the freedesktop icon theme tree (16 to 512 px) and the runtime window icon (256 px) |
| `hicolor/scalable/apps/com.localmind.app.svg` | the scalable theme entry (a copy of the master) |
| `localmindmap.ico` | the Windows icon: 16, 20, 24, 32, 40, 48, 64, 128 and 256 px, packed by `cargo xtask ico` |

Every one of them is generated from the two SVGs by `packaging/icons.sh`; the outputs are committed,
so building and packaging never need `resvg`.

## The design

The family's frame, unchanged: a full-bleed tile with corner radius 100, a diagonal gradient, a
white glyph at ~70% of the tile and the "Taylor" signature at the bottom. What is this app's own:

- **Hue: lime** (`#84cc16` -> `#4d7c0f`), the one band of the colour wheel the suite did not use
  yet. Measured on the siblings' tiles: PDF red ~10°, Slides orange ~21°, Paint amber ~43°, Player
  crimson ~343°, Photo fuchsia ~289°, Keys and Obsidian indigo ~242-245°, Office and Image blue
  ~219°, Utils green ~158°, Sheets emerald ~160°, Vector teal ~175°. The empty band is 50-150°;
  the light stop sits at ~82°, in the middle of it.
- **The light stop is `#84cc16`, not `#a3e635`.** White-on-lime has to survive under the glyph:
  `#a3e635` gives ~1.5:1 against white (the upper branch melted into the bright quadrant of the
  gradient), `#84cc16` gives ~2.0:1 - the same ceiling the family's paint icon has.
- **Glyph: the map itself.** A central card with four smaller cards around it, joined by thin
  links - the hub the window draws. It is not a curve with nodes (Local Vector's glyph) and not a
  line chart (Local Slides'). The links are a third of a satellite's width on purpose: at bar width
  approaching the card's, the four arms fuse into one fat cross and the icon reads as a crosshair.
  The branches have different sizes because a map's branches are not equal - and because equal ones
  read as a plus.
- **A different silhouette for the small sizes.** At 16 px the hub reads as a plus/crosshair: the
  upper branch plus the two side arms make a symmetric cross, and the eye files it as "add". The
  small master drops the upper branch - one card, a trunk and three branches to the right, a tree
  that survives 16-32 px as "a map". The master is untouched for 40 px and up.

## Regenerating

```
packaging/icons.sh              # Linux/macOS (needs resvg; uses segoesc.ttf when present)
```

On Windows the same steps are `resvg -w N -h N --use-font-file C:\Windows\Fonts\segoesc.ttf
<src> <out.png>` per size (16/20/24/32 from `localmindmap-small.svg`, the rest from `localmindmap.svg`)
and then `cargo xtask ico assets/app-icon/localmindmap.ico <the 9 PNGs>`.

The signature is Segoe Script: without `--use-font-file` (or with fontdb unable to find the family)
resvg falls back to another face and the icon stops being deterministic.

QA of the set: the renders at 16/24/32/48/64/128/256/512, an 8x nearest-neighbour zoom of the 16 px
slice, a contact sheet next to the sibling apps' icons, and the pixel check (corner alpha 0, tile
corners rounded, glyph coverage) - the renders live in `target/icones-qa/`.
