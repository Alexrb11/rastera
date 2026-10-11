# Rastera logo and icons

The approved mark combines an image-frame-shaped **R** with a detached raster pixel on a charcoal app tile. The space outside the tile is transparent.

## Assets

| File | Use |
| --- | --- |
| `rastera.svg` | Editable vector master exported by Vectorize; README and tutorial logo. |
| `rastera-256.png` | Embedded native window icon. |
| `rastera.ico` | Windows executable icon with 16, 24, 32, 48, 64, 128 and 256 px frames. |
| `rastera-32.png` | Tutorial favicon. |
| `rastera-{size}.png` | Transparent PNG exports: 16, 20, 24, 32, 40, 48, 64, 96, 128, 192, 256, 512 and 1024 px. |
| `source.png` | Original generated proposal approved before vectorization. |
| `manifest.json` | Export dimensions, renderer version and SHA-256 checksums. |

Vector palette: charcoal **#191A1C** and blue **#6BABF5**. Preserve the transparent margins when using the app icon. Use the SVG for scalable layouts and the supplied ICO for Windows resources.

The portable application includes the SVG and favicon needed by its offline tutorial. Window and executable icons are embedded at build time.

## Regenerating exports

Generated assets are committed; building or running Rastera does not require Node.js. To regenerate the PNGs and ICO from the saved SVG, install the development-only renderer and run:

```powershell
npm install --prefix work/icon-tools sharp@0.35.5
$env:NODE_PATH = Join-Path (Get-Location) 'work/icon-tools/node_modules'
node scripts/generate-icons.cjs
```

The generator renders the vector master directly and packs the corresponding PNGs into the ICO without upscaling a small image. Check `manifest.json` after regeneration; renderer or platform changes may produce different PNG bytes. Do not overwrite `rastera.svg` when regenerating raster exports.

Windows builds additionally require the Windows SDK resource compiler for MSVC, or `windres` for GNU. Linux builds skip executable resources. No runtime asset download is used.

## Provenance

- Raster proposal generated with the built-in OpenAI image generation tool and approved by the project owner on 11 October 2026.
- Brief: a geometric R suggesting an image frame, one detached pixel, light blue on a charcoal rounded tile, transparent outer background and a clear silhouette at desktop icon sizes.
- Vectorized with [Vectorize](https://vectorize.stemonte.io), revision `62a61e56a4104c548224732ca3b8b873`: two colors, smoothing 20, transparency enabled, trimming disabled.
- The SVG is the original Vectorize export, preserved without path edits. PNGs and ICO were generated from that SVG with Sharp 0.35.5.
- Original proposal SHA-256: `ae766f55131b1b37392af14b050b4d648330819b103097324911855b593446ca`.

See the project [license](../../LICENSE).
