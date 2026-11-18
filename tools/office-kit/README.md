# office-kit

Office tools for the `office-writer` mini-app. Builds into a single Windows
executable with PyInstaller, so the target machine needs no Python.

## Commands

```powershell
office-kit.exe read --input report.docx --output report.extracted.md
office-kit.exe docx --input report.md --output report.docx
office-kit.exe xlsx --input workbook.json --output report.xlsx
office-kit.exe xlsx --input report.md --output report.xlsx
```

`read` supports `.docx` and `.xlsx`, writes markdown plus a
`<output-stem>.assets/` folder, and prints a short JSON summary. Formulas are
kept as text and values are not recalculated. Sheets are capped at 200 rows and
40 columns in the markdown output.

`docx` accepts the markdown subset used by `doc-writer`, plus standalone image
lines `![Alt](path.png)`.

`xlsx` accepts either markdown tables (each `##` heading becomes a sheet) or a
`workbook.json` document with sheets, widths, number formats, merges, formulas,
native Excel charts and images.

## Build

```powershell
powershell -ExecutionPolicy Bypass -File .\build.ps1
```

With a closed contour, prepare a wheelhouse (`pip download -r requirements.txt`)
and pass `-Wheelhouse <dir>`.

The build copies `office-kit.exe` into `dev-apps\office-writer\bin`. Re-sign the
package after rebuilding.
