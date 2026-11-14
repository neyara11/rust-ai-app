param(
    [string]$Source = "",
    [string]$ModelsPath = "",
    [string]$Out = "",
    [string]$Lock = "",
    [string]$VersionOverride = "",
    [switch]$SkipModels,
    [switch]$UpdateLock
)

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if (-not $Out) { $Out = Join-Path $repo "vendor\engine" }
if (-not $Lock) { $Lock = Join-Path $repo "tools\engine.lock.json" }

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

$version = ""
$pkg = Join-Path $repo "tools\oc\node_modules\opencode-ai\package.json"
if (Test-Path -LiteralPath $pkg) {
    $version = (Get-Content -LiteralPath $pkg -Raw | ConvertFrom-Json).version
}
if ($VersionOverride) { $version = $VersionOverride }
if (-not $version) { $version = "unknown" }

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
$sourceHash = (Get-FileHash -LiteralPath $Source -Algorithm SHA256).Hash.ToLowerInvariant()
$modelsSourceHash = $null
if (-not $SkipModels -and $ModelsPath -and (Test-Path -LiteralPath $ModelsPath)) {
    $modelsSourceHash = (Get-FileHash -LiteralPath $ModelsPath -Algorithm SHA256).Hash.ToLowerInvariant()
}

$lockData = $null
if (Test-Path -LiteralPath $Lock) {
    $lockData = Get-Content -LiteralPath $Lock -Raw | ConvertFrom-Json
}

if ($lockData -and -not $UpdateLock) {
    if ($version -eq "unknown") {
        throw "engine version is unknown; pass -VersionOverride to verify against $Lock"
    }
    if ($lockData.version -ne $version) {
        throw "engine version mismatch: $Lock pins $($lockData.version), source is $version; use -UpdateLock only for an intentional engine upgrade"
    }
    if ($lockData.opencode_exe_sha256 -ne $sourceHash) {
        throw "opencode.exe hash mismatch: $Lock pins $($lockData.opencode_exe_sha256), source is $sourceHash; use -UpdateLock only for an intentional engine upgrade"
    }
    if ($modelsSourceHash -and $lockData.models_json_sha256 -and $lockData.models_json_sha256 -ne $modelsSourceHash) {
        throw "models.json hash mismatch: $Lock pins $($lockData.models_json_sha256), source is $modelsSourceHash; use -UpdateLock only for an intentional models refresh"
    }
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

$versionText = "opencode $version`nsource $Source`npacked $(Get-Date -Format "yyyy-MM-ddTHH:mm:ssK")`n"
[System.IO.File]::WriteAllText((Join-Path $Out "VERSION.txt"), $versionText, $utf8NoBom)
[System.IO.File]::WriteAllText((Join-Path $Out "sha256.json"), (($hashes | ConvertTo-Json) + "`n"), $utf8NoBom)

if ($UpdateLock) {
    if ($version -eq "unknown") {
        throw "cannot write $Lock with an unknown engine version; pass -VersionOverride"
    }
    $modelsHash = $null
    if ($hashes.Contains("models.json")) {
        $modelsHash = $hashes["models.json"]
    } elseif ($lockData -and $lockData.models_json_sha256) {
        $modelsHash = $lockData.models_json_sha256
    }
    $lockOut = [ordered]@{
        version = $version
        opencode_exe_sha256 = $hashes["opencode.exe"]
        models_json_sha256 = $modelsHash
    }
    [System.IO.File]::WriteAllText($Lock, (($lockOut | ConvertTo-Json) + "`n"), $utf8NoBom)
    Write-Host "engine lock updated: $Lock"
}

$total = (Get-ChildItem -LiteralPath $Out -File | Measure-Object Length -Sum).Sum
Write-Host "vendored opencode $version -> $Out"
if ($lockData -and -not $UpdateLock) { Write-Host "verified against $Lock" }
foreach ($name in $hashes.Keys) {
    Write-Host ("  {0}  {1}" -f $hashes[$name], $name)
}
Write-Host ("total: {0:N1} MB" -f ($total / 1MB))
