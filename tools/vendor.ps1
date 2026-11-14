param(
    [string]$Source = "",
    [string]$ModelsPath = "",
    [string]$Out = "",
    [switch]$SkipModels
)

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if (-not $Out) { $Out = Join-Path $repo "vendor\engine" }

if (-not $Source) {
    $candidates = @(
        (Join-Path $repo "tools\oc\node_modules\opencode-ai\bin\opencode.exe"),
        (Join-Path $repo "tools\oc\node_modules\opencode-windows-x64\bin\opencode.exe")
    )
    $Source = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
}
if (-not $Source -or -not (Test-Path -LiteralPath $Source)) {
    throw "opencode.exe not found; run npm install in tools\oc or pass -Source"
}
$Source = (Resolve-Path -LiteralPath $Source).Path

if (-not $ModelsPath) {
    $candidates = @(
        (Join-Path $repo ".data\oc-home\cache\opencode\models.json"),
        (Join-Path $repo ".data\engine\cache\opencode\models.json")
    )
    $ModelsPath = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
}

$version = "unknown"
$pkg = Join-Path $repo "tools\oc\node_modules\opencode-ai\package.json"
if (Test-Path -LiteralPath $pkg) {
    $version = (Get-Content -LiteralPath $pkg -Raw | ConvertFrom-Json).version
}

New-Item -ItemType Directory -Force -Path $Out | Out-Null
$exeOut = Join-Path $Out "opencode.exe"
Copy-Item -LiteralPath $Source -Destination $exeOut -Force

$hashes = [ordered]@{}
$hashes["opencode.exe"] = (Get-FileHash -LiteralPath $exeOut -Algorithm SHA256).Hash.ToLowerInvariant()

if (-not $SkipModels) {
    if ($ModelsPath -and (Test-Path -LiteralPath $ModelsPath)) {
        $modelsOut = Join-Path $Out "models.json"
        Copy-Item -LiteralPath $ModelsPath -Destination $modelsOut -Force
        $hashes["models.json"] = (Get-FileHash -LiteralPath $modelsOut -Algorithm SHA256).Hash.ToLowerInvariant()
    } else {
        Write-Warning "models.json not found; the engine will rely on its embedded model list (no models.dev fetch at runtime)"
    }
}

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
$versionText = "opencode $version`nsource $Source`npacked $(Get-Date -Format "yyyy-MM-ddTHH:mm:ssK")`n"
[System.IO.File]::WriteAllText((Join-Path $Out "VERSION.txt"), $versionText, $utf8NoBom)
[System.IO.File]::WriteAllText((Join-Path $Out "sha256.json"), (($hashes | ConvertTo-Json) + "`n"), $utf8NoBom)

$total = (Get-ChildItem -LiteralPath $Out -File | Measure-Object Length -Sum).Sum
Write-Host "vendored opencode $version -> $Out"
foreach ($name in $hashes.Keys) {
    Write-Host ("  {0}  {1}" -f $hashes[$name], $name)
}
Write-Host ("total: {0:N1} MB" -f ($total / 1MB))
