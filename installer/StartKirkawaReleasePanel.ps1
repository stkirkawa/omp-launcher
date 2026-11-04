[CmdletBinding()]
param(
    [string]$Server = "root@2.27.98.254",
    [string]$IdentityFile = (Join-Path $env:USERPROFILE ".ssh\kirkawa-panel-tunnel"),
    [ValidateRange(1024, 65535)]
    [int]$LocalPort = 8787
)

$ErrorActionPreference = "Stop"
$ssh = "C:\Windows\System32\OpenSSH\ssh.exe"
if (-not (Test-Path -LiteralPath $ssh)) {
    throw "OpenSSH is not installed: $ssh"
}
if (-not (Test-Path -LiteralPath $IdentityFile)) {
    throw "The Kirkawa deployment key was not found: $IdentityFile"
}

$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, $LocalPort)
try {
    $listener.Start()
} catch {
    throw "Local port $LocalPort is already in use. Close the existing panel tunnel and try again."
} finally {
    $listener.Stop()
}

$url = "http://127.0.0.1:$LocalPort/"
$forward = "127.0.0.1:{0}:127.0.0.1:8787" -f $LocalPort
$arguments = @(
    "-N",
    "-L", $forward,
    "-i", $IdentityFile,
    "-o", "BatchMode=yes",
    "-o", "ExitOnForwardFailure=yes",
    "-o", "ServerAliveInterval=30",
    "-o", "ServerAliveCountMax=3",
    $Server
)
$tunnel = Start-Process -FilePath $ssh -ArgumentList $arguments -PassThru -WindowStyle Hidden

try {
    $ready = $false
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        if ($tunnel.HasExited) {
            throw "SSH tunnel exited with code $($tunnel.ExitCode). Check your SSH key and server access."
        }
        $client = [Net.Sockets.TcpClient]::new()
        try {
            $client.Connect("127.0.0.1", $LocalPort)
            $ready = $true
            break
        } catch {
            Start-Sleep -Milliseconds 300
        } finally {
            $client.Dispose()
        }
    }
    if (-not $ready) {
        throw "The secure panel tunnel did not become ready."
    }

    Start-Process $url
    Write-Host "Kirkawa release panel: $url"
    Write-Host "The panel is available only while this window remains open."
    Write-Host "Press Enter to close the secure tunnel."
    [void][Console]::ReadLine()
} finally {
    if (-not $tunnel.HasExited) {
        Stop-Process -Id $tunnel.Id
        $tunnel.WaitForExit()
    }
}
