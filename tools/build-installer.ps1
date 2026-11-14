param(
    [string]$Subject = "CN=ECO AIOps Dev",
    [string]$Thumbprint = "",
    [switch]$Trust,
    [switch]$SkipSign,
    [switch]$SkipVendor
)

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$srcTauri = Join-Path $repo "src-tauri"

if (-not $SkipVendor) {
    & (Join-Path $repo "tools\vendor.ps1")
}

$buildArgs = @("tauri", "build")
if (-not $SkipSign) {
    if (-not $Thumbprint) {
        $cert = Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert |
            Where-Object { $_.Subject -eq $Subject -and $_.HasPrivateKey -and $_.NotAfter -gt (Get-Date) } |
            Sort-Object NotAfter -Descending |
            Select-Object -First 1
        if (-not $cert) {
            $cert = New-SelfSignedCertificate `
                -Type CodeSigningCert `
                -Subject $Subject `
                -CertStoreLocation Cert:\CurrentUser\My `
                -KeyAlgorithm RSA `
                -KeyLength 3072 `
                -HashAlgorithm SHA256 `
                -KeyExportPolicy NonExportable `
                -NotAfter (Get-Date).AddYears(3)
            Write-Host "created self-signed code-signing certificate: $($cert.Thumbprint)"
        }
        $Thumbprint = $cert.Thumbprint
    } else {
        $cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Thumbprint -eq $Thumbprint }
        if (-not $cert) { throw "certificate $Thumbprint not found in CurrentUser\My" }
    }

    if ($Trust) {
        $cer = Join-Path $env:TEMP ("eco-authenticode-" + $cert.Thumbprint + ".cer")
        Export-Certificate -Cert $cert -FilePath $cer -Type CERT | Out-Null
        foreach ($store in @("Root", "TrustedPublisher")) {
            $location = "Cert:\CurrentUser\$store"
            $present = Get-ChildItem $location | Where-Object { $_.Thumbprint -eq $cert.Thumbprint }
            if (-not $present) {
                Import-Certificate -FilePath $cer -CertStoreLocation $location | Out-Null
                Write-Host "trusted certificate in CurrentUser\$store"
            }
        }
        Remove-Item -LiteralPath $cer -Force -ErrorAction SilentlyContinue
    }

    $override = @{ bundle = @{ windows = @{ certificateThumbprint = $Thumbprint } } } |
        ConvertTo-Json -Depth 5 -Compress
    $overridePath = Join-Path $env:TEMP ("eco-tauri-sign-" + $Thumbprint + ".json")
    [System.IO.File]::WriteAllText($overridePath, $override, (New-Object System.Text.UTF8Encoding($false)))
    $buildArgs += @("--config", $overridePath)
}

Push-Location $srcTauri
try {
    & cargo @buildArgs
    if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }
} finally {
    Pop-Location
    if ($overridePath -and (Test-Path -LiteralPath $overridePath)) {
        Remove-Item -LiteralPath $overridePath -Force
    }
}

$artifacts = @(Get-Item (Join-Path $srcTauri "target\release\eco-platform.exe"))
$signerCert = $null
if (-not $SkipSign) { $signerCert = $cert }
if ($signerCert) {
    foreach ($file in $artifacts) {
        if ((Get-AuthenticodeSignature -FilePath $file.FullName).SignerCertificate) { continue }
        $signed = Set-AuthenticodeSignature -FilePath $file.FullName -Certificate $signerCert -HashAlgorithm SHA256
        if (-not $signed.SignerCertificate) {
            throw "cannot sign $($file.Name): $($signed.StatusMessage)"
        }
        Write-Host "signed loose executable: $($file.Name)"
    }
}

$bundleDir = Join-Path $srcTauri "target\release\bundle"
if (Test-Path $bundleDir) {
    $artifacts += @(Get-ChildItem $bundleDir -Recurse -File | Where-Object { $_.Extension -in ".exe", ".msi" })
}
foreach ($file in $artifacts) {
    $sig = Get-AuthenticodeSignature -FilePath $file.FullName
    $signer = if ($sig.SignerCertificate) { $sig.SignerCertificate.Subject } else { "(none)" }
    Write-Host ("{0}: {1} | {2}" -f $file.Name, $sig.Status, $signer)
}
