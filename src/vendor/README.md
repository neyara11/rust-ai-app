# Vendored offline assets

These files are served from the application bundle. The runtime makes **no**
network requests for them; CI/UI smoke fails if the page tries to load anything
outside the local origin.

| Asset | Version | License |
|---|---|---|
| `mermaid.min.js` | 11.17.2 | MIT (`LICENSE-mermaid.txt`) |
| `chart.umd.min.js` | 4.6.1 | MIT (`LICENSE-chartjs.txt`) |

Fonts live in `../fonts/` (`@fontsource` 5.3.0): Golos Text, JetBrains Mono,
Syne. Their OFL licenses are next to the woff2 files.

Update procedure (developer machine, not part of the app runtime):

```powershell
npm install mermaid@11.17.2 chart.js@4.6.1 `
  @fontsource/golos-text@5.3.0 @fontsource/jetbrains-mono@5.3.0 @fontsource/syne@5.3.0
Copy-Item node_modules/mermaid/dist/mermaid.min.js src/vendor/
Copy-Item node_modules/chart.js/dist/chart.umd.min.js src/vendor/
```
