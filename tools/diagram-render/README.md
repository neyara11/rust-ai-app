# diagram-render

Offline Mermaid and chart renderer for the `office-writer` mini-app. Pure Rust,
no browser, no Node and no bundled Chromium.

```powershell
diagram-render.exe mermaid --input diagram.mmd --output diagram.png
diagram-render.exe chart --input chart.json --output chart.png
```

The output extension selects PNG or SVG (`.svg` for mermaid only).

## Mermaid

Uses `merman`, a headless Rust implementation that targets Mermaid 12.3 and
produces SVG/PNG. Text is measured and painted with system fonts, so Cyrillic
labels work without extra assets. Layout starts with the Cytoscape engine and
does not pull the EPL-2.0 ELK dependency.

## Charts

Uses `charts-rs` (Apache-2.0), which renders SVG/PNG without a browser and
accepts a JSON document. The input is the same Chat-style chart block used in
the platform UI:

```json
{
  "type": "bar",
  "title": "Calls",
  "labels": ["jan", "feb"],
  "datasets": [
    { "label": "plan", "data": [10, 12] },
    { "label": "fact", "data": [9, 13] }
  ]
}
```

Supported: `bar`, `line`, `pie`, `doughnut`, `radar`, `scatter`, bar/line mix
(`type` per dataset), area (`line` + `fill`), horizontal bars (`indexAxis: "y"`),
stacked series (`options.scales.*.stacked`) and a second axis (`yAxisID: "y1"`).
The legacy aliases `area` and `hbar` are accepted for export but are not valid
Chart.js types in the chat, so the documented format uses the canonical form. Unsupported chart types fail with an explicit message.

## Build

```powershell
powershell -ExecutionPolicy Bypass -File .\build.ps1
```

The build copies `diagram-render.exe` into `dev-apps\office-writer\bin`. Re-sign
the package after rebuilding.
