# MaterialCraft app icon

**Creature:** an engraved rhino, MaterialCraft's mascot.

**Style:** an engraving-style portrait in the Crafting Apps template: a tight head-and-shoulders portrait on a
full-bleed field, no frame or roundel, with the body running off the tile.

**Palette** (exactly three colours):

| Colour | Hex | Used for |
|---|---|---|
| Ink | `#0b0b0c` | line work and contour |
| Paper | `#efe9dc` | the figure |
| MaterialCraft teal | `#178a9c` | the full-bleed field; also the default UI accent "Cyan" (`crates/ui-egui/src/theme.rs`) |

**Tile:** `viewBox="0 0 512 512"`, a rounded square with `rx=112`, clipped. The macOS `.icns` adds
Apple's transparent margin (tile = 824/1024 of the canvas); Windows and Linux use the tile edge to edge.

**Provenance:** the owner's original drawing (2000 px), vectorised with craftrules
`assets/logo-options/_tools/vectorize_tile.py` at the same settings as the other Crafting App icons
(`materialcraft.svg`: 2048 px, turd size 2, `OPT=0.4 DP=1`; `materialcraft-small.svg`: 1024 px, turd
size 4, `OPT=1.0 DP=0`). `materialcraft.svg` is the canonical artwork. Licence: see `LICENSE.txt`.

## Files

| File | What | Used by |
|---|---|---|
| `materialcraft.svg` | master vector (traced at 2048 px, about 1.1 MB) | source for everything below |
| `materialcraft-small.svg` | lighter vector (traced at 1024 px, about 450 KB) | `hicolor/scalable` |
| `materialcraft-1024.png` | full tile, 1024 px | docs, store listings |
| `hicolor/<size>/apps/ai.storyteller.materialcraft.png` | 16–512 px PNGs | Linux packages; the 256 px one is the runtime window icon and the README icon |
| `materialcraft.ico` | Windows icon (16–256 px) | embedded in the `.exe` |
| `materialcraft.icns` | macOS icon (16–1024 px, Apple grid) | the `.app` bundle |

Regenerate everything but the two SVGs with `packaging/icons.sh`.

## Nightly channel

The nightly derivatives keep the foreground artwork, the alpha and the tile geometry unchanged.
Only the background contribution changes, from MaterialCraft teal (`#178a9c`) to the deeper
`#0e3a41`; there is no letter badge or crescent, and the app and the Launcher also name the
channel in text.

`materialcraft-nightly.png` and its nine-frame ICO use the committed full-bleed 256 master.
`materialcraft-nightly-1024.png` is the full-bleed 1024 master recoloured, and the eleven ICNS
slots are padded to Apple's 824/1024 grid from it and scaled down, never upscaled from 256.
Input and output hashes, the palette-fit recipe and the container sizes are recorded in
[`nightly-provenance.json`](nightly-provenance.json). Stable source files are not recoloured.
