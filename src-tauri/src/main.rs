#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod hotkey;
mod paths;
mod profiles;
mod platform;
mod updater;

use platform::single_instance;
use platform::{config_path, refresh_autostart, register_zen_host, set_autostart};

use hotkey::chord::Fire;
use hotkey::hook::HookHandle;
use profiles::{Config, LayoutSnapshot};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};

struct AppState {
    path: PathBuf,
    config: Mutex<Config>,
    hook: Mutex<Option<HookHandle>>,
    undo: Mutex<Option<LayoutSnapshot>>,
    test_mode: AtomicBool,
    /// false si profiles.json estaba corrupto y no se pudo apartar: no se escribe en disco.
    persist: AtomicBool,
    /// Avisos del arranque para la UI (ej. "se recuperó la configuración").
    startup_warnings: Vec<String>,
}

impl AppState {
    fn save(&self, config: &Config) -> anyhow::Result<()> {
        if !self.persist.load(Ordering::Relaxed) {
            anyhow::bail!("no se guarda: profiles.json estaba dañado y no se pudo apartar");
        }
        config.save(&self.path)
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChordEvent { profile_id: String, profile_name: String, delta_ms: u32, test: bool }

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunningApp { process_name: String, exe_path: String }

// ── Comandos Tauri ───────────────────────────────────────────────────────────
#[tauri::command]
fn get_config(s: State<AppState>) -> Config { s.config.lock().unwrap().clone() }

#[tauri::command]
fn get_startup_warnings(s: State<AppState>) -> Vec<String> { s.startup_warnings.clone() }

#[tauri::command]
fn save_config(app: AppHandle, s: State<AppState>, config: Config) -> Result<(), String> {
    let config = config.accept_from_ui(&s.config.lock().unwrap()); // el modo activo es del backend
    s.save(&config).map_err(|e| e.to_string())?;
    set_autostart(config.settings.start_with_windows);
    let (b, c) = hotkey::bindings_from(&config);
    if let Some(h) = s.hook.lock().unwrap().as_ref() { h.reconfigure(b, c); }
    *s.config.lock().unwrap() = config;
    rebuild_tray_menu(&app);
    Ok(())
}

#[tauri::command]
fn list_audio_devices() -> Result<Vec<actions::audio::AudioDevice>, String> {
    actions::ensure_com();
    actions::audio::list_outputs().map_err(|e| e.to_string())
}

#[tauri::command]
fn list_running_apps() -> Vec<RunningApp> {
    let mut v: Vec<RunningApp> = actions::running_apps()
        .into_iter()
        .map(|(process_name, exe_path)| RunningApp { process_name, exe_path }) // sin títulos: pueden contener datos privados
        .collect();
    v.sort_by(|a, b| a.process_name.cmp(&b.process_name));
    v.dedup_by(|a, b| a.process_name == b.process_name);
    v
}

/// Íconos de los .exe como data URLs PNG (la CSP permite `img-src data:`).
/// `async` → corre fuera del hilo principal, no congela la ventana.
#[tauri::command(async)]
fn app_icons(paths: Vec<String>) -> std::collections::HashMap<String, String> {
    actions::ensure_com();
    paths.into_iter()
        .filter_map(|p| actions::icons::exe_icon_data_url(&p).map(|url| (p, url)))
        .collect()
}

/// true si la extensión de Zen está conectada (cs-bridge escuchando en el pipe).
#[tauri::command]
fn zen_bridge_connected() -> bool { actions::bridge::wait_ready(1) }

#[tauri::command]
fn list_monitors()-> Vec<actions::placement::MonitorInfo> { actions::placement::monitors() }

#[tauri::command]
fn capture_layout() -> LayoutSnapshot {
    actions::layout::capture(chrono::Local::now().to_rfc3339())
}

#[tauri::command]
fn undo_layout(s: State<AppState>) -> bool {
    match s.undo.lock().unwrap().take() {
        Some(prev) => { actions::layout::apply(&prev); true }
        None => false,
    }
}

#[tauri::command]
fn set_test_mode(s: State<AppState>, on: bool) { s.test_mode.store(on, Ordering::Relaxed); }

#[tauri::command]
fn activate_profile(app: AppHandle, id: String) -> Result<(), String> {
    let idx = app.state::<AppState>().config.lock().unwrap().profile_index(&id).ok_or("perfil inexistente")?;
    run_activation(&app, idx as u16, 0, false);
    Ok(())
}

/// Respuesta de la ventanita del arranque: `Some(id)` aplica ese modo, `None` = "no".
/// En ambos casos se cierra (no vuelve a aparecer hasta el próximo inicio de Windows).
#[tauri::command]
fn picker_choose(app: AppHandle, id: Option<String>) {
    if let Some(w) = app.get_webview_window("picker") { let _ = w.close(); }
    if let Some(id) = id {
        std::thread::spawn(move || { actions::ensure_com(); let _ = activate_profile(app, id); });
    }
}

/// Desde la ventanita: cerrarla y abrir los Ajustes completos.
#[tauri::command]
fn picker_open_settings(app: AppHandle) {
    if let Some(w) = app.get_webview_window("picker") { let _ = w.close(); }
    show_settings(&app);
}

fn show_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Ventanita "¿Deseas aplicar algún contexto?" que aparece al iniciar con Windows.
fn open_startup_picker(app: &AppHandle) -> tauri::Result<()> {
    let n = {
        let s = app.state::<AppState>();
        let cfg = s.config.lock().unwrap();
        cfg.profiles.iter().filter(|p| p.enabled).count()
    };
    if n == 0 { return Ok(()); } // sin modos no hay nada que preguntar
    let height = (200 + 62 * n).min(640) as f64;
    // Se crea oculta y se centra antes de mostrarla.
    let w = WebviewWindowBuilder::new(app, "picker", WebviewUrl::App("index.html".into()))
        .title("ContextSwitch")
        .inner_size(400.0, height)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .visible(false)
        .build()?;
    let _ = w.center();
    let _ = w.show();
    let _ = w.set_focus();
    Ok(())
}

// ── Actualizaciones (solo cuando el usuario lo pide) ─────────────────────────
#[tauri::command]
fn app_version() -> String { env!("CARGO_PKG_VERSION").to_string() }

#[tauri::command(async)]
fn check_update() -> Result<updater::UpdateInfo, String> { updater::check().map_err(|e| e.to_string()) }

/// Descarga e instala la versión nueva: la app se cierra y vuelve a abrirse actualizada.
#[tauri::command(async)]
fn install_update(app: AppHandle, url: String) -> Result<(), String> {
    updater::download_and_launch(&url).map_err(|e| e.to_string())?;
    // Windows: el instalador corre en silencio y reabre la app → hay que salir ya.
    // Mac: se abre la imagen .dmg; la app sigue hasta que el usuario reemplace la copia.
    if cfg!(windows) {
        actions::reveal_hidden_windows();
        app.exit(0);
    }
    Ok(())
}

/// "windows" o "macos": la UI ajusta textos (⌘ vs Win, .app vs .exe…).
#[tauri::command]
fn platform_name() -> &'static str { if cfg!(target_os = "macos") { "macos" } else { "windows" } }

/// Mac: ¿tiene el permiso de Accesibilidad? (sin él no hay atajo ni control de ventanas).
/// En Windows no hace falta ningún permiso: siempre true.
#[tauri::command]
fn accessibility_ok() -> bool { platform::accessibility_ok() }

/// Mac: abre Ajustes del Sistema en Privacidad → Accesibilidad (y muestra el aviso de macOS).
#[tauri::command]
fn open_accessibility_settings() { platform::open_accessibility_settings() }

// ── Activación ───────────────────────────────────────────────────────────────
fn run_activation(app: &AppHandle, idx: u16, delta_ms: u32, from_hook: bool) {
    let s = app.state::<AppState>();
    let (prev, next) = {
        let cfg = s.config.lock().unwrap();
        let Some(next) = cfg.profiles.get(idx as usize).cloned() else { return };
        let prev = cfg.settings.active_profile_id.as_ref()
            .and_then(|id| cfg.profiles.iter().find(|p| &p.id == id).cloned());
        (prev, next)
    };

    let test = from_hook && s.test_mode.load(Ordering::Relaxed);
    let _ = app.emit("chord-detected", ChordEvent {
        profile_id: next.id.clone(), profile_name: next.name.clone(), delta_ms, test,
    });
    if test { return; } // modo prueba: solo feedback visual, sin acciones

    let report = actions::activate(prev.as_ref().filter(|p| p.id != next.id), &next);
    if let Some(u) = report.undo_layout { *s.undo.lock().unwrap() = Some(u); }
    {
        let mut cfg = s.config.lock().unwrap();
        cfg.settings.active_profile_id = Some(next.id.clone());
        let _ = s.save(&cfg);
    }
    let _ = app.emit("profile-activated", (&next.id, &report.warnings));
    rebuild_tray_menu(app);
}

// ── Tray ─────────────────────────────────────────────────────────────────────
fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let s = app.state::<AppState>();
    let cfg = s.config.lock().unwrap();
    let menu = Menu::new(app)?;
    for p in cfg.profiles.iter().filter(|p| p.enabled) {
        let active = cfg.settings.active_profile_id.as_deref() == Some(p.id.as_str());
        let label = format!("{} {}{}", p.icon.clone().unwrap_or_default(), p.name, if active { "  ✓" } else { "" });
        menu.append(&MenuItem::with_id(app, format!("profile:{}", p.id), label, true, None::<&str>)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "undo", "↶ Deshacer último cambio de layout", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "settings", "Ajustes…", true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?)?;
    Ok(menu)
}

fn rebuild_tray_menu(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(menu) = build_menu(app) { let _ = tray.set_menu(Some(menu)); }
        let _ = tray.set_tooltip(Some(tray_tooltip(app)));
    }
}

/// "ContextSwitch · Gaming" con el modo activo; sin modo, el lema de privacidad.
fn tray_tooltip(app: &AppHandle) -> String {
    let s = app.state::<AppState>();
    let cfg = s.config.lock().unwrap();
    cfg.settings.active_profile_id.as_ref()
        .and_then(|id| cfg.profiles.iter().find(|p| &p.id == id))
        .map(|p| format!("ContextSwitch · {}", p.name))
        .unwrap_or_else(|| "ContextSwitch · 100% local".into())
}

fn main() {
    // Antes que nada: si ya había una instancia, se cierra y esta toma su lugar
    // (así nunca hay dos hooks de teclado ni dos íconos en la bandeja).
    let args: Vec<String> = std::env::args().collect();
    // `ContextSwitch.exe --activate <id>`: arranca (reemplazando a la instancia anterior)
    // y activa ese modo. Útil para accesos directos y pruebas.
    let activate_on_start: Option<String> =
        args.iter().position(|a| a == "--activate").and_then(|i| args.get(i + 1).cloned());
    // `--autostart`: la abrió Windows al iniciar sesión → ventanita de modos, no Ajustes.
    let autostart = args.iter().any(|a| a == "--autostart");

    // Reabrir la app (mismo exe) mientras ya corre en segundo plano: la que corre
    // muestra los Ajustes y esta sale. No se reinicia nada.
    if activate_on_start.is_none() && !autostart && single_instance::ask_running_to_show() {
        return;
    }
    let quit_event = single_instance::take_over();
    let show_event = single_instance::show_event();

    let path = config_path();
    let rec = Config::load_or_recover(&path);
    let mut startup_warnings = Vec::new();
    match (&rec.backup, rec.persist) {
        (Some(b), _) => startup_warnings.push(format!(
            "profiles.json estaba dañado: se guardó una copia en {} y se arrancó con la configuración por defecto.",
            b.display())),
        (None, false) => startup_warnings.push(
            "profiles.json está dañado y no se pudo apartar: la app no va a guardar cambios hasta que lo arregles.".into()),
        _ => {}
    }
    let (config, persist) = (rec.config, rec.persist);
    register_zen_host();
    if config.settings.start_with_windows { refresh_autostart(); }

    tauri::Builder::default()
        .manage(AppState {
            path,
            config: Mutex::new(config),
            hook: Mutex::new(None),
            undo: Mutex::new(None),
            test_mode: AtomicBool::new(false),
            persist: AtomicBool::new(persist),
            startup_warnings,
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            // Mac: sin ícono en el Dock; vive en la barra de menú (como la bandeja de Windows).
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            if let Some(id) = activate_on_start.clone() {
                let h = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(800)); // tray + hook listos
                    actions::ensure_com();
                    let _ = activate_profile(h, id);
                });
            }

            // Si se abre otra instancia, esta se cierra limpia (el ícono de la bandeja se quita).
            if let Some(ev) = quit_event {
                let h = handle.clone();
                std::thread::Builder::new().name("cs-single-instance".into()).spawn(move || {
                    single_instance::wait_quit(ev);
                    h.exit(0);
                })?;
            }

            // Otra copia del mismo exe pidió mostrar la interfaz: Ajustes completos, sin la ventanita.
            if let Some(ev) = show_event {
                let h = handle.clone();
                std::thread::Builder::new().name("cs-show".into()).spawn(move || loop {
                    single_instance::wait_show(ev);
                    if let Some(w) = h.get_webview_window("picker") { let _ = w.close(); }
                    show_settings(&h);
                })?;
            }

            // Hook + worker: el hook solo envía Fire; el worker ejecuta acciones.
            let (tx, rx) = mpsc::channel::<Fire>();
            let (b, c) = hotkey::bindings_from(&app.state::<AppState>().config.lock().unwrap());
            *app.state::<AppState>().hook.lock().unwrap() = Some(HookHandle::start(b, c, tx)?);

            let worker_handle = handle.clone();
            std::thread::Builder::new().name("cs-worker".into()).spawn(move || {
                actions::ensure_com();
                for fire in rx {
                    run_activation(&worker_handle, fire.profile, fire.delta_ms, true);
                }
            })?;

            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip(tray_tooltip(&handle))
                .menu(&build_menu(&handle)?)
                .on_menu_event(|app, ev| match ev.id.as_ref() {
                    "quit" => { actions::reveal_hidden_windows(); app.exit(0); }
                    "settings" => show_settings(app),
                    "undo" => { undo_layout(app.state::<AppState>()); }
                    id if id.starts_with("profile:") => {
                        let pid = id.trim_start_matches("profile:").to_string();
                        let app = app.clone();
                        std::thread::spawn(move || { actions::ensure_com(); let _ = activate_profile(app, pid); });
                    }
                    _ => {}
                })
                .build(app)?;

            if autostart {
                // Según el ajuste: ventanita de modos, o nada (oculta en la bandeja).
                let ask = app.state::<AppState>().config.lock().unwrap().settings.show_startup_picker;
                if ask { open_startup_picker(&handle)?; }
            } else if activate_on_start.is_none() {
                show_settings(&handle); // abierta a mano: interfaz completa
            }
            Ok(())
        })
        .on_window_event(|w, ev| {
            // Ajustes se oculta (la app sigue en la bandeja); la ventanita del arranque sí se cierra.
            if let WindowEvent::CloseRequested { api, .. } = ev {
                if w.label() == "main" { api.prevent_close(); let _ = w.hide(); }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_config, get_startup_warnings, save_config, list_audio_devices, list_running_apps, app_icons, list_monitors, zen_bridge_connected,
            capture_layout, undo_layout, set_test_mode, activate_profile, picker_choose, picker_open_settings,
            app_version, check_update, install_update, platform_name, accessibility_ok, open_accessibility_settings
        ])
        .build(tauri::generate_context!())
        .expect("error al iniciar ContextSwitch")
        .run(|_app, _ev| {
            // Mac: abrir la app desde el Finder/Launchpad mientras ya corre no crea otro
            // proceso: macOS le avisa a la que corre (Reopen) → mostrar los Ajustes.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _ev { show_settings(_app); }
        });
}
