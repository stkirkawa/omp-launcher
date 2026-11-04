# Kirkawa Launcher installer

This folder contains two distinct installers:

- `KirkawaInstaller.csproj` builds the existing open.mp DLL helper.
- `KirkawaLauncherSetup.nsi` builds the per-user setup for the Tauri Kirkawa
  Launcher. It installs the launcher under `%LOCALAPPDATA%`, installs the
  Microsoft WebView2 Runtime when needed, and leaves the GTA folder untouched.

To compile the Tauri setup after producing the launcher executable:

1. Copy the release executable to `..\release\payload\KirkawaLauncher.exe`.
2. Install NSIS 3.13 or newer.
3. Run `makensis /INPUTCHARSET UTF8 KirkawaLauncherSetup.nsi` from this folder.
4. Find `KirkawaLauncherSetup.exe` in `..\release`.

The build requires `WebView2Bootstrapper.exe` from Microsoft's official
Evergreen Bootstrapper download. The installer checks for the matching
32-bit WebView2 Runtime before running the bootstrapper and verifies that the
runtime is registered afterward. It accepts the bootstrapper's
`-2147219416` (already installed) result only when that runtime is present.
No administrator access is requested. Player settings and downloaded game
files are not removed by the launcher uninstaller.

## Publishing a game build

`PublishGameBuild.ps1` creates a SHA-256 manifest from the exact contents of the
verified 7-Zip archive, uploads the archive and managed `SAMP` files over the
restricted SSH key, and asks the server to sign the manifest and release notes
with its private Ed25519 key. The private signing key never leaves the server.

Preview the manifest without uploading:

```powershell
.\PublishGameBuild.ps1 -Version 1.0.0 -ManifestOnly
```

Publish the build after reviewing `..\release\manifest.json` and the player-
facing release notes:

```powershell
.\PublishGameBuild.ps1 -Version 1.1.0 -Changelog "Исправлены ошибки и обновлена карта."
```

### Web release panel

The private web panel uploads one `.7z` archive, validates its paths and
integrity, extracts and hashes only the managed `SAMP` files, and signs both
the game manifest and versioned release notes on the server. It is deliberately
bound to the server's loopback interface and is not exposed through public
HTTP. Deployment creates a separate SSH key that can only open the panel's
local port and cannot run server commands. Open the panel through that tunnel:

```powershell
.\DeployReleasePanel.ps1
.\StartKirkawaReleasePanel.ps1
```

The launcher verifies the signed release notes and shows them before the
required update. The Play button remains unavailable if the signed update
check fails; after publication, players must install the current build before
the game can launch.

The updater verifies the signed manifest, downloads only changed project-
managed `SAMP` files, and only deletes files present in the prior valid signed
manifest. A first-time full game installation is accepted only into an empty
folder; the archive is path-validated and staged before the folder is replaced.

Build a self-contained Windows x64 executable from this directory:

```powershell
dotnet publish .\KirkawaInstaller.csproj -c Release -r win-x64 --self-contained true -p:PublishSingleFile=true
```

The executable installs the official open.mp client DLL into an existing
32-bit GTA: San Andreas + SA-MP folder. It downloads version metadata from
`api.open.mp`, accepts downloads only from `assets.open.mp`, verifies the
published MD5 and PE architecture, and preserves an existing `omp-client.dll`
in the user's local backup directory before updating it. It does not download
or redistribute GTA: San Andreas or a game build.

The player's game folder is selected on first run. The previously used folder
is saved in `%LOCALAPPDATA%\KirkawaLauncher\settings.json`.

The open.mp client DLL is a launcher injection component; copying it to the game
folder does not replace `samp.dll` or turn the game into an open.mp client by
itself.
