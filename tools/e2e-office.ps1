#requires -Version 5.1
<#
.SYNOPSIS
  Offline end-to-end check for the office-writer package binaries.

.DESCRIPTION
  Exercises read_office, docx_export, xlsx_export, render_chart and
  render_mermaid directly against the package binaries. No LLM or platform
  process is required unless -Agent is set.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\e2e-office.ps1
  powershell -ExecutionPolicy Bypass -File tools\e2e-office.ps1 -Agent
#>
[CmdletBinding()]
param(
  [string]$DataHome = "",
  [string]$AppId = "office-writer",
  [string]$PackageDir = "",
  [string]$Platform = "",
  [switch]$Agent,
  [switch]$KeepFiles
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
if (-not $DataHome) { $DataHome = Join-Path $repo ".data" }
if (-not $PackageDir) { $PackageDir = Join-Path $repo "dev-apps\office-writer" }
if (-not $Platform) { $Platform = Join-Path $repo "src-tauri\target\debug\eco-platform.exe" }

$kit = Join-Path $PackageDir "bin\office-kit.exe"
$render = Join-Path $PackageDir "bin\diagram-render.exe"
foreach ($exe in @($kit, $render)) {
  if (-not (Test-Path -LiteralPath $exe)) { throw "missing package binary: $exe (run package build first)" }
}

$workspace = Join-Path $DataHome "workspaces\$AppId"
$configPath = Join-Path $DataHome "config.json"
if (Test-Path -LiteralPath $configPath) {
  $config = Get-Content -LiteralPath $configPath -Raw -Encoding UTF8 | ConvertFrom-Json
  if ($config.workspaces) {
    $entry = $config.workspaces.PSObject.Properties[$AppId]
    if ($entry -and $entry.Value) { $workspace = [string]$entry.Value }
  }
}
$dir = Join-Path $workspace "e2e-office"
if (Test-Path -LiteralPath $dir) { Remove-Item -LiteralPath $dir -Recurse -Force }
New-Item -ItemType Directory -Force -Path $dir | Out-Null

$utf8 = New-Object System.Text.UTF8Encoding($false)
function Write-Utf8([string]$path, [string]$text) { [System.IO.File]::WriteAllText($path, $text, $utf8) }
function Invoke-Tool([string]$exe, [string[]]$arguments) {
  $ErrorActionPreference = "Continue"
  $output = & $exe @arguments 2>&1
  return @{ Code = $LASTEXITCODE; Output = ($output -join "`n") }
}
function Assert-Ok($result, [string]$label) {
  if ($result.Code -ne 0) { throw "$label failed ($($result.Code)): $($result.Output)" }
}
function Assert-Fail($result, [string]$label) {
  if ($result.Code -eq 0) { throw "$label unexpectedly succeeded: $($result.Output)" }
}
function Assert-Zip([string]$path, [string]$label) {
  if (-not (Test-Path -LiteralPath $path)) { throw "$label missing: $path" }
  $bytes = [System.IO.File]::ReadAllBytes($path)
  if ($bytes.Length -lt 4 -or $bytes[0] -ne 0x50 -or $bytes[1] -ne 0x4B) { throw "$label is not a ZIP container" }
}
function Assert-Png([string]$path, [string]$label) {
  if (-not (Test-Path -LiteralPath $path)) { throw "$label missing: $path" }
  $bytes = [System.IO.File]::ReadAllBytes($path)
  if ($bytes.Length -lt 1024) { throw "$label is too small ($($bytes.Length) bytes)" }
  if ($bytes[0] -ne 0x89 -or $bytes[1] -ne 0x50) { throw "$label is not a PNG" }
}

$barJson = Join-Path $dir "chart-1.json"
$flow = Join-Path $dir "diagram-1.mmd"
$barPng = Join-Path $dir "chart-1.png"
$flowPng = Join-Path $dir "diagram-1.png"
$markdown = Join-Path $dir "report.md"
$docx = Join-Path $dir "report.docx"
$bookJson = Join-Path $dir "workbook.json"
$xlsx = Join-Path $dir "report.xlsx"
$extracted = Join-Path $dir "report.extracted.md"
$xlsxExtracted = Join-Path $dir "report.book.md"

Write-Host "package:   $PackageDir"
Write-Host "workspace: $dir"

Write-Utf8 $barJson '{"type":"bar","title":"Обращения","labels":["янв","фев","мар"],"datasets":[{"label":"план","data":[10,12,9]},{"label":"факт","data":[9,13,11]}]}'
Assert-Ok (Invoke-Tool $render @("chart", "--input", $barJson, "--output", $barPng)) "render_chart"
Assert-Png $barPng "chart PNG"

$hbarJson = Join-Path $dir "chart-2.json"
$hbarPng = Join-Path $dir "chart-2.png"
Write-Utf8 $hbarJson '{"type":"bar","indexAxis":"y","labels":["Word","Excel","PDF"],"datasets":[{"label":"документы","data":[5,3,2]}],"options":{"scales":{"x":{"stacked":true},"y":{"stacked":true}}}}'
Assert-Ok (Invoke-Tool $render @("chart", "--input", $hbarJson, "--output", $hbarPng)) "render_chart horizontal"
Assert-Png $hbarPng "horizontal chart PNG"

Write-Utf8 $flow "flowchart LR`n  A[Данные] --> B[Отчёт]`n  B --> C[Word]`n"
Assert-Ok (Invoke-Tool $render @("mermaid", "--input", $flow, "--output", $flowPng)) "render_mermaid"
Assert-Png $flowPng "mermaid PNG"

Write-Utf8 $markdown @"
# Отчёт

Проверка **инструментов** и ``кода``.

| Метрика | План | Факт |
| --- | --- | --- |
| Время | 10 | 12 |

![Обращения](chart-1.png)

![Схема](diagram-1.png)

- первый пункт
- второй пункт

## Вывод

Всё готово.
"@

Assert-Ok (Invoke-Tool $kit @("docx", "--input", $markdown, "--output", $docx)) "docx_export"
Assert-Zip $docx "docx"
Assert-Ok (Invoke-Tool $kit @("read", "--input", $docx, "--output", $extracted)) "read docx"
$docxText = Get-Content -LiteralPath $extracted -Raw -Encoding UTF8
if ($docxText -notmatch "Метрика" -or $docxText -notmatch "assets/") { throw "extracted docx markdown is incomplete" }
if (-not (Test-Path -LiteralPath (Join-Path $dir "report.extracted.assets"))) { throw "docx assets folder missing" }

Write-Utf8 $bookJson '{"title":"Книга","sheets":[{"name":"Метрики","columns":["Метрика","Январь","Февраль"],"rows":[["Вызовы",10,14],["Ошибки",2,1]],"widths":[24,12,12],"freeze":"A2","charts":[{"type":"bar","title":"Динамика","anchor":"E2","categories":"A2:A3","series":[{"name":"Январь","values":"B2:B3"},{"name":"Февраль","values":"C2:C3"}]}],"images":[{"path":"chart-1.png","anchor":"E20","width":4}]}]}'
Assert-Ok (Invoke-Tool $kit @("xlsx", "--input", $bookJson, "--output", $xlsx)) "xlsx_export"
Assert-Zip $xlsx "xlsx"
Assert-Ok (Invoke-Tool $kit @("read", "--input", $xlsx, "--output", $xlsxExtracted)) "read xlsx"
$xlsxText = Get-Content -LiteralPath $xlsxExtracted -Raw -Encoding UTF8
if ($xlsxText -notmatch "## Лист: Метрики" -or $xlsxText -notmatch "BarChart") { throw "extracted xlsx markdown is incomplete" }

$badMarkdown = Join-Path $dir "bad.md"
Write-Utf8 $badMarkdown "![x](C:\Windows\win.ini)`n"
Assert-Fail (Invoke-Tool $kit @("docx", "--input", $badMarkdown, "--output", (Join-Path $dir "bad.docx"))) "absolute image path"
$badInput = Join-Path $dir "legacy.doc"
Write-Utf8 $badInput "not a binary doc"
Assert-Fail (Invoke-Tool $kit @("read", "--input", $badInput, "--output", (Join-Path $dir "legacy.md"))) "legacy .doc"

if ($Agent) {
  if (-not (Test-Path -LiteralPath $Platform)) { throw "platform binary not found: $Platform" }
  $env:ECO_PLATFORM_HOME = $DataHome
  $message = "Create office-e2e.md with a short table and a bar chart, render the chart to PNG, export office-e2e.docx and office-e2e.xlsx, then report the created files."
  $output = & $Platform --chat-async $AppId $message 2>&1
  if ($LASTEXITCODE -ne 0) { throw "agent run failed: $($output -join "`n")" }
  foreach ($name in @("office-e2e.md", "office-e2e.docx", "office-e2e.xlsx")) {
    if (-not (Test-Path -LiteralPath (Join-Path $workspace $name))) { throw "agent did not create $name" }
  }
}

if (-not $KeepFiles) { Remove-Item -LiteralPath $dir -Recurse -Force -ErrorAction SilentlyContinue }
Write-Host "e2e ok: read, docx, xlsx, chart, mermaid"
