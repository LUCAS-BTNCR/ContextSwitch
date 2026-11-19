pub mod audio;
pub mod bridge;
pub mod icons;
pub mod layout;
pub mod placement;
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

    // 1) Lista de exclusión: cierre amable.
    for proc_name in &next.close_on_enter {
        window_ops::close_gracefully(proc_name);
    }

    // 1b) "Solo las apps de este modo": cierre amable (WM_CLOSE) de todo lo demás.
    //      Va antes de lanzar nada, así las apps nuevas nunca reciben el cierre.
    if next.close_others {
        let own = std::env::current_exe().ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        let mut targets: Vec<String> = windows_enum::top_windows().into_iter()
            .filter(|w| !w.cloaked)
            .map(|w| w.process_name)
            .filter(|p| !keeps_open(next, p, own.as_deref()))
            .collect();
        targets.sort();
        targets.dedup();
        for p in &targets { window_ops::close_gracefully(p); }
    }

    // 2) Acción al salir del perfil anterior (si el nuevo no usa esa app).
    if let Some(prev) = prev {
        for app in prev.apps.iter().filter(|a| !uses(&a.process_name)) {
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

fn remembered() -> &'static Mutex<HashMap<String, isize>> {
    static M: OnceLock<Mutex<HashMap<String, isize>>> = OnceLock::new();
    M.get_or_init(Default::default)
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
        for i in 0..app.open.len() {
            match mem.get(&key(i)).copied().filter(|h| alive.contains(h)) {
                Some(h) => {
                    let hwnd = HWND(h as _);
                    match place_of(i) {
                        Some(p) => jobs.push(PlaceJob { target: Target::Hwnd(hwnd), place: p }),
                        None => window_ops::focus_hwnd(hwnd),
                    }
                }
                None => { mem.remove(&key(i)); missing.push(i); }
            }
        }
    }
    if missing.is_empty() { return false; }

    // Zen cerrado: se abre solo (sin URLs) y se espera a que restaure su sesión, así
    // esas ventanas no se confunden con las nuevas.
    let mut launched = false;
    if alive.is_empty() {
        window_ops::shell_open(&app.exe_path, "");
        launched = true;
        wait_until(Duration::from_secs(15), || !zen_hwnds(&proc).is_empty());
        thread::sleep(Duration::from_millis(2000));
        bridge::wait_ready(5000);
    }

    let before: HashSet<isize> = zen_hwnds(&proc).into_iter().collect();
    let urls: Vec<String> = missing.iter().map(|&i| app.open[i].clone()).collect();
    let container = app.container.as_deref().or(app.workspace.as_deref());

    // Con la extensión: ventanas en el contenedor del workspace. Sin ella: línea de comandos.
    let mut found = Vec::new();
    if bridge::send(&bridge::ZenCommand::OpenWindows { container, urls: &urls }).is_ok() {
        found = collect_new(&proc, &before, urls.len(), Duration::from_secs(4 + urls.len() as u64));
    }
    if found.len() < urls.len() {
        if found.is_empty() {
            r.warnings.push("Zen: extensión companion no conectada; ventanas abiertas sin workspace.".into());
        }
        let seen: HashSet<isize> = before.iter().chain(found.iter()).copied().collect();
        for u in &urls[found.len()..] {
            let arg = window_ops::quote_args(std::slice::from_ref(u));
            window_ops::shell_open(&app.exe_path, &format!("-new-window {arg}"));
            thread::sleep(Duration::from_millis(700));
        }
        found.extend(collect_new(&proc, &seen, urls.len() - found.len(), Duration::from_secs(8)));
    }

    let mut mem = remembered().lock().unwrap();
    for (&i, &h) in missing.iter().zip(found.iter()) {
        mem.insert(key(i), h);
        if let Some(p) = place_of(i) { jobs.push(PlaceJob { target: Target::Hwnd(HWND(h as _)), place: p }); }
    }
    drop(mem);

    if app.unload_inactive_tabs { let _ = bridge::send(&bridge::ZenCommand::UnloadInactive); }
    launched
}

/// Procesos del sistema que nunca se cierran con "cerrar todo lo demás":
/// cerrarlos rompe la barra de tareas, el menú Inicio, el teclado táctil o la pantalla de bloqueo.
const NEVER_CLOSE: &[&str] = &[
    "explorer.exe", "contextswitch.exe", "cs-bridge.exe",
    "shellexperiencehost.exe", "startmenuexperiencehost.exe", "searchhost.exe", "searchapp.exe",
    "textinputhost.exe", "lockapp.exe", "dwm.exe", "taskmgr.exe",
];

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

    if app.kind == AppKind::BrowserZen && app.separate_windows && !app.open.is_empty() {
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
        let running = !windows_enum::windows_of(&app.process_name).is_empty();
        if running {
            let sent = bridge::send(&bridge::ZenCommand::Activate {
                workspace: app.workspace.as_deref(),
                container: app.container.as_deref().or(app.workspace.as_deref()),
                urls: &app.open,
                unload_inactive: app.unload_inactive_tabs,
            });
            if sent.is_err() {
                r.warnings.push("Zen: extensión companion no conectada; se abren las URLs sin workspace.".into());
                for u in &app.open { window_ops::shell_open(&app.exe_path, u); }
            }
            window_ops::launch_or_focus(&app.process_name, &app.exe_path, &[], false);
            return false;
        }
        // No estaba abierto: se lanza; la extensión aplicará el workspace al conectarse
        // en el próximo cambio. Las URLs se pasan como argumentos.
        let mut args = app.args.clone();
        args.extend(app.open.iter().cloned());
        window_ops::launch_or_focus(&app.process_name, &app.exe_path, &args, true);
        return true;
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
}
