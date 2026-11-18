# Third-party notices

`office-writer` bundles two Rust binaries.

## diagram-render

- `merman` — Mermaid parsing, layout and rendering in Rust.
  Copyright the Merman contributors. Licensed under MIT OR Apache-2.0.
  This build uses the Cytoscape layout and does not include the EPL-2.0 ELK
  implementation. Source: https://github.com/Latias94/merman
- `charts-rs` — chart rendering without a browser.
  Copyright Tree Xie and contributors. Licensed under Apache-2.0.
  Source: https://github.com/vicanso/charts-rs
- `resvg` / `usvg` / `tiny-skia` — SVG rasterization (MPL-2.0 / Apache-2.0 /
  BSD-3-Clause) as pulled transitively by the crates above.

## office-kit

Built with PyInstaller from:

- `python-docx` — MIT.
- `openpyxl` — MIT.
- `Pillow` — HPND.
- `lxml` — BSD-3-Clause.

Full license texts are available from the upstream projects and from the
Python package metadata of the build environment.
