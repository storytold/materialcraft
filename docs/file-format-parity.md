# File format parity

> The SBS row describes the current codec. The remaining assessment predates the 2026-11-05
> primitive rewrite. See [atomic-workflow.md](atomic-workflow.md) and [ROADMAP.md](../ROADMAP.md)
> for the current scope and remaining validation.

Current integration also provides workspace-scoped `material_export`, real Height displacement
from the assigned material, explicit normal-map convention, and linear bitmap/input previews.
The native SBS and primitive-library limitations in [atomic-workflow.md](atomic-workflow.md) remain.

> **Last reviewed:** 2026-11-04 · **Last updated:** 2026-11-08 · **Change:** SBS project format details · **Target:** the professional node-based material workflow

SBS packages are MaterialCraft's project format, read and written with its own clean-room code
from the format's public documentation and sample files. MaterialCraft also reads and writes
the open image formats material artists exchange.

| Format | Read | Write | Fidelity | Tests |
|---|---|---|---|---|
| `.sbs` project package (XML) | Native filters, graph instances, inputs/outputs, exposed values, function and pattern graphs, folders, annotations, and embedded or linked resources; unsupported content produces warnings and unknown nodes remain placeholders | Native SBS XML with embedded resources and linked package references | Supported structures have round-trip coverage; arbitrary SBS round trips and external application interoperability remain unverified | `formats::sbs::tests`, engine `native_sbs_is_the_default_save_format`, `package_save_and_open_round_trip`, `an_sbs_file_with_missing_nodes_opens_softly` |
| PNG | Yes (8/16-bit, gray and color) | Yes (8/16-bit) | 16-bit gray round-trips within 1/65535 | `formats::tests::export_every_format` |
| JPEG | Yes | Yes (8-bit) | Lossy | same |
| TGA | Yes | Yes (8-bit) | Lossless | same |
| TIFF | Yes | Yes (8/16-bit) | Lossless | same |
| BMP | Yes | Yes (8-bit) | Lossless | same |
| WEBP | Yes | Yes (8-bit lossless) | Lossless | same |
| OpenEXR | Yes | Yes (32-bit float RGB) | Float | same |
| Radiance HDR | Yes | Yes | Float (RGBE) | same |
| SVG, fonts (resources) | No | — | — | gap |
| Meshes for the 3D view (OBJ, glTF, FBX) | No | — | — | gap |
| `.mcmat` portable material (zip: manifest, cooked maps, source package) | Yes (opens the source package) | Yes | Maps at 8-bit sRGB (color) / 16-bit linear (data); exposed inputs, presets and input nodes in the manifest. Our own format, shared with TextureCraft, not the reference app's archive format | `formats::material::tests`, engine `material_cmds::tests`, TextureCraft `materialcraft_link` |

Exports write base color, emissive and specular through the sRGB curve and data maps (normal,
roughness, metallic, height, ambient occlusion, opacity) linear.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-08 | major | Document native SBS serialization and project file handling |
| 2026-11-04 | major | SBS packages become the main project format |
| 2026-11-02 | minor | `.mcmat` portable materials replace the published-archive gap |
| 2026-11-02 | major | First version |
