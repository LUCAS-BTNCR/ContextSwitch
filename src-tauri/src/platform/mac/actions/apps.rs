//! Apps de Mac: cuáles corren, abrir, enfocar, ocultar y salir. Solo APIs públicas
//! (NSWorkspace / NSRunningApplication) y `/usr/bin/open` (Launch Services).
//!
//! Identidad de una app = nombre de su bundle sin ".app" ("Google Chrome", "Discord"),
//! que es lo que se guarda en `processName`; `exePath` es la ruta a la .app.

use objc2_app_kit::{NSApplicationActivationOptions, NSApplicationActivationPolicy, NSRunningApplication, NSWorkspace};
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct RunningApp {
    /// "Google Chrome" (nombre del bundle sin .app).
    pub name: String,
    /// "/Applications/Google Chrome.app"
    pub path: String,
    pub bundle_id: String,
    pub pid: i32,
    pub hidden: bool,
}

/// Nombre de app a partir de su ruta: "/Applications/Google Chrome.app" → "Google Chrome".
pub fn name_from_path(path: &str) -> String {
    let file = path.trim_end_matches('/').rsplit('/').next().unwrap_or(path);
    file.strip_suffix(".app").unwrap_or(file).to_string()
}

/// ¿`app` es la que indica `wanted`? Por nombre de app o por bundle id, sin mayúsculas.
/// Tolera reglas viejas/copiadas de Windows ("Discord.exe").
pub fn matches(app: &RunningApp, wanted: &str) -> bool {
    let w = wanted.trim();
    let w = w.strip_suffix(".exe").or_else(|| w.strip_suffix(".app")).unwrap_or(w);
    !w.is_empty() && (app.name.eq_ignore_ascii_case(w) || app.bundle_id.eq_ignore_ascii_case(w))
}

/// Apps "normales" (con ícono en el Dock), sin la propia.
pub fn running() -> Vec<RunningApp> { objc2::rc::autoreleasepool(|_| running_inner()) }

fn running_inner() -> Vec<RunningApp> {
    let me = std::process::id() as i32;
    let ws = NSWorkspace::sharedWorkspace();
    let mut out = Vec::new();
    for app in ws.runningApplications().iter() {
        if app.activationPolicy() != NSApplicationActivationPolicy::Regular || app.isTerminated() { continue; }
        let pid = app.processIdentifier();
        if pid == me { continue; }
        let Some(path) = app.bundleURL().and_then(|u| u.path()).map(|p| p.to_string()) else { continue };
        out.push(RunningApp {
            name: name_from_path(&path),
            bundle_id: app.bundleIdentifier().map(|s| s.to_string()).unwrap_or_default(),
            pid,
            hidden: app.isHidden(),
            path,
        });
    }
    out
}

pub fn find(process_name: &str) -> Vec<RunningApp> {
    running().into_iter().filter(|a| matches(a, process_name)).collect()
}

pub fn is_running(process_name: &str) -> bool { !find(process_name).is_empty() }

fn ns_app(pid: i32) -> Option<objc2::rc::Retained<NSRunningApplication>> {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
}

/// Trae la app al frente (y la muestra si estaba oculta).
pub fn activate(pid: i32) {
    objc2::rc::autoreleasepool(|_| if let Some(a) = ns_app(pid) {
        a.unhide();
        #[allow(deprecated)]
        a.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
    })
}

/// ⌘H: oculta todas sus ventanas. Es la forma Mac de "minimizar la app".
pub fn hide(pid: i32) { objc2::rc::autoreleasepool(|_| if let Some(a) = ns_app(pid) { a.hide(); }) }

/// ⌘Q amable: la app recibe "salir" y decide (si hay algo sin guardar, pregunta).
/// Nunca se fuerza (forceTerminate), igual que en Windows nunca se usa TerminateProcess.
pub fn quit(pid: i32) -> bool { objc2::rc::autoreleasepool(|_| ns_app(pid).map(|a| a.terminate()).unwrap_or(false)) }

/// `open -a <app> [destinos…] [--args args…]`. Si la app ya corre, Launch Services la
/// activa y le pasa los destinos (archivos, carpetas, URLs); los `args` solo cuentan al
/// lanzarla. `new_instance` = `open -n` (otra instancia aunque ya corra).
pub fn open(app_path: &str, targets: &[String], args: &[String], new_instance: bool) -> bool {
    let mut c = Command::new("/usr/bin/open");
    if new_instance { c.arg("-n"); }
    c.arg("-a").arg(app_path).args(targets.iter().filter(|t| !t.trim().is_empty()));
    if !args.is_empty() { c.arg("--args").args(args); }
    c.status().map(|s| s.success()).unwrap_or(false)
}

/// Espera hasta `max` a que la app tenga un proceso; devuelve su pid.
pub fn wait_running(process_name: &str, max: Duration) -> Option<i32> {
    let start = Instant::now();
    loop {
        if let Some(a) = find(process_name).first() { return Some(a.pid); }
        if start.elapsed() > max { return None; }
        std::thread::sleep(Duration::from_millis(150));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, bundle: &str) -> RunningApp {
        RunningApp { name: name.into(), path: format!("/Applications/{name}.app"), bundle_id: bundle.into(), pid: 1, hidden: false }
    }

    #[test]
    fn nombre_desde_la_ruta_de_la_app() {
        assert_eq!(name_from_path("/Applications/Google Chrome.app"), "Google Chrome");
        assert_eq!(name_from_path("/Applications/Vivaldi.app/"), "Vivaldi");
        assert_eq!(name_from_path("/Users/x/Applications/Chrome Apps.localized/YouTube.app"), "YouTube");
    }

    #[test]
    fn coincide_por_nombre_o_bundle_sin_mayusculas() {
        let c = app("Google Chrome", "com.google.Chrome");
        assert!(matches(&c, "google chrome"));
        assert!(matches(&c, "com.google.chrome"));
        assert!(matches(&c, "Google Chrome.app"));
        assert!(!matches(&c, "Chrome"));
        assert!(!matches(&c, ""));
        assert!(matches(&app("Discord", "com.hnc.Discord"), "Discord.exe"), "regla traída de Windows");
    }
}
