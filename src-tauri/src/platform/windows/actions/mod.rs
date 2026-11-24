pub mod audio;
pub mod bridge;
pub mod icons;
pub mod layout;
pub mod placement;
pub mod session;
pub mod window_ops;
pub mod windows_enum;

use crate::profiles::{AppKind, AppRule, EnterAction, ExitAction, LayoutSnapshot, Placement, Profile};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use std::thread;
use windows::Win32::Foundation::HWND;

pub fn ensure_com() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    // S_FALSE / RPC_E_CHANGED_MODE = ya inicializado en este hilo: OK.
    unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
}

/// Apps con ventana para el selector de la UI: (proceso, ruta del .exe). Sin títulos.
pub fn running_apps() -> Vec<(String, String)> {
    windows_enum::top_windows().into_iter().map(|w| (w.process_name, w.exe_path)).collect()
}

#[derive(Default)]
pub struct Report {
    pub undo_layout: Option<LayoutSnapshot>,
    pub warnings: Vec<String>,
}

/// Secuencia de activación. Corre en el hilo worker, nunca en el hook.
pub fn activate(prev: Option<&Profile>, next: &Profile) -> Report {
    ensure_com();
    let mut r = Report::default();
    let uses = |p: &str| next.apps.iter().any(|a| a.process_name.eq_ignore_ascii_case(p));

    // 0) "Recordar cómo lo dejé": foto del modo que se deja, ANTES de cerrar u ocultar nada.
    if let Some(prev) = prev.filter(|p| p.restore_session) { session::save(&prev.id); }
    let next_session = if next.restore_session { session::load(&next.id) } else { None };
    let session_procs = next_session.as_ref().map(session::processes).unwrap_or_default();

    // 1) Lista de exclusión: cierre amable.
    for proc_name in &next.close_on_enter {
        window_ops::close_gracefully(proc_name);
    }

    // 1b) "Solo las apps de este modo": cierre amable (WM_CLOSE) de todo lo demás.
    //      Va antes de lanzar nada, así las apps nuevas nunca reciben el cierre.
    //      Se decide ventana por ventana: explorer.exe es a la vez la barra de tareas y las
    //      carpetas abiertas, y solo las carpetas se cierran.
    if next.close_others {
        let own = std::env::current_exe().ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        let me_elevated = windows_enum::is_elevated(std::process::id());
        let mut skipped_admin: Vec<String> = Vec::new();
        for w in windows_enum::top_windows().into_iter().filter(|w| !w.cloaked) {
            // Las apps de la sesión guardada de este modo se quedan: se van a restaurar.
            if session_procs.contains(&w.process_name) { continue; }
            match close_decision(next, &w.process_name, &w.class_name, own.as_deref()) {
                Close::Keep => {}
                Close::Close if !me_elevated && windows_enum::is_elevated(w.pid) => {
                    if !skipped_admin.contains(&w.process_name) { skipped_admin.push(w.process_name.clone()); }
                }
                Close::Close => window_ops::close_window_of(&w),
            }
        }
        if !skipped_admin.is_empty() {
            r.warnings.push(format!(
                "No se pudo cerrar {} porque se ejecuta como administrador (Windows no deja que una app normal la cierre).",
                skipped_admin.join(", ")));
        }
    }

    // 2) Acción al salir del perfil anterior (si el nuevo no usa esa app).
    let mut zen_exits: Vec<(String, AppRule)> = Vec::new();
    if let Some(prev) = prev {
        for app in &prev.apps {
            // Las ventanas de Zen que abrió ese modo son SUYAS: se tratan aunque el modo
            // nuevo también use Zen (antes se salteaban y quedaban abiertas).
            if app.kind == AppKind::BrowserZen && app.separate_windows {
                zen_exits.push((prev.id.clone(), app.clone())); // se aplica al final (ver 6)
                continue;
            }
            if uses(&app.process_name) { continue; }
            match app.on_exit {
                ExitAction::Keep => {}
                ExitAction::Minimize => {
                    window_ops::minimize(&app.process_name);
                    if app.kind == AppKind::BrowserZen {
                        let _ = bridge::send(&bridge::ZenCommand::UnloadInactive);
                    }
                }
                ExitAction::Close => { window_ops::close_gracefully(&app.process_name); }
            }
        }
    }

    // 3) Audio.
    if let Some(a) = &next.audio {
        if let Err(e) = audio::set_default_output(&a.output_device_id, a.set_communications_too) {
            r.warnings.push(format!("Audio '{}': {e}", a.output_device_name));
        }
    }

    // 4) Apps / workspaces. Cada app anota qué ventanas hay que ubicar y dónde.
    let mut launched_any = false;
    let mut jobs: Vec<PlaceJob> = Vec::new();
    for app in &next.apps {
        launched_any |= enter_app(&next.id, app, &mut r, &mut jobs);
    }

    // 4b) Posiciones directas. El estado previo queda para "Deshacer".
    if !jobs.is_empty() {
        r.undo_layout = Some(layout::capture("undo".into()));
        let mons = placement::monitors();
        let mut placed: Vec<HWND> = Vec::new();
        for job in &jobs {
            let hwnd = match &job.target {
                Target::Hwnd(h) => Some(*h),
                Target::MainOf { process, launched } => main_window(process, *launched),
            };
            let (Some(hwnd), Some(rect)) = (hwnd, placement::target_rect(&job.place, &mons)) else { continue };
            placement::place(hwnd, rect);
            placed.push(hwnd);
        }
        // Traerlas al frente en orden, así quedan todas visibles a la vez.
        for h in &placed { window_ops::focus_hwnd(*h); }
    }

    // 5) Layout capturado (si se lanzaron apps, darles tiempo a crear su ventana).
    if let Some(l) = &next.layout {
        if launched_any && jobs.is_empty() { thread::sleep(Duration::from_millis(1500)); }
        let before = layout::apply(l);
        r.undo_layout.get_or_insert(before);
    }

    // 5b) "Recordar cómo lo dejé": reabrir lo que faltaba y dejar cada ventana donde estaba.
    //     Va después de las posiciones fijas del modo: la sesión guardada manda.
    if let Some(s) = &next_session {
        r.undo_layout.get_or_insert_with(|| layout::capture("undo".into()));
        let reopened = session::restore(s);
        if !reopened.is_empty() {
            r.warnings.push(format!("Sesión restaurada: se reabrieron {}.", reopened.join(", ")));
        }
    }

    // 6) Ventanas de Zen del modo anterior: AL FINAL. Si se ocultaran antes, la extensión
    //    podría volver a mostrar una al enfocar la ventana con la última pestaña usada del
    //    modo nuevo (cada ventana de Zen tiene copias de los Essentials de todos los wf).
    if !zen_exits.is_empty() {
        thread::sleep(Duration::from_millis(700)); // que la extensión termine lo suyo
        for (pid, app) in &zen_exits { exit_zen_windows(pid, app); }
    }
    r
}

/// Una ventana a ubicar en una zona.
struct PlaceJob { target: Target, place: Placement }
enum Target {
    /// Ventana ya identificada (las de Zen que abrimos nosotros).
    Hwnd(HWND),
    /// La ventana principal de la app; si se acaba de lanzar, se espera a que aparezca.
    MainOf { process: String, launched: bool },
}

/// Ventana principal: la más grande del proceso. Si la app recién arranca, espera a que
/// aparezca y le da un momento extra (Discord, Spotify… muestran antes un splash chico).
fn main_window(process: &str, launched: bool) -> Option<HWND> {
    let biggest = || {
        windows_enum::windows_of(process).into_iter()
            .filter(|w| !w.cloaked)
            .max_by_key(|w| window_area(w.hwnd))
            .map(|w| w.hwnd)
    };
    if launched {
        wait_until(Duration::from_secs(15), || biggest().is_some());
        thread::sleep(Duration::from_millis(1500));
    }
    biggest()
}

fn window_area(h: HWND) -> i64 {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
    let mut r = RECT::default();
    unsafe { let _ = GetWindowRect(h, &mut r); }
    (r.right - r.left) as i64 * (r.bottom - r.top) as i64
}

fn wait_until(max: Duration, mut ok: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < max {
        if ok() { return true; }
        thread::sleep(Duration::from_millis(100));
    }
    ok()
}

// ── Zen: una ventana por URL ─────────────────────────────────────────────────
// Para identificar cada ventana no se leen URLs ni títulos (privacidad): se anota qué
// ventanas de Zen había antes de pedirlas y se toman las nuevas en el orden en que
// aparecen. Las identificadas se recuerdan (en memoria) para reubicarlas en la próxima
// activación en vez de abrir otras.

/// Ventanas abiertas por ContextSwitch ("perfil|regla|índice" → HWND). Se guardan en
/// %APPDATA%\ContextSwitch\windows.json para sobrevivir a un reinicio de la app; al usarlas
/// se valida que el HWND siga siendo una ventana viva de ese proceso.
fn remembered() -> &'static Mutex<HashMap<String, isize>> {
    static M: OnceLock<Mutex<HashMap<String, isize>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(load_remembered_from(&windows_file())))
}

fn windows_file() -> std::path::PathBuf {
    std::env::var_os("APPDATA").map(std::path::PathBuf::from).unwrap_or_else(|| ".".into())
        .join("ContextSwitch").join("windows.json")
}

/// Archivo inexistente o dañado = no se recuerda ninguna ventana (nunca falla).
fn load_remembered_from(path: &std::path::Path) -> HashMap<String, isize> {
    std::fs::read_to_string(path).ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_remembered_to(path: &std::path::Path, map: &HashMap<String, isize>) -> std::io::Result<()> {
    if let Some(d) = path.parent() { std::fs::create_dir_all(d)?; }
    std::fs::write(path, serde_json::to_string_pretty(map).map_err(std::io::Error::other)?)
}

fn save_remembered(map: &HashMap<String, isize>) {
    let _ = save_remembered_to(&windows_file(), map);
}

/// Al salir de un modo, sus ventanas de Zen:
/// - Cerrar: se OCULTAN (fuera de pantalla y de la barra de tareas). No se cierran,
///   porque cerrar una ventana de Zen borra sus Essentials. Al volver, reaparecen.
/// - Minimizar: se minimizan.
/// - Dejar: no se tocan.
fn exit_zen_windows(profile_id: &str, app: &AppRule) {
    if app.on_exit == ExitAction::Keep { return; }
    let mem = remembered().lock().unwrap();
    let mut any = false;
    for i in 0..app.open.len() {
        let Some(h) = mem.get(&format!("{profile_id}|{}|{i}", app.id)).copied() else { continue };
        let hwnd = HWND(h as _);
        if !windows_enum::is_window_of(hwnd, &app.process_name) { continue; }
        match app.on_exit {
            ExitAction::Close => window_ops::hide_hwnd(hwnd),
            _ => window_ops::minimize_hwnd(hwnd),
        }
        any = true;
    }
    drop(mem);
    if any { let _ = bridge::send(&bridge::ZenCommand::UnloadInactive); }
}

/// Al salir de ContextSwitch: las ventanas que ocultó vuelven a la barra de tareas
/// (minimizadas), así nunca quedan ventanas de Zen invisibles sin forma de verlas.
pub fn reveal_hidden_windows() {
    let mem = remembered().lock().unwrap();
    for &h in mem.values() {
        let hwnd = HWND(h as _);
        if windows_enum::is_window_of(hwnd, "zen.exe") { window_ops::unhide_hwnd(hwnd, true); }
    }
}

fn zen_hwnds(process: &str) -> Vec<isize> {
    windows_enum::windows_of(process).into_iter().map(|w| w.hwnd.0 as isize).collect()
}

/// Espera ventanas nuevas de `process` que no estén en `before`, en orden de aparición.
fn collect_new(process: &str, before: &HashSet<isize>, want: usize, max: Duration) -> Vec<isize> {
    let mut found: Vec<isize> = Vec::new();
    wait_until(max, || {
        for h in zen_hwnds(process) {
            if !before.contains(&h) && !found.contains(&h) { found.push(h); }
        }
        found.len() >= want
    });
    found.truncate(want);
    found
}

/// URL web completa: descarta "", "https://" y similares (un campo de la UI sin completar).
fn is_web_url(u: &str) -> bool {
    let u = u.trim();
    let rest = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://"));
    rest.map(|r| r.split(['/', '?', '#']).next().unwrap_or("")).is_some_and(|host| !host.is_empty())
}

/// Devuelve true si lanzó Zen.
fn zen_separate_windows(profile_id: &str, app: &AppRule, r: &mut Report, jobs: &mut Vec<PlaceJob>) -> bool {
    let proc = app.process_name.to_lowercase();
    let key = |i: usize| format!("{profile_id}|{}|{i}", app.id);
    let place_of = |i: usize| app.placements.get(i).copied().flatten();
    let alive = zen_hwnds(&proc);

    // Ventanas de una activación anterior que siguen abiertas: se reutilizan.
    let mut missing: Vec<usize> = Vec::new();
    {
        let mut mem = remembered().lock().unwrap();
        for i in (0..app.open.len()).filter(|&i| is_web_url(&app.open[i])) {
            // Viva aunque esté oculta (la ocultamos al salir del modo).
            match mem.get(&key(i)).copied()
                .filter(|h| alive.contains(h) || windows_enum::is_window_of(HWND(*h as _), &proc)) {
                Some(h) => {
                    let hwnd = HWND(h as _);
                    window_ops::unhide_hwnd(hwnd, false);
                    match place_of(i) {
                        Some(p) => jobs.push(PlaceJob { target: Target::Hwnd(hwnd), place: p }),
                        None => window_ops::focus_hwnd(hwnd),
                    }
                }
                None => { mem.remove(&key(i)); missing.push(i); }
            }
        }
        save_remembered(&mem);
    }
    let container = app.container.as_deref().or(app.workspace.as_deref());
    if missing.len() < app.open.len() {
        let _ = bridge::send(&bridge::ZenCommand::AdoptWindows { container, urls: &app.open });
    }
    if missing.is_empty() { return false; }

    let assign = |i: usize, h: isize, jobs: &mut Vec<PlaceJob>| {
        let mut mem = remembered().lock().unwrap();
        mem.insert(key(i), h);
        save_remembered(&mem);
        drop(mem);
        if let Some(p) = place_of(i) { jobs.push(PlaceJob { target: Target::Hwnd(HWND(h as _)), place: p }); }
    };

    // Zen cerrado: se abre solo (restaura su sesión) y se espera a que la extensión conecte.
    // (Por proceso, no por ventanas visibles: con todas sus ventanas ocultas Zen sigue abierto.)
    let mut launched = false;
    if !windows_enum::process_running(&proc) {
        window_ops::shell_open(&app.exe_path, "");
        launched = true;
        wait_until(Duration::from_secs(20), || !zen_hwnds(&proc).is_empty());
        bridge::wait_ready(10_000);
    }

    // SOLO con la extensión. Sin ella no hay forma de elegir el workspace ni de reusar
    // Essentials: abrir igual (zen.exe -new-window) dejaba pestañas nuevas en el workspace
    // equivocado. Mejor no hacer nada y avisar.
    if !bridge::wait_ready(3000) {
        r.warnings.push(
            "Zen: la extensión ContextSwitch Companion no está conectada, así que no se abrió nada en Zen \
             (no se puede elegir el workspace ni reusar tus Essentials). Instalala o activala en Zen.".into());
        return launched;
    }

    // De a UNA ventana por vez: se pide, se espera a que aparezca y recién ahí la siguiente.
    // Así cada ventana nueva es inequívocamente la de esa URL.
    let mut seen: HashSet<isize> = zen_hwnds(&proc).into_iter().collect();
    for &i in &missing {
        let url = std::slice::from_ref(&app.open[i]);
        bridge::wait_ready(1500); // cs-bridge recrea su pipe entre mensajes
        let got = if bridge::send(&bridge::ZenCommand::OpenWindows { container, urls: url }).is_ok() {
            collect_new(&proc, &seen, 1, Duration::from_secs(6)).first().copied()
        } else { None };
        match got {
            Some(h) => { seen.insert(h); assign(i, h, jobs); }
            None => r.warnings.push(format!("Zen: no se pudo abrir la ventana de {}.", app.open[i])),
        }
    }

    if app.unload_inactive_tabs { let _ = bridge::send(&bridge::ZenCommand::UnloadInactive); }
    launched
}

/// Procesos del sistema que nunca se cierran con "cerrar todo lo demás":
/// cerrarlos rompe la barra de tareas, el menú Inicio, el teclado táctil o la pantalla de bloqueo.
const NEVER_CLOSE: &[&str] = &[
    "explorer.exe", "contextswitch.exe", "cs-bridge.exe",
    "shellexperiencehost.exe", "startmenuexperiencehost.exe", "searchhost.exe", "searchapp.exe",
    "textinputhost.exe", "lockapp.exe", "dwm.exe",
];

/// Ventanas de carpeta del Explorador: se cierran aunque explorer.exe esté protegido.
const EXPLORER_FOLDER_CLASSES: &[&str] = &["CabinetWClass", "ExploreWClass"];

#[derive(Debug, PartialEq)]
enum Close { Keep, Close }

fn close_decision(next: &Profile, proc: &str, class_name: &str, own_exe: Option<&str>) -> Close {
    let user_keeps = next.apps.iter().any(|a| a.process_name.eq_ignore_ascii_case(proc))
        || next.keep_open.iter().any(|k| k.eq_ignore_ascii_case(proc));
    if proc == "explorer.exe" && EXPLORER_FOLDER_CLASSES.contains(&class_name) && !user_keeps {
        return Close::Close;
    }
    if keeps_open(next, proc, own_exe) { Close::Keep } else { Close::Close }
}

/// true si `proc` (en minúsculas) debe quedar abierto al activar `next` con close_others.
fn keeps_open(next: &Profile, proc: &str, own_exe: Option<&str>) -> bool {
    NEVER_CLOSE.contains(&proc)
        || own_exe == Some(proc)
        || next.apps.iter().any(|a| a.process_name.eq_ignore_ascii_case(proc))
        || next.keep_open.iter().any(|k| k.eq_ignore_ascii_case(proc))
}

/// Devuelve true si lanzó un proceso nuevo.
fn enter_app(profile_id: &str, app: &AppRule, r: &mut Report, jobs: &mut Vec<PlaceJob>) -> bool {
    if app.on_enter == EnterAction::None { return false; }

    if app.kind == AppKind::BrowserZen && app.separate_windows && app.open.iter().any(|u| is_web_url(u)) {
        return zen_separate_windows(profile_id, app, r, jobs);
    }

    let launched = enter_app_main(app, r);
    if let Some(p) = app.placements.first().copied().flatten() {
        jobs.push(PlaceJob { target: Target::MainOf { process: app.process_name.clone(), launched }, place: p });
    }
    launched
}

/// Apps con una ventana principal (y Zen en modo pestañas). Devuelve true si lanzó.
fn enter_app_main(app: &AppRule, r: &mut Report) -> bool {
    let force = app.on_enter == EnterAction::Launch;

    if app.kind == AppKind::BrowserZen {
        // Zen cerrado: se abre solo (restaura su sesión) y se espera a la extensión.
        let launched = !windows_enum::process_running(&app.process_name);
        if launched {
            window_ops::shell_open(&app.exe_path, "");
            wait_until(Duration::from_secs(20), || !windows_enum::windows_of(&app.process_name).is_empty());
            bridge::wait_ready(10_000);
        }
        // Solo con la extensión (workspace + Essentials). Sin ella no se abre nada:
        // abrir las URLs igual las dejaba en el workspace equivocado.
        let urls: Vec<String> = app.open.iter().filter(|u| is_web_url(u)).cloned().collect();
        let sent = bridge::wait_ready(3000) && bridge::send(&bridge::ZenCommand::Activate {
            workspace: app.workspace.as_deref(),
            container: app.container.as_deref().or(app.workspace.as_deref()),
            urls: &urls,
            unload_inactive: app.unload_inactive_tabs,
        }).is_ok();
        if !sent {
            r.warnings.push(
                "Zen: la extensión ContextSwitch Companion no está conectada, así que no se abrió nada en Zen \
                 (no se puede elegir el workspace ni reusar tus Essentials). Instalala o activala en Zen.".into());
        }
        if !launched { window_ops::launch_or_focus(&app.process_name, &app.exe_path, &[], false); }
        return launched;
    }

    // Si ya corre: se enfoca y cada destino se pasa al exe (ej. `code "C:\carpeta"`).
    // Si hay que lanzarla: los destinos van como argumentos del mismo lanzamiento; abrirlos
    // con ShellExecute(destino) usaría el handler por defecto (una carpeta → Explorador).
    if !force && window_ops::launch_or_focus(&app.process_name, &app.exe_path, &[], false) {
        for target in &app.open {
            window_ops::shell_open(&app.exe_path, &window_ops::quote_args(std::slice::from_ref(target)));
        }
        return false;
    }
    let mut args = app.args.clone();
    args.extend(app.open.iter().cloned());
    window_ops::launch_or_focus(&app.process_name, &app.exe_path, &args, true);
    true
}

#[cfg(test)]
mod tests {
    use super::keeps_open;
    use crate::profiles::Profile;

    fn profile() -> Profile {
        serde_json::from_value(serde_json::json!({
            "id": "gaming", "name": "Gaming",
            "trigger": { "modifiers": ["ctrl", "shift"], "key": "G", "vk": 71 },
            "apps": [{ "id": "a", "label": "Discord", "kind": "chat",
                       "exePath": "C:\\x\\Discord.exe", "processName": "Discord.exe" }],
            "closeOthers": true,
            "keepOpen": ["WindowsTerminal.exe"]
        })).unwrap()
    }

    #[test]
    fn close_others_respeta_apps_shell_y_keep_open() {
        let p = profile();
        assert!(keeps_open(&p, "discord.exe", None));          // app del modo (case-insensitive)
        assert!(keeps_open(&p, "windowsterminal.exe", None));  // keepOpen
        assert!(keeps_open(&p, "explorer.exe", None));         // shell
        assert!(keeps_open(&p, "contextswitch-dev.exe", Some("contextswitch-dev.exe"))); // la propia app
        assert!(!keeps_open(&p, "chrome.exe", None));
        assert!(!keeps_open(&p, "code.exe", Some("contextswitch.exe")));
    }

    #[test]
    fn ventanas_recordadas_ida_y_vuelta_en_disco() {
        use super::{load_remembered_from, save_remembered_to};
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("nueva").join("windows.json");
        assert!(load_remembered_from(&p).is_empty(), "sin archivo = nada recordado");
        let mut m = std::collections::HashMap::new();
        m.insert("trabajo|r1|0".to_string(), 123456isize);
        m.insert("trabajo|r1|1".to_string(), -7isize);
        save_remembered_to(&p, &m).unwrap();
        assert_eq!(load_remembered_from(&p), m);
    }

    #[test]
    fn ventanas_recordadas_archivo_danado_no_rompe() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("windows.json");
        std::fs::write(&p, "[no es un mapa").unwrap();
        assert!(super::load_remembered_from(&p).is_empty());
    }

    #[test]
    fn urls_incompletas_se_ignoran() {
        use super::is_web_url;
        assert!(is_web_url("https://www.youtube.com/"));
        assert!(is_web_url("http://localhost:3000"));
        assert!(!is_web_url("https://"));
        assert!(!is_web_url(""));
        assert!(!is_web_url("https:///path"));
        assert!(!is_web_url("C:\\carpeta"));
    }

    #[test]
    fn explorador_cierra_carpetas_pero_no_la_barra() {
        use super::{close_decision, Close};
        let p = profile();
        assert_eq!(close_decision(&p, "explorer.exe", "CabinetWClass", None), Close::Close);   // carpeta
        assert_eq!(close_decision(&p, "explorer.exe", "Shell_TrayWnd", None), Close::Keep);    // barra de tareas
        assert_eq!(close_decision(&p, "explorer.exe", "Progman", None), Close::Keep);          // escritorio
        assert_eq!(close_decision(&p, "taskmgr.exe", "TaskManagerWindow", None), Close::Close); // ya no está protegido
        assert_eq!(close_decision(&p, "discord.exe", "Chrome_WidgetWin_1", None), Close::Keep); // app del modo
    }
}
