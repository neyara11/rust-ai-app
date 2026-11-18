param(
    [string]$App = "doc-writer",
    [string]$PackageDir = "",
    [string]$KeyPath = "",
    [string]$TrustedPath = "",
    [string]$DataHome = "",
    [string]$Wheelhouse = "",
    [switch]$SkipConverter,
    [switch]$SkipBuild,
    [switch]$NoInstall
)

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if (-not $PackageDir) {
    $PackageDir = Join-Path $repo (Join-Path "dev-apps" $App)
}
$PackageDir = (Resolve-Path -LiteralPath $PackageDir).Path
$manifestPath = Join-Path $PackageDir "manifest.json"
if (-not (Test-Path -LiteralPath $manifestPath)) {
    throw "manifest not found: $manifestPath"
}
if (-not $KeyPath) { $KeyPath = Join-Path $repo "dev\keys\dev.key" }
if (-not $TrustedPath) { $TrustedPath = Join-Path $repo "dev\keys\trusted.pub" }
if (-not $DataHome) {
    $DataHome = $env:ECO_PLATFORM_HOME
    if (-not $DataHome) { $DataHome = Join-Path $repo ".data" }
}
$DataHome = [System.IO.Path]::GetFullPath($DataHome)

$signerExe = Join-Path $repo "tools\signer\target\debug\signer.exe"
$platformExe = Join-Path $repo "src-tauri\target\debug\eco-platform.exe"

$manifest = [System.IO.File]::ReadAllText($manifestPath, [System.Text.Encoding]::UTF8) | ConvertFrom-Json

$hasConverter = @($manifest.tools | Where-Object { $_ -and $_.bin -eq "bin/docx-export.exe" }).Count -gt 0
$converterScript = Join-Path $repo "tools\docx-export\build.ps1"
if (-not $SkipConverter -and $hasConverter -and (Test-Path -LiteralPath $converterScript)) {
    $converterArgs = @()
    if ($Wheelhouse) { $converterArgs += @("-Wheelhouse", $Wheelhouse) }
    & $converterScript @converterArgs
    if ($LASTEXITCODE -ne 0) { throw "converter build failed" }
}

$hasOfficeKit = @($manifest.tools | Where-Object { $_ -and $_.bin -eq "bin/office-kit.exe" }).Count -gt 0
$officeKitScript = Join-Path $repo "tools\office-kit\build.ps1"
if (-not $SkipConverter -and $hasOfficeKit -and (Test-Path -LiteralPath $officeKitScript)) {
    $officeKitArgs = @()
    if ($Wheelhouse) { $officeKitArgs += @("-Wheelhouse", $Wheelhouse) }
    & $officeKitScript @officeKitArgs
    if ($LASTEXITCODE -ne 0) { throw "office-kit build failed" }
}

$hasDiagramRender = @($manifest.tools | Where-Object { $_ -and $_.bin -eq "bin/diagram-render.exe" }).Count -gt 0
$diagramRenderScript = Join-Path $repo "tools\diagram-render\build.ps1"
if (-not $SkipConverter -and $hasDiagramRender -and (Test-Path -LiteralPath $diagramRenderScript)) {
    & $diagramRenderScript
    if ($LASTEXITCODE -ne 0) { throw "diagram-render build failed" }
}

if (-not $SkipBuild) {
    if (-not (Test-Path -LiteralPath $signerExe)) {
        & cargo build --manifest-path (Join-Path $repo "tools\signer\Cargo.toml")
        if ($LASTEXITCODE -ne 0) { throw "signer build failed" }
    }
    if (-not (Test-Path -LiteralPath $platformExe)) {
        & cargo build --manifest-path (Join-Path $repo "src-tauri\Cargo.toml")
        if ($LASTEXITCODE -ne 0) { throw "platform build failed" }
    }
}
if (-not (Test-Path -LiteralPath $signerExe)) { throw "signer not built: $signerExe" }
if (-not (Test-Path -LiteralPath $platformExe)) { throw "platform not built: $platformExe" }

function Get-FileSha256([string]$path) {
    (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
}

$targets = New-Object System.Collections.Generic.List[string]

function Add-HashTarget([string]$rel) {
    if (-not $rel) { return }
    $rel = $rel -replace '\\', '/'
    if (-not $targets.Contains($rel)) { $targets.Add($rel) }
}

Add-HashTarget $manifest.profile.system_prompt

$skillsDir = Join-Path $PackageDir "skills"
if (Test-Path -LiteralPath $skillsDir) {
    Get-ChildItem -LiteralPath $skillsDir -Recurse -File | Sort-Object FullName | ForEach-Object {
        Add-HashTarget ($_.FullName.Substring($PackageDir.Length + 1))
    }
}

foreach ($tool in @($manifest.tools)) {
    if ($tool -and $tool.bin) { Add-HashTarget $tool.bin }
}

foreach ($name in $manifest.hashes.PSObject.Properties.Name) { Add-HashTarget $name }

$entries = @()
foreach ($rel in $targets) {
    $full = Join-Path $PackageDir ($rel -replace '/', '\')
    if (-not (Test-Path -LiteralPath $full)) { throw "hashed file missing: $rel" }
    $entries += [pscustomobject]@{ Path = $rel; Hash = (Get-FileSha256 $full) }
}

$text = [System.IO.File]::ReadAllText($manifestPath, [System.Text.Encoding]::UTF8)
$nl = "`n"
if ($text.Contains("`r`n")) { $nl = "`r`n" }
$bodyLines = @(foreach ($entry in $entries) {
    '    "' + $entry.Path + '": "sha256:' + $entry.Hash + '"'
})
$body = $bodyLines -join (',' + $nl)

$hashesRx = [regex]'(?s)("hashes"\s*:\s*\{)(.*?)(\})'
if (-not $hashesRx.IsMatch($text)) { throw "manifest has no hashes object" }
$evaluator = { param($m) $m.Groups[1].Value + $nl + $body + $nl + '  ' + $m.Groups[3].Value }
$text = $hashesRx.Replace($text, $evaluator, 1)

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($manifestPath, $text, $utf8NoBom)

& $signerExe sign $manifestPath --key $KeyPath
if ($LASTEXITCODE -ne 0) { throw "signing failed" }

$homeTrusted = Join-Path $DataHome "trusted.pub"
if ((Test-Path -LiteralPath $homeTrusted) -and (Test-Path -LiteralPath $TrustedPath)) {
    $a = [System.IO.File]::ReadAllText($homeTrusted, [System.Text.Encoding]::UTF8).Trim().ToLowerInvariant()
    $b = [System.IO.File]::ReadAllText($TrustedPath, [System.Text.Encoding]::UTF8).Trim().ToLowerInvariant()
    if ($a -ne $b) {
        Write-Warning "trusted.pub in $DataHome differs from $TrustedPath; run: eco-platform --trust $TrustedPath"
    }
}

$env:ECO_PLATFORM_HOME = $DataHome
$verifyRaw = & $platformExe --verify $PackageDir
$verifyCode = $LASTEXITCODE
$verifyRaw | Write-Host
if ($verifyCode -ne 0) { throw "package verification failed" }
$report = ($verifyRaw -join "`n") | ConvertFrom-Json
if (-not $report.ok) { throw "package verification failed" }

if (-not $NoInstall) {
    $installRaw = & $platformExe --install $PackageDir
    $installCode = $LASTEXITCODE
    $installRaw | Write-Host
    if ($installCode -ne 0) {
        $hint = ""
        if (($installRaw -join "`n") -match "занят|in use|os error 32") {
            $hint = " A running app or a leftover opencode engine holds the installed copy; close it and retry."
        }
        throw "install failed.$hint"
    }
}

$exeEntry = $entries | Where-Object { $_.Path -like 'bin/*.exe' } | Select-Object -First 1
Write-Host ""
Write-Host "package: $PackageDir"
Write-Host "version: $($manifest.version)"
if ($exeEntry) { Write-Host "exe sha256: $($exeEntry.Hash)" }
Write-Host "verify: ok"
if (-not $NoInstall) { Write-Host "install: ok ($DataHome)" }
