<#
.SYNOPSIS
  Prueba de punta a punta de ContextSwitch con la app REAL compilada.

.DESCRIPTION
  - Usa una configuración aislada (APPDATA temporal): no toca tu profiles.json, tus sesiones
    ni el registro del puente de Zen (el exe se copia solo, sin cs-bridge al lado).
  - App de prueba: Mapa de caracteres (charmap.exe), que no guarda estado.
  - Mueve ventanas reales y, por la instancia única, cierra tu ContextSwitch mientras corre
    (al final lo vuelve a abrir si estaba abierto). Por eso NO corre por defecto.

  Flujos verificados:
    1. --activate abre la app del modo y la ubica en su zona (mitad izquierda del monitor 1).
    2. Instancia única: abrir otra vez deja un solo proceso.
    3. Cambio de contexto: al pasar a otro modo, la app del anterior se minimiza (onExit).
    4. Volver al modo: la ventana vuelve a su zona.
    5. Bug A: un profiles.json dañado se aparta (profiles.corrupt-*.json) y no se pisa.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\e2e.ps1
  powershell -ExecutionPolicy Bypass -File scripts\e2e.ps1 -Exe src-tauri\target\release\contextswitch.exe
#>
param(
  [string]$Exe = (Join-Path $PSScriptRoot "..\src-tauri\target\release\contextswitch.exe")
)
$ErrorActionPreference = "Stop"

Add-Type @'
using System; using System.Runtime.InteropServices; using System.Text;
public class E2E {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint c);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr SetProcessDpiAwarenessContext(IntPtr c);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int s);
  [DllImport("user32.dll")] public static extern bool EnumDisplayMonitors(IntPtr hdc, IntPtr clip, MonProc p, IntPtr l);
  public delegate bool MonProc(IntPtr m, IntPtr hdc, ref RECT r, IntPtr l);
  [DllImport("user32.dll")] public static extern bool GetMonitorInfo(IntPtr m, ref MONITORINFO mi);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct MONITORINFO { public int cbSize; public RECT Monitor; public RECT Work; public uint Flags; }
}
'@
# Coordenadas físicas, igual que la app (per-monitor v2).
[void][E2E]::SetProcessDpiAwarenessContext([IntPtr](-4))

$results = New-Object Collections.ArrayList
function Check($name, [bool]$ok, $detail = "") {
  [void]$results.Add([pscustomobject]@{ Resultado = $(if ($ok) { "PASS" } else { "FAIL" }); Prueba = $name; Detalle = $detail })
  Write-Host ("[{0}] {1} {2}" -f $(if ($ok) { "PASS" } else { "FAIL" }), $name, $detail)
}
function Wait-Until([scriptblock]$cond, [int]$ms = 15000) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($sw.ElapsedMilliseconds -lt $ms) { if (& $cond) { return $true }; Start-Sleep -Milliseconds 100 }
  return [bool](& $cond)
}
function Get-MainWindow($procName) {
  $ids = @(Get-Process $procName -ErrorAction SilentlyContinue | ForEach-Object { [uint32]$_.Id })
  $script:hw = [IntPtr]::Zero
  [E2E]::EnumWindows({ param($h, $l) $p = [uint32]0; [void][E2E]::GetWindowThreadProcessId($h, [ref]$p)
    if ($ids -contains $p -and [E2E]::IsWindowVisible($h) -and [E2E]::GetWindow($h, 4) -eq [IntPtr]::Zero) { $script:hw = $h; return $false }; $true }, [IntPtr]::Zero) | Out-Null
  $script:hw
}
function Get-Frame($h) { $r = New-Object E2E+RECT; [void][E2E]::DwmGetWindowAttribute($h, 9, [ref]$r, 16); $r }
function Get-Monitors {
  $list = New-Object Collections.ArrayList
  [E2E]::EnumDisplayMonitors([IntPtr]::Zero, [IntPtr]::Zero, { param($m, $hdc, [ref]$r, $l)
    $mi = New-Object E2E+MONITORINFO; $mi.cbSize = [Runtime.InteropServices.Marshal]::SizeOf($mi); [void][E2E]::GetMonitorInfo($m, [ref]$mi)
    [void]$list.Add($mi.Work); $true }, [IntPtr]::Zero) | Out-Null
  @($list | Sort-Object L, T)
}

$Exe = (Resolve-Path $Exe).Path
$userApp = Get-Process contextswitch -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Path
$work = Join-Path ([IO.Path]::GetTempPath()) ("cs-e2e-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
$appdata = Join-Path $work "appdata"; $bin = Join-Path $work "bin"
New-Item -ItemType Directory -Force (Join-Path $appdata "ContextSwitch"), $bin | Out-Null
Copy-Item $Exe (Join-Path $bin "ContextSwitch.exe")   # SIN cs-bridge: no registra el puente de Zen
$cs = Join-Path $bin "ContextSwitch.exe"
$charmap = Join-Path $env:WINDIR "System32\charmap.exe"
$origAppData = $env:APPDATA

function Write-Config($json) { [IO.File]::WriteAllText((Join-Path $appdata "ContextSwitch\profiles.json"), $json, (New-Object Text.UTF8Encoding $false)) }
function Activate($id) { $env:APPDATA = $appdata; Start-Process $cs -ArgumentList "--activate", $id; $env:APPDATA = $origAppData }

$config = @"
{ "version": 1,
  "settings": { "doubleTapWindowMs": 450, "swallowTrigger": true, "startWithWindows": false, "showToastOnSwitch": false, "activeProfileId": null },
  "profiles": [
    { "id": "e2e-a", "name": "E2E A", "enabled": true, "trigger": { "modifiers": ["ctrl","shift","alt"], "key": "F9", "vk": 120 },
      "apps": [ { "id": "cm", "label": "Charmap", "kind": "generic", "exePath": $(ConvertTo-Json $charmap), "processName": "charmap.exe",
                  "open": [], "onEnter": "launch-or-focus", "onExit": "minimize",
                  "placements": [ { "monitor": 0, "x": 0, "y": 0, "w": 0.5, "h": 1 } ] } ],
      "closeOnEnter": [] },
    { "id": "e2e-b", "name": "E2E B", "enabled": true, "trigger": { "modifiers": ["ctrl","shift","alt"], "key": "F10", "vk": 121 },
      "apps": [], "closeOnEnter": [] } ] }
"@

try {
  if (Get-Process charmap -ErrorAction SilentlyContinue) { throw "Cerrá el Mapa de caracteres antes de correr el E2E." }
  Write-Config $config
  $mon = (Get-Monitors)[0]; $halfRight = $mon.L + [int](($mon.R - $mon.L) / 2)

  # 1) activar A: abre charmap y lo ubica en la mitad izquierda del monitor 1
  Activate "e2e-a"
  $opened = Wait-Until { (Get-MainWindow "charmap") -ne [IntPtr]::Zero } 20000
  Check "1. --activate abre la app del modo" $opened
  Start-Sleep -Milliseconds 2500
  $h = Get-MainWindow "charmap"
  if ($h -ne [IntPtr]::Zero) {
    $f = Get-Frame $h
    $placed = [math]::Abs($f.L - $mon.L) -le 2 -and [math]::Abs($f.T - $mon.T) -le 2 -and $f.R -le $halfRight + 2
    Check "1. la ubica en su zona (mitad izquierda del monitor 1)" $placed "frame=$($f.L),$($f.T)→$($f.R),$($f.B) · zona=$($mon.L),$($mon.T)→$halfRight,$($mon.B)"
  }

  # 2) instancia única
  Activate "e2e-a"; Start-Sleep -Seconds 3
  $n = @(Get-Process contextswitch -ErrorAction SilentlyContinue).Count
  Check "2. instancia única: queda un solo proceso" ($n -eq 1) "procesos=$n"

  # 3) pasar a B: la app de A se minimiza (onExit = minimize)
  Activate "e2e-b"
  $min = Wait-Until { $x = Get-MainWindow "charmap"; $x -ne [IntPtr]::Zero -and [E2E]::IsIconic($x) } 10000
  Check "3. cambio de contexto: la app del modo anterior se minimiza" $min

  # 4) volver a A: vuelve a su zona
  Activate "e2e-a"
  $back = Wait-Until { $x = Get-MainWindow "charmap"; $x -ne [IntPtr]::Zero -and -not [E2E]::IsIconic($x) } 10000
  Start-Sleep -Milliseconds 1500
  $f = Get-Frame (Get-MainWindow "charmap")
  Check "4. al volver, la ventana vuelve a su zona" ($back -and [math]::Abs($f.L - $mon.L) -le 2) "frame=$($f.L),$($f.T)→$($f.R),$($f.B)"

  # 5) bug A: profiles.json dañado se aparta y no se pisa
  Get-Process contextswitch -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Milliseconds 500
  $corrupt = '{ "version": 1, ROTO'
  Write-Config $corrupt
  Activate "e2e-a"; Start-Sleep -Seconds 4
  $backup = Get-ChildItem (Join-Path $appdata "ContextSwitch") -Filter "profiles.corrupt-*.json" | Select-Object -First 1
  $kept = $backup -and ([IO.File]::ReadAllText($backup.FullName) -eq $corrupt)
  Check "5. bug A: el profiles.json dañado se guarda intacto aparte" $kept "$($backup.Name)"
}
finally {
  Get-Process contextswitch -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $cs } | Stop-Process -Force
  $cm = Get-MainWindow "charmap"; if ($cm -ne [IntPtr]::Zero) { [void][E2E]::PostMessage($cm, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) } # WM_CLOSE
  Start-Sleep -Milliseconds 500
  Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
  if ($userApp) { Start-Process $userApp; Write-Host "Se volvió a abrir tu ContextSwitch: $userApp" }
}

$results | Format-Table -AutoSize | Out-String | Write-Host
$failed = @($results | Where-Object Resultado -eq "FAIL").Count
if ($failed) { Write-Host "E2E: $failed prueba(s) fallaron" -ForegroundColor Red; exit 1 }
Write-Host "E2E: todo OK" -ForegroundColor Green
