# MaterialCraft roadmap

**Current focus (2026-11-28):** ship the measured root-graph improvements and validated save/load fixes; carry the remaining parity work into the next release.

The root inventory records **494 exact cases out of 546 measured roots**, not complete
numerical compatibility. The other 52 remain deferred: [reference variability and remaining
root differences (#336)](https://github.com/storytold/materialcraft/issues/336) and
[two older linked-dependency cases (#337)](https://github.com/storytold/materialcraft/issues/337).
[Review edge cases and older-document rendering compatibility (#338)](https://github.com/storytold/materialcraft/issues/338)
also need investigation. Corrected SBS conventions can change renders of documents authored
against earlier MaterialCraft behavior; keep original files when comparing versions.

The old Rust procedural library has been removed. The main catalog has 29 entries: 25 native
primitives and four graph input/output items. Functions have 85 operations; pattern graphs have
Quadrant, Switch, Iterate and Dot. See [the primitive workflow](docs/atomic-workflow.md) for usage and limits.

Earlier percentage estimates and node counts are superseded by this change. Full numerical
compatibility and arbitrary SBS round trips have not been established. The
[catalog status](docs/node-catalog-parity.md) records available nodes, implemented corrections
and behavior that remains untested.

## Next steps

1. Extend atomic sample and SBS interoperability coverage
2. Resolve the primitive, processor and map gaps in the catalog status
3. Validate the converted standard library and establish redistribution rights
4. Extend the SBS library beyond reusable functions to processing graphs

## Progress log

| Date | What landed |
|---|---|
| 2026-11-16 | Corrected six atomic nodes: filter kernels and Float32 arithmetic, rectangular scaling, independent tiling, channel selection and save/load, and empty Text/SVG backgrounds. [Behavior and validation](docs/atomic-parity-improvements.md) |
| 2026-11-15 | Corrected node defaults, typed unconnected inputs, missing-texture output and SBS serialization; updated catalog counts and remaining validation scope |
| 2026-11-08 | Explorer folder hierarchy and package/folder/graph context menus, native SBS empty folders and resource paths, undoable copy/move/rename/remove, and package reload/save-copy actions |
| 2026-11-08 | Portable `sbs://` function instances in the Library, preserved package folders and missing references, and a converted library retaining all 171 supplied function bodies (2,334 nodes); external runtime equivalence and redistribution rights remain unverified |
| 2026-11-02 | Cross-app materials: `.mcmat` portable materials, Send to TextureCraft, input-node preview images (mesh maps from TextureCraft), live re-cooks from TextureCraft through the CLI |
| 2026-11-02 | Visual function editor, dynamic parameters, presets and groups, 105 more nodes (flood fill, FX-Map, SVG, Text, 3D noises, weathering, material blending), mesh import, browser file I/O, 2D histogram, CLI `--set`, better samples |
| 2026-11-02 | First day: engine, ~120 nodes, cooking cache, packages, export, the full editor UI, 3D preview, CLI, control channel with synthetic input, MCP server, wasm builds |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-02 | minor | ≈ 66% → ≈ 67% (portable materials) |
| 2026-11-02 | major | ≈ 51% → ≈ 66% |
| 2026-11-02 | major | First version |
