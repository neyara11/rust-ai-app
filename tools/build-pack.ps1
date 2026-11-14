param(
    [string]$Thumbprint = "",
    [string]$Subject = "CN=ECO AIOps Dev",
    [switch]$SkipSign
)

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$manifest = Join-Path $repo "tools\pack\Cargo.toml"
$exe = Join-Path $repo "tools\pack\target\release\eco-pack.exe"

& cargo build --release --manifest-path $manifest
if ($LASTEXITCODE -ne 0) { throw "eco-pack build failed" }
if (-not (Test-Path -LiteralPath $exe)) { throw "eco-pack not built: $exe" }

$outDir = Join-Path $repo "vendor\pack"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$out = Join-Path $outDir "eco-pack.exe"
Copy-Item -LiteralPath $exe -Destination $out -Force

if (-not $SkipSign) {
    $cert = $null
    if ($Thumbprint) {
        $cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Thumbprint -eq $Thumbprint }
        if (-not $cert) { throw "certificate $Thumbprint not found in CurrentUser\My" }
    } else {
        $cert = Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert |
            Where-Object { $_.Subject -eq $Subject -and $_.HasPrivateKey -and $_.NotAfter -gt (Get-Date) } |
            Sort-Object NotAfter -Descending |
            Select-Object -First 1
    }
    if ($cert) {
        $signed = Set-AuthenticodeSignature -FilePath $out -Certificate $cert -HashAlgorithm SHA256
        if (-not $signed.SignerCertificate) {
            throw "cannot sign eco-pack: $($signed.StatusMessage)"
        }
        Write-Host "signed eco-pack: $($cert.Subject)"
    } else {
        Write-Warning "no code-signing certificate found; eco-pack left unsigned"
    }
}

$sig = Get-AuthenticodeSignature -FilePath $out
Write-Host "built: $out"
Write-Host ("signature: {0}" -f $sig.Status)
