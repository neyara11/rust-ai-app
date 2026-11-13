# docx-export

Markdown → Word converter used by the platform export button. It is a Python
script packaged into a single Windows executable with PyInstaller, so the
target machine does not need Python.

## Build

```powershell
cd tools\docx-export
.\build.ps1
```

The result is `tools\docx-export\dist\docx-export.exe`. The build also copies it
into the `doc-writer` mini-app package:

```text
dev-apps\doc-writer\bin\docx-export.exe
```

After rebuilding, update the `bin/docx-export.exe` hash in the package manifest
and re-sign `manifest.json`. The executable is part of the signed package; the
platform does not search for it on its own.

## Use

```powershell
docx-export.exe --input report.md --output report.docx
```

The `doc-writer` app exposes this as the `docx_export` tool. The agent supplies
relative `input` and `output` paths inside the app's working folder; the gateway
runs the package binary with absolute paths.

## Offline build

For a closed contour, prepare a wheelhouse on a connected machine:

```powershell
python -m pip download python-docx pyinstaller -d wheelhouse
```

Copy `wheelhouse` and this directory into the contour, then build with:

```powershell
.\build.ps1 -Wheelhouse .\wheelhouse
```

The converter supports headings, paragraphs, bullet and numbered lists,
fenced code blocks, simple tables, bold/italic/inline code and blockquotes.
Markdown anchors `<!-- id: ... -->` are removed from the output.
