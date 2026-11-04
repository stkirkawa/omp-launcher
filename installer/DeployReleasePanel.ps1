[CmdletBinding()]
param(
    [string]$Server = "root@2.27.98.254",
    [string]$IdentityFile = (Join-Path $env:USERPROFILE ".ssh\kirkawa-deploy")
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path $PSScriptRoot -Parent
$panelRoot = Join-Path $PSScriptRoot "admin"
$ssh = "C:\Windows\System32\OpenSSH\ssh.exe"
$sshKeygen = "C:\Windows\System32\OpenSSH\ssh-keygen.exe"
$scp = "C:\Windows\System32\OpenSSH\scp.exe"
$panelIdentity = Join-Path $env:USERPROFILE ".ssh\kirkawa-panel-tunnel"

foreach ($path in @($ssh, $sshKeygen, $scp, $IdentityFile)) {
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Required deployment file is missing: $path"
    }
}
foreach ($file in @("admin_server.py", "index.html", "admin.css", "admin.js", "kirkawa-release-panel.service", "install.sh", "provision-tunnel.sh")) {
    if (-not (Test-Path -LiteralPath (Join-Path $panelRoot $file))) {
        throw "Release panel file is missing: $file"
    }
}

if (-not (Test-Path -LiteralPath $panelIdentity)) {
    $keyCommand = '"{0}" -t ed25519 -N "" -C kirkawa-release-panel-tunnel -f "{1}"' -f $sshKeygen, $panelIdentity
    & $env:ComSpec /c $keyCommand
    if ($LASTEXITCODE -ne 0) {
        throw "Could not create the dedicated SSH tunnel key."
    }
}
if (-not (Test-Path -LiteralPath "$panelIdentity.pub")) {
    throw "The dedicated SSH tunnel public key is missing."
}

$remote = "${Server}:/tmp/kirkawa-release-panel"
& $ssh -i $IdentityFile -o BatchMode=yes -o StrictHostKeyChecking=accept-new $Server `
    "mkdir -p /tmp/kirkawa-release-panel"
if ($LASTEXITCODE -ne 0) { throw "Could not prepare the server staging folder." }

foreach ($file in @("admin_server.py", "index.html", "admin.css", "admin.js", "kirkawa-release-panel.service", "install.sh", "provision-tunnel.sh")) {
    $localPath = Join-Path $panelRoot $file
    $uploadPath = $localPath
    $temporaryUpload = $null
    if ($file -match '\.(sh|service)$') {
        $temporaryUpload = Join-Path $env:TEMP ("kirkawa-panel-" + [guid]::NewGuid().ToString("N") + "-" + $file)
        $contents = [IO.File]::ReadAllText($localPath).Replace("`r`n", "`n").Replace("`r", "`n")
        [IO.File]::WriteAllText($temporaryUpload, $contents, [Text.UTF8Encoding]::new($false))
        $uploadPath = $temporaryUpload
    }
    try {
    & $scp -i $IdentityFile -o BatchMode=yes -o StrictHostKeyChecking=accept-new `
        $uploadPath "$remote/$file"
    } finally {
        if ($temporaryUpload -and (Test-Path -LiteralPath $temporaryUpload)) {
            Remove-Item -LiteralPath $temporaryUpload -Force
        }
    }
    if ($LASTEXITCODE -ne 0) { throw "Could not upload release panel file: $file" }
}

& $scp -i $IdentityFile -o BatchMode=yes -o StrictHostKeyChecking=accept-new `
    "$panelIdentity.pub" "$remote/panel-tunnel.pub"
if ($LASTEXITCODE -ne 0) { throw "Could not upload the panel's public SSH key." }

& $ssh -i $IdentityFile -o BatchMode=yes -o StrictHostKeyChecking=accept-new $Server `
    "bash /tmp/kirkawa-release-panel/install.sh"
if ($LASTEXITCODE -ne 0) {
    throw "Release panel deployment failed. Inspect the server service logs."
}

Write-Host "Release panel deployed. Run .\StartKirkawaReleasePanel.ps1 to open it securely."
