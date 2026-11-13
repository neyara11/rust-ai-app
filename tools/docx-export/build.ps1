param(
    [string]$Wheelhouse = ""
)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $here

if (-not (Test-Path ".venv")) {
    python -m venv .venv
}

$python = Join-Path $here ".venv\Scripts\python.exe"

if ($Wheelhouse) {
    & $python -m pip install --no-index --find-links $Wheelhouse python-docx pyinstaller
} else {
    & $python -m pip install --upgrade pip
    & $python -m pip install python-docx pyinstaller
}

& (Join-Path $here ".venv\Scripts\pyinstaller.exe") `
    --noconfirm `
    --clean `
    --onefile `
    --name docx-export `
    --distpath dist `
    --workpath build `
    --specpath . `
    convert.py

$exe = Join-Path $here "dist\docx-export.exe"
if (-not (Test-Path $exe)) {
    throw "converter was not built: $exe"
}

$packageBin = Join-Path $here "..\..\dev-apps\doc-writer\bin"
New-Item -ItemType Directory -Force -Path $packageBin | Out-Null
Copy-Item -LiteralPath $exe -Destination (Join-Path $packageBin "docx-export.exe") -Force

Write-Host "built: $exe"
Write-Host "copied into package: $packageBin\docx-export.exe"
Write-Host "update the manifest hash and re-sign the package"
