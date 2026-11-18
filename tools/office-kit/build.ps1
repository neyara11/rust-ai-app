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
    & $python -m pip install --no-index --find-links $Wheelhouse python-docx openpyxl Pillow pyinstaller
} else {
    & $python -m pip install --upgrade pip
    & $python -m pip install python-docx openpyxl Pillow pyinstaller
}

& (Join-Path $here ".venv\Scripts\pyinstaller.exe") `
    --noconfirm `
    --clean `
    --onefile `
    --name office-kit `
    --distpath dist `
    --workpath build `
    --specpath . `
    office_kit.py

$exe = Join-Path $here "dist\office-kit.exe"
if (-not (Test-Path -LiteralPath $exe)) {
    throw "office-kit was not built: $exe"
}

$packageBin = Join-Path $here "..\..\dev-apps\office-writer\bin"
New-Item -ItemType Directory -Force -Path $packageBin | Out-Null
Copy-Item -LiteralPath $exe -Destination (Join-Path $packageBin "office-kit.exe") -Force

Write-Host "built: $exe"
Write-Host "copied into package: $packageBin\office-kit.exe"
Write-Host "update the manifest hashes and re-sign the package"
