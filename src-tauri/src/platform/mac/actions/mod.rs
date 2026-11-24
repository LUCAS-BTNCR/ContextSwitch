//! Acciones de un modo en Mac. Misma secuencia que Windows (ver actions/win/mod.rs):
//! cerrar → salir del modo anterior → audio → apps → posiciones → layout → sesión.
//!
//! Diferencias de Mac:
//! - "Cerrar" una app = ⌘Q amable (la app pregunta si hay algo sin guardar).
//! - "Minimizar / liberar RAM" = ⌘H (ocultar la app), la forma Mac de sacarla de encima.
//! - Navegadores sin workspaces (Chrome, Vivaldi, Brave, Edge, Firefox…): cada modo tiene
//!   SUS ventanas. Se abren con las URLs del modo, se recuerdan por su CGWindowID y al
//!   salir del modo se minimizan o se cierran; al volver reaparecen las mismas.

pub mod apps;
pub mod audio;
pub mod ax;
pub mod bridge;
pub mod icons;
pub mod layout;
pub mod placement;
pub mod session;

use crate::profiles::{AppKind, AppRule, EnterAction, ExitAction, LayoutSnapshot, Placement, Profile};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

/// En Windows inicializa COM; en Mac no hace falta.
pub fn ensure_com() {}

#[derive(Default)]
pub struct Report {
    pub undo_layout: Option<LayoutSnapshot>,
    pub warnings: Vec<String>,
}

/// Apps abiertas para el selector de la UI: (nombre, ruta de la .app). Sin títulos de ventana.
pub fn running_apps() -> Vec<(String, String)> {
    apps::running().into_iter().map(|a| (a.name, a.path)).collect()
}

/// Al salir de ContextSwitch no queda nada oculto que haya que devolver: en Mac las
/// ventanas de un modo se minimizan (siguen en el Dock), nunca se ocultan sin rastro.
pub fn reveal_hidden_windows() {}

pub fn activate(prev: Option<&Profile>, next: &Profile) -> Report {
    let mut r = Report::default();
    let uses = |p: &str| next.apps.iter().any(|a| a.process_name.eq_ignore_ascii_case(p));
    if !ax::trusted(false) {
        r.warnings.push("Falta el permiso de Accesibilidad: sin él no se pueden mover, minimizar ni reconocer ventanas. \
                         Ajustes del Sistema → Privacidad y seguridad → Accesibilidad → activá ContextSwitch.".into());
    }

    // 0) "Recordar cómo lo dejé": foto del modo que se deja, antes de cerrar nada.
    if let Some(prev) = prev.filter(|p| p.restore_session) { session::save(&prev.id); }
    let next_session = if next.restore_session { session::load(&next.id) } else { None };
    let session_procs = next_session.as_ref().map(session::processes).unwrap_or_default();

    // 1) Lista de exclusión: salir amable.
    for name in &next.close_on_enter { quit_app(name); }

    // 1b) "Solo las apps de este modo".
    if next.close_others {
        for a in apps::running() {
            if session_procs.contains(&a.name.to_lowercase()) { continue; }
            match close_decision(next, &a.name, &a.bundle_id) {
                Close::Keep => {}
                Close::FinderWindows => for (_, w) in ax::windows_of_pid(a.pid) { w.close(); },
                Close::Quit => { quit_pid(&a); }
            }
        }
    }

    // 2) Salida del modo anterior. Las ventanas de navegador de ese modo son SUYAS: se
    //    tratan aunque el modo nuevo use el mismo navegador, y al final (ver 6).
    let mut window_exits: Vec<(String, AppRule)> = Vec::new();
    if let Some(prev) = prev {
        for app in &prev.apps {
            if uses_mode_windows(app) {
                window_exits.push((prev.id.clone(), app.clone()));
                continue;
            }
            if uses(&app.process_name) { continue; }
            match app.on_exit {
                ExitAction::Keep => {}
                ExitAction::Minimize => {
                    for a in apps::find(&app.process_name) { apps::hide(a.pid); }
                    if app.kind == AppKind::BrowserZen { let _ = bridge::send(&bridge::ZenCommand::UnloadInactive); }
                }
                ExitAction::Close => quit_app(&app.process_name),
            }
        }
    }

    // 3) Audio.
    if let Some(a) = &next.audio {
        if let Err(e) = audio::set_default_output(&a.output_device_id, a.set_communications_too) {
            r.warnings.push(format!("Audio '{}': {e}", a.output_device_name));
        }
    }

    // 4) Apps. Cada una anota qué ventanas ubicar y dónde.
    let mut launched_any = false;
    let mut jobs: Vec<PlaceJob> = Vec::new();
    for app in &next.apps {
        launched_any |= enter_app(&next.id, app, &mut r, &mut jobs);
    }

    // 4b) Posiciones directas. El estado previo queda para "Deshacer".
    if !jobs.is_empty() {
        r.undo_layout = Some(layout::capture("undo".into()));
        let mons = placement::monitors();
        for job in &jobs {
            let win = match &job.target {
                Target::Window(w) => Some(w.clone()),
                Target::MainOf { process, launched } => main_window(process, *launched),
            };
            let (Some(win), Some(rect)) = (win, placement::target_rect(&job.place, &mons)) else { continue };
            placement::place(&win, rect);
            win.raise();
        }
    }

    // 5) Layout capturado.
    if let Some(l) = &next.layout {
        if launched_any && jobs.is_empty() { thread::sleep(Duration::from_millis(1500)); }
        let before = layout::apply(l);
        r.undo_layout.get_or_insert(before);
    }

    // 5b) "Recordar cómo lo dejé".
    if let Some(s) = &next_session {
        r.undo_layout.get_or_insert_with(|| layout::capture("undo".into()));
        let reopened = session::restore(s);
        if !reopened.is_empty() {
            r.warnings.push(format!("Sesión restaurada: se reabrieron {}.", reopened.join(", ")));
        }
    }

    // 6) Ventanas de navegador del modo anterior: al final, así no se minimiza una
    //    ventana que el modo nuevo acaba de traer al frente.
    for (pid, app) in &window_exits { exit_mode_windows(pid, app); }
    r
}

// ── Salir de apps ────────────────────────────────────────────────────────────

/// Apps que nunca se cierran: se ocultan. Zen: cerrar sus ventanas borra los Essentials.
const NEVER_QUIT: &[&str] = &["zen", "app.zen-browser.zen"];

fn never_quit(name: &str, bundle: &str) -> bool {
    NEVER_QUIT.iter().any(|p| p.eq_ignore_ascii_case(name) || p.eq_ignore_ascii_case(bundle))
}

fn quit_pid(a: &apps::RunningApp) {
    if never_quit(&a.name, &a.bundle_id) { apps::hide(a.pid); } else { apps::quit(a.pid); }
}

fn quit_app(name: &str) {
    for a in apps::find(name) { quit_pid(&a); }
}

#[derive(Debug, PartialEq)]
enum Close { Keep, Quit, FinderWindows }

/// Con "cerrar todo lo demás": el Finder nunca sale (es el escritorio), pero sus
/// ventanas sí se cierran, como las carpetas del Explorador en Windows.
fn close_decision(next: &Profile, name: &str, bundle: &str) -> Close {
    let is_it = |p: &str| {
        let p = p.trim().trim_end_matches(".exe").trim_end_matches(".app");
        p.eq_ignore_ascii_case(name) || p.eq_ignore_ascii_case(bundle)
    };
    let user_keeps = next.apps.iter().any(|a| is_it(&a.process_name)) || next.keep_open.iter().any(|k| is_it(k));
    if user_keeps || name.eq_ignore_ascii_case("ContextSwitch") { return Close::Keep; }
    if bundle == "com.apple.finder" { return Close::FinderWindows; }
    Close::Quit
}

// ── Entrar a una app ─────────────────────────────────────────────────────────

struct PlaceJob { target: Target, place: Placement }
enum Target {
    Window(ax::Element),
    MainOf { process: String, launched: bool },
}

/// URL web completa: descarta "", "https://" y similares.
fn is_web_url(u: &str) -> bool {
    let u = u.trim();
    let rest = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://"));
    rest.map(|r| r.split(['/', '?', '#']).next().unwrap_or("")).is_some_and(|host| !host.is_empty())
}

/// ¿La regla abre ventanas propias del modo? (Navegador o Zen con "una ventana por URL").
fn uses_mode_windows(app: &AppRule) -> bool {
    let has_urls = app.open.iter().any(|u| is_web_url(u));
    has_urls && match app.kind {
        AppKind::Browser => true,
        AppKind::BrowserZen => app.separate_windows,
        _ => false,
    }
}

/// Devuelve true si lanzó la app.
fn enter_app(profile_id: &str, app: &AppRule, r: &mut Report, jobs: &mut Vec<PlaceJob>) -> bool {
    if app.on_enter == EnterAction::None { return false; }
    if uses_mode_windows(app) { return mode_windows(profile_id, app, r, jobs); }
    if app.kind == AppKind::BrowserZen { return zen_tabs(app, r); }

    let running = apps::is_running(&app.process_name);
    let force = app.on_enter == EnterAction::Launch;
    // `open -a` activa la app si ya corre (y le pasa carpetas/archivos/URLs); si no, la lanza.
    let ok = apps::open(&app.exe_path, &app.open, if running && !force { &[] } else { &app.args }, force && running);
    if !ok { r.warnings.push(format!("No se pudo abrir {} ({}).", app.label, app.exe_path)); }
    let launched = ok && (!running || force);
    if let Some(p) = app.placements.first().copied().flatten() {
        jobs.push(PlaceJob { target: Target::MainOf { process: app.process_name.clone(), launched }, place: p });
    }
    launched
}

/// Zen en modo pestañas: el workspace lo elige la extensión.
fn zen_tabs(app: &AppRule, r: &mut Report) -> bool {
    let launched = !apps::is_running(&app.process_name);
    if launched {
        apps::open(&app.exe_path, &[], &[], false);
        apps::wait_running(&app.process_name, Duration::from_secs(20));
        bridge::wait_ready(10_000);
    }
    let urls: Vec<String> = app.open.iter().filter(|u| is_web_url(u)).cloned().collect();
    let sent = bridge::wait_ready(3000) && bridge::send(&bridge::ZenCommand::Activate {
        workspace: app.workspace.as_deref(),
        container: app.container.as_deref().or(app.workspace.as_deref()),
        urls: &urls,
        unload_inactive: app.unload_inactive_tabs,
    }).is_ok();
    if !sent { r.warnings.push(zen_missing_warning()); }
    if let Some(a) = apps::find(&app.process_name).first() { apps::activate(a.pid); }
    launched
}

fn zen_missing_warning() -> String {
    "Zen: la extensión ContextSwitch Companion no está conectada, así que no se abrió nada en Zen \
     (no se puede elegir el workspace ni reusar tus Essentials). Instalala o activala en Zen.".into()
}

/// Ventana principal: la más grande de la app. Si recién arranca, espera a que aparezca.
fn main_window(process: &str, launched: bool) -> Option<ax::Element> {
    let biggest = || {
        app_windows(process).into_iter()
            .filter(|(_, w)| !w.is_minimized())
            .max_by_key(|(_, w)| w.size().map(|s| (s.w * s.h) as i64).unwrap_or(0))
            .map(|(_, w)| w)
    };
    if launched {
        wait_until(Duration::from_secs(15), || biggest().is_some());
        thread::sleep(Duration::from_millis(1500)); // splash screens
    }
    biggest()
}

fn app_windows(process: &str) -> Vec<(u32, ax::Element)> {
    apps::find(process).into_iter().flat_map(|a| ax::windows_of_pid(a.pid)).collect()
}

fn wait_until(max: Duration, mut ok: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < max {
        if ok() { return true; }
        thread::sleep(Duration::from_millis(100));
    }
    ok()
}

// ── Ventanas propias de cada modo (navegadores) ──────────────────────────────
// Para reconocer cada ventana no se leen URLs ni títulos (privacidad): se anota qué
// ventanas tenía el navegador antes de pedir una nueva y se toma la que aparece.

/// "perfil|regla|índice" → CGWindowID. En disco, para sobrevivir a un reinicio de la app;
/// al usarlo se verifica que esa ventana siga abierta y sea de ese navegador.
fn remembered() -> &'static Mutex<HashMap<String, u32>> {
    static M: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(load_remembered_from(&windows_file())))
}

fn windows_file() -> std::path::PathBuf { crate::paths::data_dir().join("windows.json") }

fn load_remembered_from(path: &std::path::Path) -> HashMap<String, u32> {
    std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save_remembered_to(path: &std::path::Path, map: &HashMap<String, u32>) -> std::io::Result<()> {
    if let Some(d) = path.parent() { std::fs::create_dir_all(d)?; }
    std::fs::write(path, serde_json::to_string_pretty(map).map_err(std::io::Error::other)?)
}

fn save_remembered(map: &HashMap<String, u32>) { let _ = save_remembered_to(&windows_file(), map); }

/// Qué ventanas abre una regla: (índice, URLs). Con "una ventana por URL", el índice es
/// la posición de la URL en `open` (y de su zona en `placements`); si no, una sola (0).
fn window_slots(app: &AppRule) -> Vec<(usize, Vec<String>)> {
    if app.separate_windows {
        app.open.iter().enumerate().filter(|(_, u)| is_web_url(u)).map(|(i, u)| (i, vec![u.clone()])).collect()
    } else {
        vec![(0, app.open.iter().filter(|u| is_web_url(u)).cloned().collect())]
    }
}

#[derive(Debug, PartialEq)]
enum Family { Chromium, Gecko, Zen }

/// Familia del navegador, por su nombre o ruta. Todo lo que no es Gecko ni Zen se trata
/// como Chromium (Chrome, Vivaldi, Brave, Edge, Opera, Chromium…): todos aceptan --new-window.
fn family(app: &AppRule) -> Family {
    let s = format!("{} {}", app.process_name, app.exe_path).to_lowercase();
    if app.kind == AppKind::BrowserZen || s.contains("zen.app") || s.trim_start().starts_with("zen ") { return Family::Zen; }
    if ["firefox", "librewolf", "waterfox", "floorp", "mullvad browser", "tor browser"].iter().any(|g| s.contains(g)) {
        return Family::Gecko;
    }
    Family::Chromium
}

/// Argumentos para abrir UNA ventana nueva con esas URLs (como pestañas).
fn new_window_args(fam: &Family, urls: &[String]) -> Vec<String> {
    match fam {
        Family::Chromium => std::iter::once("--new-window".to_string()).chain(urls.iter().cloned()).collect(),
        Family::Gecko => {
            let mut v = Vec::new();
            for (i, u) in urls.iter().enumerate() {
                v.push(if i == 0 { "-new-window" } else { "-new-tab" }.to_string());
                v.push(u.clone());
            }
            v
        }
        Family::Zen => vec![],
    }
}

/// Pide una ventana nueva. Chromium/Gecko: una segunda instancia (`open -n`) que le pasa
/// el pedido a la que ya corre y sale. Zen: por la extensión (elige el workspace).
fn open_window(app: &AppRule, fam: &Family, urls: &[String]) -> bool {
    match fam {
        Family::Zen => {
            let container = app.container.as_deref().or(app.workspace.as_deref());
            bridge::wait_ready(1500) && bridge::send(&bridge::ZenCommand::OpenWindows { container, urls }).is_ok()
        }
        _ => apps::open(&app.exe_path, &[], &new_window_args(fam, urls), true),
    }
}

/// Devuelve true si lanzó el navegador.
fn mode_windows(profile_id: &str, app: &AppRule, r: &mut Report, jobs: &mut Vec<PlaceJob>) -> bool {
    let key = |i: usize| format!("{profile_id}|{}|{i}", app.id);
    let place_of = |i: usize| app.placements.get(i).copied().flatten();
    let fam = family(app);
    let slots = window_slots(app);

    // Navegador cerrado: se abre (restaura su sesión) y se espera su primera ventana.
    let mut launched = false;
    if !apps::is_running(&app.process_name) {
        if !apps::open(&app.exe_path, &[], &[], false) {
            r.warnings.push(format!("No se pudo abrir {} ({}).", app.label, app.exe_path));
            return false;
        }
        launched = true;
        apps::wait_running(&app.process_name, Duration::from_secs(20));
        wait_until(Duration::from_secs(8), || !app_windows(&app.process_name).is_empty());
        if fam == Family::Zen { bridge::wait_ready(10_000); }
    }
    if fam == Family::Zen && !bridge::wait_ready(3000) {
        r.warnings.push(zen_missing_warning());
        return launched;
    }

    // Ventanas de una activación anterior que siguen abiertas: se reusan.
    let alive: HashMap<u32, ax::Element> = app_windows(&app.process_name).into_iter().collect();
    let mut missing: Vec<(usize, Vec<String>)> = Vec::new();
    let mut shown: Vec<ax::Element> = Vec::new();
    let mut reused = 0;
    {
        let mut mem = remembered().lock().unwrap();
        for (i, urls) in slots {
            match mem.get(&key(i)).and_then(|id| alive.get(id)) {
                Some(w) => {
                    reused += 1;
                    if w.is_minimized() { w.set_minimized(false); }
                    match place_of(i) {
                        Some(p) => jobs.push(PlaceJob { target: Target::Window(w.clone()), place: p }),
                        None => shown.push(w.clone()),
                    }
                }
                None => { mem.remove(&key(i)); missing.push((i, urls)); }
            }
        }
        save_remembered(&mem);
    }

    // Zen: las que se reusaron se marcan en la extensión como "de un modo", así activar
    // otro modo en pestañas no las usa ni les cambia el workspace.
    if fam == Family::Zen && reused > 0 {
        let container = app.container.as_deref().or(app.workspace.as_deref());
        let _ = bridge::send(&bridge::ZenCommand::AdoptWindows { container, urls: &app.open });
    }

    // De a UNA ventana por vez: se pide, se espera a que aparezca y recién ahí la siguiente.
    // Así cada ventana nueva es inequívocamente la de esas URLs.
    let mut seen: HashSet<u32> = app_windows(&app.process_name).into_iter().map(|(id, _)| id).collect();
    for (i, urls) in missing {
        let mut got: Option<(u32, ax::Element)> = None;
        if open_window(app, &fam, &urls) {
            wait_until(Duration::from_secs(8), || {
                got = app_windows(&app.process_name).into_iter().find(|(id, _)| !seen.contains(id));
                got.is_some()
            });
        }
        match got {
            Some((id, w)) => {
                seen.insert(id);
                let mut mem = remembered().lock().unwrap();
                mem.insert(key(i), id);
                save_remembered(&mem);
                drop(mem);
                match place_of(i) {
                    Some(p) => jobs.push(PlaceJob { target: Target::Window(w), place: p }),
                    None => shown.push(w),
                }
            }
            None => r.warnings.push(format!("{}: no se pudo abrir la ventana de {}.", app.label, urls.join(", "))),
        }
    }

    if fam == Family::Zen && app.unload_inactive_tabs { let _ = bridge::send(&bridge::ZenCommand::UnloadInactive); }
    if let Some(a) = apps::find(&app.process_name).first() { apps::activate(a.pid); }
    for w in &shown { w.raise(); }
    launched
}

/// Al salir de un modo, sus ventanas de navegador:
/// - Minimizar: van al Dock (al volver reaparecen).
/// - Cerrar: se cierran (al volver se abren de nuevo con las URLs del modo). En Zen se
///   minimizan: cerrar una ventana de Zen borra los Essentials que contiene.
/// - Dejar: no se tocan.
fn exit_mode_windows(profile_id: &str, app: &AppRule) {
    if app.on_exit == ExitAction::Keep { return; }
    let alive: HashMap<u32, ax::Element> = app_windows(&app.process_name).into_iter().collect();
    let close = app.on_exit == ExitAction::Close && family(app) != Family::Zen;
    let mut mem = remembered().lock().unwrap();
    let mut any = false;
    for (i, _) in window_slots(app) {
        let k = format!("{profile_id}|{}|{i}", app.id);
        let Some(w) = mem.get(&k).and_then(|id| alive.get(id)) else { continue };
        if close {
            if w.close() { mem.remove(&k); }
        } else {
            w.set_minimized(true);
        }
        any = true;
    }
    save_remembered(&mem);
    drop(mem);
    if any && family(app) == Family::Zen { let _ = bridge::send(&bridge::ZenCommand::UnloadInactive); }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(kind: &str, name: &str, path: &str, open: &[&str], separate: bool) -> AppRule {
        serde_json::from_value(serde_json::json!({
            "id": "r1", "label": name, "kind": kind, "exePath": path, "processName": name,
            "open": open, "separateWindows": separate,
        })).unwrap()
    }

    fn profile() -> Profile {
        serde_json::from_value(serde_json::json!({
            "id": "trabajo", "name": "Trabajo",
            "trigger": { "modifiers": ["ctrl", "alt"], "key": "T", "vk": 84 },
            "apps": [{ "id": "a", "label": "Slack", "kind": "chat", "exePath": "/Applications/Slack.app", "processName": "Slack" }],
            "closeOthers": true,
            "keepOpen": ["Terminal", "com.spotify.client"]
        })).unwrap()
    }

    #[test]
    fn familias_de_navegador() {
        assert_eq!(family(&rule("browser", "Google Chrome", "/Applications/Google Chrome.app", &[], false)), Family::Chromium);
        assert_eq!(family(&rule("browser", "Vivaldi", "/Applications/Vivaldi.app", &[], false)), Family::Chromium);
        assert_eq!(family(&rule("browser", "Brave Browser", "/Applications/Brave Browser.app", &[], false)), Family::Chromium);
        assert_eq!(family(&rule("browser", "Firefox", "/Applications/Firefox.app", &[], false)), Family::Gecko);
        assert_eq!(family(&rule("browser", "Firefox Developer Edition", "/Applications/Firefox Developer Edition.app", &[], false)), Family::Gecko);
        assert_eq!(family(&rule("browser-zen", "Zen", "/Applications/Zen.app", &[], false)), Family::Zen);
    }

    #[test]
    fn argumentos_de_ventana_nueva() {
        let u = vec!["https://a.com/".to_string(), "https://b.com/".to_string()];
        assert_eq!(new_window_args(&Family::Chromium, &u), ["--new-window", "https://a.com/", "https://b.com/"]);
        assert_eq!(new_window_args(&Family::Gecko, &u), ["-new-window", "https://a.com/", "-new-tab", "https://b.com/"]);
    }

    #[test]
    fn una_ventana_por_url_respeta_el_indice_de_open() {
        let r = rule("browser", "Vivaldi", "/Applications/Vivaldi.app", &["https://mail.com/", "https://", "https://cal.com/"], true);
        let s = window_slots(&r);
        assert_eq!(s.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [0, 2], "la URL incompleta no abre ventana pero no corre los índices");
        let r = rule("browser", "Vivaldi", "/Applications/Vivaldi.app", &["https://mail.com/", "https://cal.com/"], false);
        assert_eq!(window_slots(&r), vec![(0, vec!["https://mail.com/".to_string(), "https://cal.com/".to_string()])]);
    }

    #[test]
    fn que_reglas_usan_ventanas_del_modo() {
        assert!(uses_mode_windows(&rule("browser", "Google Chrome", "", &["https://a.com"], false)));
        assert!(!uses_mode_windows(&rule("browser", "Google Chrome", "", &[], false)), "sin URLs = app común");
        assert!(!uses_mode_windows(&rule("browser-zen", "Zen", "", &["https://a.com"], false)), "Zen en pestañas");
        assert!(uses_mode_windows(&rule("browser-zen", "Zen", "", &["https://a.com"], true)));
        assert!(!uses_mode_windows(&rule("generic", "Code", "", &["https://a.com"], true)));
    }

    #[test]
    fn cerrar_todo_lo_demas() {
        let p = profile();
        assert_eq!(close_decision(&p, "Slack", "com.tinyspeck.slackmacgap"), Close::Keep);   // app del modo
        assert_eq!(close_decision(&p, "Terminal", "com.apple.Terminal"), Close::Keep);       // keepOpen por nombre
        assert_eq!(close_decision(&p, "Spotify", "com.spotify.client"), Close::Keep);        // keepOpen por bundle id
        assert_eq!(close_decision(&p, "ContextSwitch", "app.contextswitch"), Close::Keep);   // la propia app
        assert_eq!(close_decision(&p, "Finder", "com.apple.finder"), Close::FinderWindows);  // solo sus ventanas
        assert_eq!(close_decision(&p, "Google Chrome", "com.google.Chrome"), Close::Quit);
    }

    #[test]
    fn zen_nunca_sale() {
        assert!(never_quit("Zen", "app.zen-browser.zen"));
        assert!(!never_quit("Google Chrome", "com.google.Chrome"));
    }

    #[test]
    fn ventanas_recordadas_ida_y_vuelta() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x").join("windows.json");
        assert!(load_remembered_from(&p).is_empty());
        let mut m = HashMap::new();
        m.insert("trabajo|r1|0".to_string(), 4242u32);
        save_remembered_to(&p, &m).unwrap();
        assert_eq!(load_remembered_from(&p), m);
        std::fs::write(&p, "roto").unwrap();
        assert!(load_remembered_from(&p).is_empty(), "archivo dañado = nada recordado");
    }

    #[test]
    fn urls_incompletas_se_ignoran() {
        assert!(is_web_url("https://www.youtube.com/"));
        assert!(!is_web_url("https://"));
        assert!(!is_web_url("/Users/x/proyecto"));
    }
}

/// Pruebas contra la Mac real, SOLO LECTURA (no mueve ni cierra nada):
/// `cargo test -- --ignored mac_real --nocapture`
#[cfg(test)]
mod real {
    use super::*;

    #[test]
    #[ignore]
    fn mac_real_solo_lectura() {
        println!("accesibilidad: {}", ax::trusted(false));
        for a in apps::running() {
            let wins = ax::windows_of_pid(a.pid);
            println!("{:<28} pid {:>6} ventanas {:>2} oculta {}  {}", a.name, a.pid, wins.len(), a.hidden, a.bundle_id);
        }
        for m in placement::monitors() { println!("monitor {:?}", m); }
        for d in audio::list_outputs().unwrap() { println!("audio {:?}", d); }
        println!("layout: {} ventanas", layout::capture("t".into()).windows.len());
    }
}
