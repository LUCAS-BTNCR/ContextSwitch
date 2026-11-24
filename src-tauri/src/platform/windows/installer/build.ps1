# Build de Windows: instalador ContextSwitch_<versión>_x64-setup.exe en release\windows\.
# Uso (desde la raíz del repo): npm run build:windows
$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..\..")
Set-Location $Root

# Tauri valida que el recurso cs-bridge.exe exista ANTES de compilarlo: un marcador vacío
# alcanza; beforeBuildCommand lo reemplaza por el binario real.
New-Item -ItemType Directory -Force src-tauri\target\release | Out-Null
if (-not (Test-Path src-tauri\target\release\cs-bridge.exe)) { New-Item -ItemType File src-tauri\target\release\cs-bridge.exe | Out-Null }

npm run tauri build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$Out = "release\windows"
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force $Out | Out-Null
Copy-Item src-tauri\target\release\bundle\nsis\*-setup.exe $Out
Copy-Item src-tauri\target\release\contextswitch.exe $Out
Copy-Item src-tauri\target\release\cs-bridge.exe $Out
Write-Host "Listo: $Out"
Get-ChildItem $Out | ForEach-Object { $_.Name }
