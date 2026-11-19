# Registra el Native Messaging host para Zen (y Firefox) en HKCU. No requiere admin.
# Uso: .\register-host.ps1 -BridgeExe "C:\ruta\a\cs-bridge.exe"
param([Parameter(Mandatory)] [string] $BridgeExe)

$manifestDir = Join-Path $env:APPDATA "ContextSwitch"
New-Item -ItemType Directory -Force -Path $manifestDir | Out-Null
$manifestPath = Join-Path $manifestDir "contextswitch_bridge.json"

@{
  name = "contextswitch_bridge"
  description = "ContextSwitch local bridge"
  path = (Resolve-Path $BridgeExe).Path
  type = "stdio"
  allowed_extensions = @("companion@contextswitch.app")
} | ConvertTo-Json | ForEach-Object {
  # UTF-8 SIN BOM: Set-Content -Encoding UTF8 (PS 5.1) agrega BOM y Firefox/Zen no parsea el manifest.
  [IO.File]::WriteAllText($manifestPath, $_, (New-Object Text.UTF8Encoding $false))
}

# Zen lee la misma clave que Firefox (Mozilla).
$key = "HKCU:\Software\Mozilla\NativeMessagingHosts\contextswitch_bridge"
New-Item -Force -Path $key | Out-Null
Set-ItemProperty -Path $key -Name "(default)" -Value $manifestPath
Write-Host "OK -> $manifestPath"
