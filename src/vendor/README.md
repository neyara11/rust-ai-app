# Vendored offline assets

These files are served from the application bundle. The runtime makes **no**
network requests for them; CI/UI smoke fails if the page tries to load anything
outside the local origin.

| Asset | Version | License |
|---|---|---|
| `mermaid.min.js` | 11.17.2 | MIT (`LICENSE-mermaid.txt`) |
| `chart.umd.min.js` | 4.6.1 | MIT (`LICENSE-chartjs.txt`) |
| `highlight.min.js` | 11.12.0 (common build, 36 languages) | BSD-3-Clause (`LICENSE-highlightjs.txt`) |
| `highlight-powershell.min.js` | 11.12.0 (extra grammar) | BSD-3-Clause (`LICENSE-highlightjs.txt`) |

Fonts live in `../fonts/` (`@fontsource` 5.3.0): Golos Text, JetBrains Mono,
Syne. Their OFL licenses are next to the woff2 files.

Update procedure (developer machine, not part of the app runtime):

```powershell
npm install mermaid@11.17.2 chart.js@4.6.1 `
  @fontsource/golos-text@5.3.0 @fontsource/jetbrains-mono@5.3.0 @fontsource/syne@5.3.0
Copy-Item node_modules/mermaid/dist/mermaid.min.js src/vendor/
Copy-Item node_modules/chart.js/dist/chart.umd.min.js src/vendor/
```

`highlight.min.js` comes from the browser bundle in
`highlightjs/cdn-release` (the npm package no longer ships `dist/`), and the
PowerShell grammar is a cdnjs language file that registers itself on the global
`hljs`. Both must be the same version:

```powershell
Invoke-WebRequest "https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.12.0/build/highlight.min.js" -OutFile src/vendor/highlight.min.js
Invoke-WebRequest "https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.12.0/languages/powershell.min.js" -OutFile src/vendor/highlight-powershell.min.js
Invoke-WebRequest "https://cdn.jsdelivr.net/npm/highlight.js@11.12.0/LICENSE" -OutFile src/vendor/LICENSE-highlightjs.txt
```

The theme lives in `../styles.css` (`.md-code pre code.hljs` + `--code-*`
variables), not in a highlight.js theme file, so it follows the app palettes.
