#requires -Version 5.1
<#
.SYNOPSIS
  End-to-end smoke test: the agent writes markdown and converts it to Word
  through the package-owned docx_export tool.

.DESCRIPTION
  Runs the real platform in headless `--chat-async` mode against the selected
  profile, then checks that both `note.md` and a valid `note.docx` appeared in
  the app workspace. Requires a configured LLM endpoint/key in the profile.

.EXAMPLE
  powershell -File tools\e2e-docx.ps1
  powershell -File tools\e2e-docx.ps1 -DataHome .data -AppId doc-writer
#>
[CmdletBinding()]
param(
  [string]$DataHome = "",
  [string]$AppId = "doc-writer",
  [string]$Platform = "",
  [string]$Message = "Create note.md in the workspace with a short verification plan, then call docx_export to produce note.docx.",
  [switch]$KeepFiles
)

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent $PSScriptRoot
if (-not $DataHome) {
  $DataHome = Join-Path $repo ".data"
}
if (-not $Platform) {
  $Platform = Join-Path $repo "src-tauri\target\debug\eco-platform.exe"
}
if (-not (Test-Path -LiteralPath $Platform)) {
  throw "platform binary not found: $Platform (run cargo build first)"
}

$workspace = Join-Path $DataHome "workspaces\$AppId"
$configPath = Join-Path $DataHome "config.json"
if (Test-Path -LiteralPath $configPath) {
  $config = Get-Content -LiteralPath $configPath -Raw -Encoding UTF8 | ConvertFrom-Json
  if ($config.workspaces) {
    $entry = $config.workspaces.PSObject.Properties[$AppId]
    if ($entry -and $entry.Value) {
      $workspace = [string]$entry.Value
    }
  }
}

if (-not (Test-Path -LiteralPath $workspace)) {
  New-Item -ItemType Directory -Path $workspace | Out-Null
}

$markdown = Join-Path $workspace "note.md"
$docx = Join-Path $workspace "note.docx"
if (-not $KeepFiles) {
  Remove-Item -LiteralPath $markdown, $docx -Force -ErrorAction SilentlyContinue
}

$env:ECO_PLATFORM_HOME = $DataHome
Write-Host "profile:   $DataHome"
Write-Host "workspace: $workspace"
Write-Host "prompt:    $Message"

$output = & $Platform --chat-async $AppId $Message 2>&1
$code = $LASTEXITCODE
$output | ForEach-Object { Write-Host $_ }
if ($code -ne 0) {
  throw "agent run failed with exit code $code"
}

if (-not (Test-Path -LiteralPath $markdown)) {
  throw "agent did not create $markdown"
}
if (-not (Test-Path -LiteralPath $docx)) {
  throw "agent did not create $docx"
}

$bytes = [System.IO.File]::ReadAllBytes($docx)
if ($bytes.Length -lt 4 -or $bytes[0] -ne 0x50 -or $bytes[1] -ne 0x4B) {
  throw "$docx is not a ZIP/Word container"
}

Write-Host ("e2e ok: {0} ({1} bytes) -> {2} ({3} bytes)" -f `
  $markdown, (Get-Item -LiteralPath $markdown).Length, `
  $docx, $bytes.Length)
