# ContextSwitch

Utilidad de bandeja del sistema para Windows (y Mac, beta) que cambia tu entorno completo (apps, workspace del navegador, audio, layout de ventanas) con un **Chorded Double-Tap**: mantenés `Ctrl+Shift` y tocás `G` → `G`.

**100% local · sin red · sin cuentas · sin registro de teclas.**

## Instalación

1. Descargá `ContextSwitch_0.5.0_x64-setup.exe` de [Releases](https://github.com/LUCAS-BTNCR/ContextSwitch/releases) y ejecutalo (por usuario, sin admin). Al iniciar con Windows pregunta qué contexto aplicar (cerrar la ventanita = ninguno); abrir la app de nuevo muestra los Ajustes de la que ya corre: nunca hay dos.
2. **Zen Browser** (opcional): instalá `contextswitch_companion-0.3.1.xpi` (firmada por Mozilla) desde `about:addons` → ⚙ → *Install Add-on From File…*. La app registra sola el puente local al arrancar.
3. En Zen, cada workspace debe tener un **contenedor por defecto**; ese contenedor es el que se pone en la regla de Zen. Activá *Switch to workspace where container is set as default*.

Detalles de Zen que la app respeta: reusa tus **Essentials** del mismo sitio en vez de abrir pestañas nuevas, y **nunca cierra ventanas de Zen** (cerrar una ventana borra los Essentials que contiene): al salir de un modo las oculta y al volver reaparecen. Para ventanas más angostas que 500 px (zonas de 25 %) Zen necesita un `userChrome.css` con `--window-min-width` menor.

Línea de comandos: `ContextSwitch.exe --activate <id-del-modo>`. Diagnóstico de la extensión: `%APPDATA%\ContextSwitch\bridge.log`.

### Mac (beta)

1. Compilá con `npm run build:mac` (requiere Rust y Node): deja `ContextSwitch.app` y el `.dmg` en `release/mac/`. Arrastrá la app a Aplicaciones. Vive en la **barra de menú**, sin ícono en el Dock.
2. **Permiso de Accesibilidad** (obligatorio): Ajustes del Sistema → Privacidad y seguridad → Accesibilidad → activá ContextSwitch. Sin él no funciona el atajo ni se pueden ubicar/minimizar ventanas; la app lo avisa con un botón que abre ese panel. Con builds sin firmar, cada build nueva hay que volver a activarlo.
3. Atajo por defecto: **⌃⌥ + tecla, dos veces** (`profiles.json` es el mismo formato que en Windows: ⌥ = `alt`, ⌘ = `win`).
4. **Chrome, Vivaldi, Brave, Edge, Firefox** (tipo `browser`): cada modo tiene **sus propias ventanas**. La primera vez se abren con las URLs del modo (opcional: una ventana por URL, cada una con su zona de pantalla); al salir del modo se minimizan o se cierran, y al volver reaparecen las mismas con lo que hayas navegado. No hace falta extensión: las ventanas se reconocen por su ID de ventana, sin leer títulos ni URLs.
5. **Zen** funciona como en Windows (misma extensión firmada); la app registra sola el puente en `~/Library/Application Support/{Mozilla,zen}/NativeMessagingHosts/`.

Diferencias con Windows: "Minimizar" una app = ocultarla (⌘H); "Cerrar" = salir (⌘Q) y la app pregunta si hay algo sin guardar; "Cerrar todo lo demás" nunca sale del Finder (solo cierra sus ventanas); "Iniciar con Windows" = LaunchAgent del usuario (`~/Library/LaunchAgents/app.contextswitch.plist`). Datos en `~/Library/Application Support/ContextSwitch/`.

Excepción honesta (como `IPolicyConfig` en Windows): para reconocer una ventana se usa `_AXUIElementGetWindow` (privada, estable desde 10.6; la usan Rectangle, yabai y AltTab). Cambiar el audio en Mac sí es API pública (Core Audio).

---

## 1. Arquitectura

```
┌──────────────────────────── contextswitch.exe (Tauri 2) ──────────────────────────────┐
│                                                                                       │
│  [hilo cs-keyboard-hook]          [hilo cs-worker]                [hilo principal]    │
│  WH_KEYBOARD_LL                   COM (MTA)                       Tauri + WebView2    │
│  └─ chord::Engine  ──Fire──mpsc──▶ actions::activate()           ├─ Tray (menú)       │
│     (máquina de estados,           ├─ window_ops  WM_CLOSE/foco  ├─ comandos IPC      │
│      sin historial)                ├─ audio       Core Audio     └─ React (Ajustes)   │
│        ▲                           ├─ layout      Get/SetWindowPlacement              │
│        │ PostThreadMessage         └─ bridge ─┐                                       │
│        │ (reconfig en caliente)               │ named pipe local                      │
│  profiles.json ◀── save_config ◀── UI         │ \\.\pipe\contextswitch-zen            │
└───────────────────────────────────────────────┼───────────────────────────────────────┘
                                                ▼
                     cs-bridge.exe (Native Messaging host, lo lanza Zen)
                                                │ stdio (u32 LE + JSON)
                                                ▼
                     Extensión "ContextSwitch Companion" (Zen)
                     permisos: nativeMessaging, contextualIdentities, cookies — nada más
```

| Capa | Archivo | Responsabilidad |
|---|---|---|
| Motor de chords | `src-tauri/src/hotkey/chord.rs` | Máquina de estados pura, sin Win32. 11 tests. |
| Hook | `src-tauri/src/platform/windows/hook.rs` · `platform/mac/hook.rs` | Hilo dedicado, `WH_KEYBOARD_LL` / `CGEventTap`, reconfig por mensaje. |
| Config | `src-tauri/src/profiles.rs` | Serde de `profiles.json`, escritura atómica. |
| Acciones | `src-tauri/src/platform/{windows,mac}/actions/*` | Cierre amable, foco/lanzamiento, audio, layout, puente Zen. |
| Puente | `src-tauri/src/bin/cs-bridge.rs` + `extension-zen/` | Pipe local ↔ Native Messaging ↔ extensión. |
| UI | `src/` | React/TS. Pestaña Ajustes con autoguardado. |

### Decisiones clave

- **El hook no trabaja.** Consulta el motor (O(perfiles)) y, si dispara, manda un `Fire` por canal. Windows elimina en silencio los hooks que superan `LowLevelHooksTimeout` (~1 s), así que audio, COM y ventanas van siempre al worker.
- **Cero locks en el hot path.** El estado vive en un `thread_local` del hilo del hook. La UI lo reconfigura con `PostThreadMessageW`.
- **Auto-repeat.** El hook LL no trae flag de repetición. Se detecta como un `KEYDOWN` de una disparadora que ya estaba abajo, y el segundo tap exige un `KEYUP` real antes.
- **Modificadores exactos.** `Ctrl+Shift+X` ≠ `Ctrl+Shift+Alt+X`. L/R son equivalentes.
- **"Reservar combinación"** (`swallowTrigger`). Se traga el KEYDOWN, sus repeats y su KEYUP, así nunca quedan KEYUP huérfanos. Si hay Win/Alt sostenido se inyecta `vkE8` para que al soltar no se abra Inicio ni la barra de menú (la misma técnica que usa AutoHotkey).
- **Eventos inyectados ignorados** (`LLKHF_INJECTED`) → no hay bucles con `SendInput`.
- **Cierre = WM_CLOSE.** Es lo mismo que clic en la X: si hay cambios sin guardar, la app muestra su diálogo. Nunca `TerminateProcess`.
- **Foco.** `AttachThreadInput` al hilo en foreground + `SetForegroundWindow` (forma documentada de pasar el foreground lock).
- **Privacidad verificable.** La CSP del WebView solo permite `ipc:`. No hay plugins `http`/`fs`/`shell` en `capabilities/`. El backend no tiene cliente HTTP. El selector de apps solo muestra nombre de proceso y ruta (no títulos de ventana, que pueden traer asuntos de mails).

### ⚠️ Dos excepciones honestas

1. **Audio predeterminado.** Windows **no tiene API pública** para cambiar el dispositivo de salida por defecto. Se usa `IPolicyConfig` (COM no documentado, estable desde Vista; lo usan EarTrumpet, SoundVolumeView y AudioDeviceCmdlets). Quedó aislado en `platform/windows/actions/audio.rs`. Enumerar dispositivos sí usa la API pública.
2. **Workspaces de Zen.** Zen no expone sus workspaces a WebExtensions. El puente usa lo que sí es público: Zen tiene la opción *"Switch to workspace where container is set as default when opening container tabs"*. Si cada workspace tiene un contenedor por defecto, abrir o activar una pestaña de ese contenedor salta al workspace. Por eso cada regla de Zen tiene `workspace` (nombre visible) + `container` (lo que usa el puente).

---

## 2. `profiles.json`

- Esquema: [`docs/profiles.schema.json`](docs/profiles.schema.json) (JSON Schema 2020-12)
- Ejemplo completo (Trabajo + Gaming, Zen, Spark, Discord, layout de 2 monitores): [`docs/profiles.example.json`](docs/profiles.example.json)
- Ubicación: `%APPDATA%\ContextSwitch\profiles.json`. Se escribe de forma atómica (tmp + rename).

Semántica al activar el modo **B** viniendo de **A**:

1. `B.closeOnEnter` → `WM_CLOSE` a esos procesos.
2. Cada app de `A` que **B no usa** → aplica su `onExit` (`keep` / `minimize` / `close`).
3. `B.audio` → dispositivo por defecto (consola + multimedia, y comunicaciones opcional).
4. Cada app de `B` → `onEnter`. Si es Zen: puente → workspace + URLs + unload. Si la extensión no responde, abre las URLs igual.
5. `B.layout` → `SetWindowPlacement` (espera 1,5 s si lanzó apps). El layout previo queda en memoria para **Deshacer**.

---

## 3. Máquina de estados (Chorded Double-Tap)

```
           mods exactos + T↓             T↑                     T↓ (Δ ≤ ventana)
  Idle ───────────────────────▶ Down1 ─────────▶ Up1 ───────────────────────────▶ Down2 ──T↑──▶ Idle
   ▲                             │ T↓ repeat      │ T↓ (Δ > ventana) → Down1(t0 nuevo)  │ repeats
   │                             │ = ignorado     │                                      │ ignorados
   └──── otra tecla / cambio de modificadores ────┴───────────── reset ─────────────────┘
```

Tests (`cargo test` o `rustc --test src/hotkey/chord.rs`): disparo en el segundo down, auto-repeat, ventana vencida, soltar un modificador, modificadores exactos, tecla ajena en el medio, L/R, mask de Win, wrap del tick de 32 bits, escritura normal intacta y swallow desactivado.

---

## 4. Plan MVP — fin de semana

**Requisitos:** Windows 10/11, Rust stable (MSVC), Node 20+, WebView2 (viene con Windows 11), Zen Browser.

### Viernes noche (1–2 h) · Esqueleto
- [ ] `npm install`, `npx tauri icon ruta/a/logo.png` (genera `src-tauri/icons/`).
- [ ] `npm run tauri dev` → aparece el ícono en la bandeja y la ventana de Ajustes desde el menú.
- [ ] `cargo test` en `src-tauri` (motor de chords en verde).
- [ ] Ajustar errores de compilación de la crate `windows` 0.58 si aparecen. El código está escrito contra esa versión, pero no se compiló en Windows: lo más probable son firmas de `CreateNamedPipeW`/`ShellExecuteW` y `PROPVARIANT::to_string`.

### Sábado mañana (3 h) · Hook real
- [ ] Probar el área **"Probar ahora"**: `Ctrl+Shift+G G` en Notepad, VS Code y un juego en ventana.
- [ ] Verificar: escribir normal no se afecta, mantener `G` apretada no dispara, `Win+…` no abre Inicio.
- [ ] Medir CPU en reposo con el Administrador de tareas (debe dar 0 %).

### Sábado tarde (3–4 h) · Acciones
- [ ] Cierre amable: modo que cierre Notepad con texto sin guardar → tiene que aparecer el diálogo.
- [ ] Lanzar/enfocar VS Code + abrir carpeta.
- [ ] Audio: listar dispositivos y cambiar entre auriculares y parlantes.
- [ ] Layout: capturar en 2 monitores, desordenar, activar el modo y después **Deshacer** desde la bandeja.

### Domingo mañana (3 h) · Zen
- [x] `cargo build --release --bin cs-bridge`
- [x] Registrar el host: la app lo hace sola al arrancar si `cs-bridge.exe` está junto a su exe (instalador o carpeta portable). En dev: `extension-zen\register-host.ps1 -BridgeExe src-tauri\target\release\cs-bridge.exe`.
- [ ] Zen → `about:debugging` → *Load Temporary Add-on* → `extension-zen/manifest.json`.
- [ ] Zen: crear contenedores (ej. *Oficina*, *Personal*), asignar cada uno como default de su workspace y activar *Switch to workspace where container is set as default*.
- [ ] Verificar: el modo Gaming salta al workspace *Entretenimiento*, abre YouTube la primera vez y descarga pestañas inactivas.
- [ ] Para uso permanente: firmar la extensión como **unlisted** en AMO (`npx web-ext sign --channel=unlisted`). Es gratis, automático y no queda publicada.

### Domingo tarde (2–3 h) · Pulido y release
- [ ] Toast al cambiar de modo (falta `tauri-plugin-notification`). Hechos: tooltip con el modo activo y check ✓ en el menú de la bandeja.
- [ ] Iniciar con Windows (ya escribe `HKCU\...\Run`).
- [x] `npm run build:windows` → instalador NSIS en `release\windows\`. Ya incluye `cs-bridge.exe`.
- [ ] Probar en una sesión limpia: borrar `%APPDATA%\ContextSwitch` y arrancar de cero.

### Fuera del MVP
Plugin `single-instance`, exportar/importar perfiles, perfiles por monitor conectado, más navegadores (Firefox comparte el mismo puente; Chromium necesita otra clave de registro y manifest MV3).

---

## Estructura

Dos apartados por sistema; todo lo demás es compartido.

```
contextswitch/
├─ src-tauri/
│  ├─ src/main.rs              tray, comandos, worker (compartido)
│  ├─ src/profiles.rs, paths.rs, updater.rs
│  ├─ src/hotkey/{mod,chord}.rs           motor del atajo (puro, compartido)
│  ├─ src/actions/{mod,protocol}.rs       interfaz común + mensajes a la extensión
│  ├─ src/platform/
│  │  ├─ windows/              ── APARTADO WINDOWS ──
│  │  │  ├─ actions/           ventanas Win32, audio (IPolicyConfig), layout, pipe de Zen…
│  │  │  ├─ hook.rs            WH_KEYBOARD_LL
│  │  │  ├─ system.rs          autoarranque (registro), host de Native Messaging
│  │  │  ├─ single_instance.rs mutex + eventos con nombre
│  │  │  ├─ e2e.ps1            prueba de punta a punta
│  │  │  └─ installer/         build.ps1 (.exe) + installer-hooks.nsh
│  │  └─ mac/                  ── APARTADO MAC ──
│  │     ├─ actions/           Accesibilidad, NSWorkspace, Core Audio, socket de Zen…
│  │     ├─ hook.rs, keys.rs   CGEventTap + teclas de Mac → códigos de Windows
│  │     ├─ system.rs          LaunchAgent, host de Native Messaging, permisos
│  │     ├─ single_instance.rs socket Unix
│  │     └─ installer/         build.sh (.app + .dmg)
│  ├─ src/bin/cs-bridge.rs     puente (pipe en Windows, socket en Mac)
│  ├─ tauri.conf.json          compartido
│  ├─ tauri.windows.conf.json  instalador NSIS
│  └─ tauri.macos.conf.json    .app + .dmg
├─ src/                        React (App, settings/*, keymap, platform, api, types)
├─ extension-zen/              manifest, background.js, register-host.ps1
├─ docs/                       profiles.schema.json, profiles.example.json
└─ release/{windows,mac}/      salida de `npm run build:windows` / `npm run build:mac`
```
