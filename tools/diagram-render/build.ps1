param()

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $here

& cargo build --release --manifest-path (Join-Path $here "Cargo.toml")
if ($LASTEXITCODE -ne 0) {
    throw "diagram-render build failed"
}

$exe = Join-Path $here "target\release\diagram-render.exe"
if (-not (Test-Path -LiteralPath $exe)) {
    throw "diagram-render was not built: $exe"
}

$packageBin = Join-Path $here "..\..\dev-apps\office-writer\bin"
New-Item -ItemType Directory -Force -Path $packageBin | Out-Null
Copy-Item -LiteralPath $exe -Destination (Join-Path $packageBin "diagram-render.exe") -Force

Write-Host "built: $exe"
Write-Host "copied into package: $packageBin\diagram-render.exe"
Write-Host "update the manifest hashes and re-sign the package"
